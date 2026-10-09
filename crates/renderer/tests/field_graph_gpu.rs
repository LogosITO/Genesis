//! Opt-in native GPU parity and offscreen graph execution.
use analytic_renderer::{
    Camera, Ray,
    field_graph_gpu::{GraphGpuOutcome, GraphGpuRenderer, GraphTracePolicy},
};
use spatial_math::Vec3;
use std::time::Instant;
use world_authoring::field_graph::{TraceOptions, TraceOutcome, compile_json, trace};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}
fn camera() -> Camera {
    Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap()
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn field_graph_gpu_readback_and_cpu_parity() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let twin = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/twin.json"
    ))
    .unwrap();
    let plinth = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/plinth.json"
    ))
    .unwrap();
    let policy = GraphTracePolicy::default();
    let rays = [
        Ray::new(v(0.0, 0.0, -6.0), v(0.0, 0.0, 1.0), 0.001, 12.0).unwrap(),
        Ray::new(v(3.0, 0.0, -6.0), v(0.0, 0.0, 1.0), 0.001, 12.0).unwrap(),
        Ray::new(Vec3::ZERO, v(0.0, 0.0, 1.0), 0.001, 12.0).unwrap(),
        Ray::new(v(0.65, 0.0, -6.0), v(0.0, 0.0, 1.0), 0.001, 12.0).unwrap(),
    ];
    for graph in [&twin, &plinth] {
        let gpu = renderer
            .query(&device, &queue, graph, camera(), &rays, policy)
            .unwrap();
        for (ray, result) in rays.iter().zip(&gpu) {
            let (near, far) = ray.interval();
            let cpu = trace(
                graph,
                ray.origin(),
                ray.direction(),
                TraceOptions {
                    near,
                    far,
                    tolerance: f64::from(policy.tolerance),
                    iterations: policy.iterations,
                    safety: f64::from(policy.safety),
                },
            )
            .unwrap();
            match (cpu, result) {
                (
                    TraceOutcome::Hit { distance },
                    GraphGpuOutcome::Hit {
                        distance: gpu_distance,
                        normal,
                    },
                ) => {
                    assert!(
                        (distance - f64::from(*gpu_distance)).abs() < 0.005,
                        "{} CPU {distance} GPU {gpu_distance}",
                        graph.id()
                    );
                    assert!(normal.iter().all(|x| x.is_finite()));
                }
                (TraceOutcome::Miss, GraphGpuOutcome::Miss)
                | (TraceOutcome::Uncertain(_), GraphGpuOutcome::Uncertain(_)) => {}
                (a, b) => panic!("{} CPU {a:?} GPU {b:?}", graph.id()),
            }
        }
    }
    let a = renderer
        .render_rgba(&device, &queue, &twin, camera(), [128, 96], policy)
        .unwrap();
    let b = renderer
        .render_rgba(&device, &queue, &plinth, camera(), [128, 96], policy)
        .unwrap();
    assert_eq!(a.rgba.len(), 128 * 96 * 4);
    assert_ne!(
        a.rgba, b.rgba,
        "external definitions must produce different GPU pixels"
    );
    assert_ne!(
        &a.rgba[(48 * 128 + 64) * 4..(48 * 128 + 64) * 4 + 3],
        &a.rgba[0..3]
    );
    println!(
        "adapter={} twin_full_frame_ms={:.3} plinth_full_frame_ms={:.3}",
        adapter.get_info().name,
        a.full_frame_ms,
        b.full_frame_ms
    );
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn field_graph_gpu_numerical_boundaries() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let policy = GraphTracePolicy {
        tolerance: 0.00001,
        safety: 0.8,
        iterations: 256,
    };
    for radius in [0.01, 1.0, 100.0] {
        let source = format!(
            r#"{{"format_version":1,"id":"edge","root":"ball","nodes":[{{"op":"sphere","id":"ball","radius":{radius}}}]}}"#
        );
        let graph = compile_json(source.as_bytes()).unwrap();
        let ray = Ray::new(
            v(0.0, 0.0, -radius - 1.0),
            v(0.0, 0.0, 1.0),
            0.001,
            radius * 2.0 + 2.0,
        )
        .unwrap();
        let result = renderer
            .query(&device, &queue, &graph, camera(), &[ray], policy)
            .unwrap();
        match result[0] {
            GraphGpuOutcome::Hit { distance, normal } => {
                assert!(
                    (f64::from(distance) - 1.0).abs() < 0.005,
                    "radius {radius} distance {distance}"
                );
                assert!(normal[2] < -0.99);
            }
            other => panic!("radius {radius}: {other:?}"),
        }
    }
    let source = r#"{"format_version":1,"id":"coincident","root":"u","nodes":[{"op":"sphere","id":"ball","radius":1},{"op":"scale","id":"scaled","child":"ball","factor":1},{"op":"translate","id":"moved","child":"scaled","offset":[0,0,0]},{"op":"union","id":"u","left":"ball","right":"moved"}]}"#;
    let graph = compile_json(source.as_bytes()).unwrap();
    let rays = [
        Ray::new(v(1.0, 0.0, -3.0), v(0.0, 0.0, 1.0), 0.001, 6.0).unwrap(),
        Ray::new(v(0.999, 0.0, -3.0), v(0.0, 0.0, 1.0), 0.001, 6.0).unwrap(),
        Ray::new(v(1.001, 0.0, -3.0), v(0.0, 0.0, 1.0), 0.001, 6.0).unwrap(),
    ];
    let results = renderer
        .query(&device, &queue, &graph, camera(), &rays, policy)
        .unwrap();
    assert!(
        !matches!(results[0], GraphGpuOutcome::Miss),
        "tangent cannot be silently discarded"
    );
    assert!(matches!(
        results[1],
        GraphGpuOutcome::Hit { .. } | GraphGpuOutcome::Exhausted
    ));
    assert!(matches!(
        results[2],
        GraphGpuOutcome::Miss | GraphGpuOutcome::Exhausted
    ));
    let limited = renderer
        .query(
            &device,
            &queue,
            &graph,
            camera(),
            &[rays[1]],
            GraphTracePolicy {
                iterations: 1,
                ..policy
            },
        )
        .unwrap();
    assert!(matches!(limited[0], GraphGpuOutcome::Exhausted));
    let mut nodes = vec![r#"{"op":"sphere","id":"tiny","radius":1}"#.to_owned()];
    for i in 0..9 {
        let child = if i == 0 {
            "tiny".to_owned()
        } else {
            format!("scale{}", i - 1)
        };
        nodes.push(format!(
            r#"{{"op":"scale","id":"scale{i}","child":"{child}","factor":0.01}}"#
        ));
    }
    nodes.push(r#"{"op":"sphere","id":"ordinary","radius":1}"#.to_owned());
    nodes.push(r#"{"op":"union","id":"root","left":"scale8","right":"ordinary"}"#.to_owned());
    let source = format!(
        r#"{{"format_version":1,"id":"numeric","root":"root","nodes":[{}]}}"#,
        nodes.join(",")
    );
    let graph = compile_json(source.as_bytes()).unwrap();
    let ray = Ray::new(v(1000.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.001, 10.0).unwrap();
    let outcome = renderer
        .query(&device, &queue, &graph, camera(), &[ray], policy)
        .unwrap();
    assert!(
        matches!(outcome[0], GraphGpuOutcome::Uncertain(_)),
        "invalid child arithmetic must not be hidden by union: {:?}",
        outcome[0]
    );
    println!(
        "edge rays: tangent={:?} near_inside={:?} near_outside={:?}",
        results[0], results[1], results[2]
    );
}

fn generated_scene(leaves: usize) -> String {
    let mut nodes = vec![r#"{"op":"sphere","id":"s","radius":0.45}"#.to_owned()];
    let mut level = Vec::new();
    for i in 0..leaves {
        let x = (i as f64 - (leaves as f64 - 1.0) / 2.0) * 0.5;
        nodes.push(format!(
            r#"{{"op":"translate","id":"t{i}","child":"s","offset":[{x},0,0]}}"#
        ));
        level.push(format!("t{i}"));
    }
    let mut id = 0;
    while level.len() > 1 {
        let mut next = Vec::new();
        for pair in level.chunks(2) {
            let name = format!("u{id}");
            id += 1;
            nodes.push(format!(
                r#"{{"op":"union","id":"{name}","left":"{}","right":"{}"}}"#,
                pair[0], pair[1]
            ));
            next.push(name);
        }
        level = next;
    }
    format!(
        r#"{{"format_version":1,"id":"benchmark","root":"{}","nodes":[{}]}}"#,
        level[0],
        nodes.join(",")
    )
}
fn stats(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(f64::total_cmp);
    let p95 = ((values.len() * 95).div_ceil(100)).saturating_sub(1);
    (values[values.len() / 2], values[p95])
}

#[test]
#[ignore = "local GPU timing; run explicitly with --ignored --nocapture"]
fn field_graph_scaling_measurements() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    for leaves in [1, 4, 8, 16] {
        let source = generated_scene(leaves);
        let graph = compile_json(source.as_bytes()).unwrap();
        let mut parse = Vec::new();
        let mut compile = Vec::new();
        let mut cpu = Vec::new();
        for _ in 0..40 {
            let start = Instant::now();
            let _: serde_json::Value = serde_json::from_slice(source.as_bytes()).unwrap();
            parse.push(start.elapsed().as_secs_f64() * 1e3);
            let start = Instant::now();
            let _ = compile_json(source.as_bytes()).unwrap();
            compile.push(start.elapsed().as_secs_f64() * 1e3);
            let start = Instant::now();
            for x in 0..1000 {
                let _ = graph.sample(v(f64::from(x) * 0.001, 0.1, 1.0)).unwrap();
            }
            cpu.push(start.elapsed().as_secs_f64() * 1e3 / 1000.0);
        }
        for iterations in [32, 128] {
            let policy = GraphTracePolicy {
                iterations,
                ..GraphTracePolicy::default()
            };
            let rays: Vec<_> = (0..512)
                .map(|i| {
                    Ray::new(
                        v((f64::from(i) - 256.0) * 0.01, 0.0, -6.0),
                        v(0.0, 0.0, 1.0),
                        0.001,
                        12.0,
                    )
                    .unwrap()
                })
                .collect();
            let mut query = Vec::new();
            let mut frame = Vec::new();
            for _ in 0..12 {
                let start = Instant::now();
                let _ = renderer
                    .query(&device, &queue, &graph, camera(), &rays, policy)
                    .unwrap();
                query.push(start.elapsed().as_secs_f64() * 1e3);
                frame.push(
                    renderer
                        .render_rgba(&device, &queue, &graph, camera(), [320, 240], policy)
                        .unwrap()
                        .full_frame_ms,
                );
            }
            println!(
                "field_graph_bench adapter={} backend={:?} profile={} nodes={} visits={} budget={} parse_ms={:?} compile_including_parse_ms={:?} cpu_sample_ms={:?} gpu_query_512_readback_ms={:?} gpu_frame_320x240_readback_ms={:?} node_bytes={}",
                adapter.get_info().name,
                adapter.get_info().backend,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
                graph.nodes().len(),
                graph.visits(),
                iterations,
                stats(parse.clone()),
                stats(compile.clone()),
                stats(cpu.clone()),
                stats(query),
                stats(frame),
                graph.nodes().len() * 32
            );
        }
    }
}
