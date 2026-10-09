//! Opt-in native GPU parity and offscreen graph execution.
use analytic_renderer::{
    Camera, Ray,
    field_graph_gpu::{
        GraphGpuIssue, GraphGpuOutcome, GraphGpuRenderer, GraphRenderOptions, GraphTracePolicy,
        WorldFieldGpuOutcome,
    },
};
use spatial_math::{Transform, Vec3};
use std::time::Instant;
use world_authoring::field_graph::{TraceOptions, TraceOutcome, compile_json, trace};
use world_state::{DeterministicSeed, WorldState};

#[test]
#[ignore = "requires native GPU; world-owned graph readback"]
fn world_field_occurrences_query_real_gpu_without_becoming_colliders() {
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
    let source = include_bytes!("../../../examples/field-graph/definitions/twin.json");
    let mut world = WorldState::new(DeterministicSeed(90));
    let left = world
        .spawn_field(source, Transform::new(v(-3.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    let right = world
        .spawn_field(source, Transform::new(v(3.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    assert_eq!(world.field_definitions().len(), 1);
    assert_eq!(
        world_simulation::contact::ContactScene::from_world(&world)
            .unwrap()
            .solid_count(),
        0
    );
    let rays = [
        Ray::new(v(-3.0, 0.0, -6.0), v(0.0, 0.0, 1.0), 0.001, 12.0).unwrap(),
        Ray::new(v(3.0, 0.0, -6.0), v(0.0, 0.0, 1.0), 0.001, 12.0).unwrap(),
    ];
    let query = |world: &WorldState| {
        renderer
            .query_world_fields(
                &device,
                &queue,
                world,
                camera(),
                &rays,
                GraphTracePolicy::default(),
            )
            .unwrap()
    };
    let hits = query(&world);
    assert!(
        matches!(hits[0], WorldFieldGpuOutcome::Hit{id,distance,..} if id==left && (distance-5.38).abs()<0.02)
    );
    assert!(
        matches!(hits[1], WorldFieldGpuOutcome::Hit{id,distance,..} if id==right && (distance-5.38).abs()<0.02)
    );
    world
        .set_field_transform(right, Transform::new(v(-3.0, 0.0, -2.0), 1.0).unwrap())
        .unwrap();
    assert!(matches!(query(&world)[0], WorldFieldGpuOutcome::Hit { id, .. } if id == right));
    world
        .set_field_transform(right, Transform::new(v(-3.0, 0.0, 2.0), 1.0).unwrap())
        .unwrap();
    assert!(matches!(query(&world)[0], WorldFieldGpuOutcome::Hit { id, .. } if id == left));
    world
        .set_field_transform(right, Transform::new(v(-3.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    assert!(matches!(query(&world)[0], WorldFieldGpuOutcome::Hit { id, .. } if id == left));
    world.set_field_enabled(right, false).unwrap();
    assert!(matches!(query(&world)[1], WorldFieldGpuOutcome::Miss));
    world.remove_field(left).unwrap();
    assert!(
        query(&world)
            .iter()
            .all(|outcome| matches!(outcome, WorldFieldGpuOutcome::Miss))
    );
}

#[test]
#[ignore = "requires native GPU; f64 bounds versus f32 query boundary"]
fn world_field_query_preserves_gpu_boundary_diagnostics() {
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
    let source = br#"{"format_version":1,"id":"boundary","root":"s","nodes":[{"op":"sphere","id":"s","radius":1}]}"#;
    let mut world = WorldState::new(DeterministicSeed(91));
    world
        .spawn_field(source, Transform::new(Vec3::ZERO, 1.0).unwrap())
        .unwrap();
    let graph = compile_json(source).unwrap();
    let rays = [
        Ray::new(v(0.0, 1.0 + 1e-8, -4.0), v(0.0, 0.0, 1.0), 0.001, 8.0).unwrap(),
        Ray::new(v(0.0, 2.0, -4.0), v(0.0, 0.0, 1.0), 0.001, 8.0).unwrap(),
    ];
    let direct = renderer
        .query(
            &device,
            &queue,
            &graph,
            camera(),
            &rays,
            GraphTracePolicy::default(),
        )
        .unwrap();
    let world_outcomes = renderer
        .query_world_fields(
            &device,
            &queue,
            &world,
            camera(),
            &rays,
            GraphTracePolicy::default(),
        )
        .unwrap();
    assert_eq!(direct.len(), world_outcomes.len());
    for (direct, world) in direct.iter().zip(world_outcomes) {
        assert_eq!(
            std::mem::discriminant(direct),
            match world {
                WorldFieldGpuOutcome::Miss => std::mem::discriminant(&GraphGpuOutcome::Miss),
                WorldFieldGpuOutcome::Hit { .. } => std::mem::discriminant(&GraphGpuOutcome::Hit {
                    distance: 0.0,
                    normal: [0.0; 3]
                }),
                WorldFieldGpuOutcome::Uncertain { issue, .. } =>
                    std::mem::discriminant(&GraphGpuOutcome::Uncertain(issue)),
                WorldFieldGpuOutcome::Exhausted { .. } =>
                    std::mem::discriminant(&GraphGpuOutcome::Exhausted),
            },
            "direct {direct:?} world {world:?}"
        );
    }
}

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

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn field_graph_gpu_samples_preserve_shared_face_diagnostic() {
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
    let graph = compile_json(br#"{"format_version":1,"id":"shared","root":"u","nodes":[{"op":"box","id":"b","half_extent":[1,1,1]},{"op":"translate","id":"l","child":"b","offset":[-1,0,0]},{"op":"translate","id":"r","child":"b","offset":[1,0,0]},{"op":"union","id":"u","left":"l","right":"r"}]}"#).unwrap();
    assert!(
        renderer
            .sample_points(&device, &queue, &graph, camera(), &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        renderer
            .sample_points(&device, &queue, &graph, camera(), &vec![Vec3::ZERO; 4097])
            .is_err()
    );
    let points = [
        v(-2.01, 0.0, 0.0),
        v(-2.0, 0.0, 0.0),
        v(-1.5, 0.0, 0.0),
        Vec3::ZERO,
        v(0.5, 0.0, 0.0),
        v(2.0, 0.0, 0.0),
        v(2.01, 0.0, 0.0),
    ];
    let gpu = renderer
        .sample_points(&device, &queue, &graph, camera(), &points)
        .unwrap();
    for (&point, observed) in points.iter().zip(&gpu) {
        let cpu = graph.sample(point).unwrap();
        let observed = observed.expect("supported sample must remain finite");
        assert!(
            (cpu - f64::from(observed)).abs() <= 0.0001,
            "point {point:?}: CPU {cpu}, GPU {observed}"
        );
        if cpu.abs() > 0.001 {
            assert_eq!(
                cpu.is_sign_negative(),
                observed.is_sign_negative(),
                "point {point:?}"
            );
        }
    }
    assert_eq!(graph.sample(Vec3::ZERO).unwrap(), 0.0);
    let internal = Ray::new(Vec3::ZERO, v(1.0, 0.0, 0.0), 0.0, 1.0).unwrap();
    assert!(matches!(
        renderer
            .query(
                &device,
                &queue,
                &graph,
                camera(),
                &[internal],
                GraphTracePolicy::default()
            )
            .unwrap()[0],
        GraphGpuOutcome::Uncertain(GraphGpuIssue::UndefinedNormal | GraphGpuIssue::Inside)
    ));
    let transformed = compile_json(br#"{"format_version":1,"id":"scaled","root":"m","nodes":[{"op":"sphere","id":"s","radius":1},{"op":"scale","id":"z","child":"s","factor":2},{"op":"translate","id":"m","child":"z","offset":[3,0,0]}]}"#).unwrap();
    let transformed_points = [
        v(0.9, 0.0, 0.0),
        v(1.0, 0.0, 0.0),
        v(1.00001, 0.0, 0.0),
        v(3.0, 0.0, 0.0),
        v(5.0, 0.0, 0.0),
        v(5.01, 0.0, 0.0),
    ];
    for (&point, observed) in transformed_points.iter().zip(
        renderer
            .sample_points(&device, &queue, &transformed, camera(), &transformed_points)
            .unwrap(),
    ) {
        let cpu = transformed.sample(point).unwrap();
        assert!(
            (cpu - f64::from(observed.unwrap())).abs() <= 0.0001,
            "transformed point {point:?}"
        );
    }
    println!(
        "adapter={} shared_face_gpu_sample={:?}",
        adapter.get_info().name,
        gpu[3]
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

#[test]
#[ignore = "requires native GPU; deterministic 512-sample f64/f32 diagnostic"]
fn field_graph_gpu_seeded_sample_differential() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let mut state = 0x1222_5eed_u64;
    let mut near_zero = 0usize;
    let mut checked = 0usize;
    for leaves in [1, 4, 8, 16] {
        let source = generated_scene(leaves);
        let graph = compile_json(source.as_bytes()).unwrap();
        let mut points = vec![v(-((leaves as f64 - 1.0) * 0.25) - 0.45, 0.0, 0.0)];
        points.extend((0..127).map(|_| {
            let mut component = || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                ((state >> 32) % 1001) as f64 * 0.01 - 5.0
            };
            v(component(), component(), component())
        }));
        let observed = renderer
            .sample_points(&device, &queue, &graph, camera(), &points)
            .unwrap();
        for (&p, gpu) in points.iter().zip(observed) {
            let cpu = graph.sample(p).unwrap();
            let gpu = gpu.expect("bounded scene sample should be finite");
            assert!(
                (cpu - f64::from(gpu)).abs() < 0.002,
                "leaves={leaves} p={p:?} cpu={cpu} gpu={gpu}"
            );
            if cpu.abs() > 0.002 {
                assert_eq!(
                    cpu.is_sign_negative(),
                    gpu.is_sign_negative(),
                    "leaves={leaves} p={p:?}"
                );
            } else {
                near_zero += 1;
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 512);
    assert!(near_zero >= 4);
    println!(
        "adapter={} checked={} near_zero={} sign_mismatches_away_from_threshold=0",
        adapter.get_info().name,
        checked,
        near_zero
    );
}
fn stats(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(f64::total_cmp);
    let p95 = ((values.len() * 95).div_ceil(100)).saturating_sub(1);
    (values[values.len() / 2], values[p95])
}

#[test]
#[ignore = "native release-profile GPU resolution matrix; run serially with --nocapture"]
fn field_graph_resolution_matrix() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let timer = analytic_renderer::GpuTimer::new(&device);
    let simple = compile_json(br#"{"format_version":1,"id":"sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1}]}"#).unwrap();
    let medium8 = compile_json(generated_scene(4).as_bytes()).unwrap();
    let medium16 = compile_json(generated_scene(8).as_bytes()).unwrap();
    let balanced = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/balanced-16.json"
    ))
    .unwrap();
    for graph in [&simple, &medium8, &medium16, &balanced] {
        for size in [[320, 240], [1280, 720], [1920, 1080], [2560, 1440]] {
            let options = || GraphRenderOptions {
                size,
                policy: GraphTracePolicy::default(),
                timer: timer.as_ref(),
            };
            renderer
                .render_rgba_timed(&device, &queue, graph, camera(), options())
                .unwrap();
            let mut full = Vec::new();
            let mut prepare = Vec::new();
            let mut readback = Vec::new();
            let mut compute = Vec::new();
            for _ in 0..5 {
                let frame = renderer
                    .render_rgba_timed(&device, &queue, graph, camera(), options())
                    .unwrap();
                assert_eq!(frame.rgba.len(), (size[0] * size[1] * 4) as usize);
                full.push(frame.full_frame_ms);
                prepare.push(frame.cpu_prepare_ms);
                readback.push(frame.readback_wait_ms);
                if let Some(ms) = frame.gpu_compute_ms {
                    compute.push(ms);
                }
            }
            let row_bytes = size[0] * 4;
            let readback_bytes = row_bytes.div_ceil(256) * 256 * size[1];
            println!(
                "field_resolution adapter={} backend={:?} profile={} nodes={} visits={} size={}x{} full_ms={:?} prepare_ms={:?} readback_wait_ms={:?} gpu_compute_ms={:?} node_bytes={} image_bytes={} readback_bytes={}",
                adapter.get_info().name,
                adapter.get_info().backend,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
                graph.nodes().len(),
                graph.visits(),
                size[0],
                size[1],
                stats(full),
                stats(prepare),
                stats(readback),
                (!compute.is_empty()).then(|| stats(compute)),
                graph.nodes().len() * 32,
                row_bytes * size[1],
                readback_bytes
            );
        }
    }
    assert!(
        renderer
            .render_rgba(
                &device,
                &queue,
                &simple,
                camera(),
                [2561, 1440],
                GraphTracePolicy::default()
            )
            .is_err()
    );
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
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let timer = analytic_renderer::GpuTimer::new(&device);
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
            let mut compute = Vec::new();
            let mut prepare = Vec::new();
            let mut readback = Vec::new();
            for _ in 0..12 {
                let start = Instant::now();
                let _ = renderer
                    .query(&device, &queue, &graph, camera(), &rays, policy)
                    .unwrap();
                query.push(start.elapsed().as_secs_f64() * 1e3);
                let result = renderer
                    .render_rgba_timed(
                        &device,
                        &queue,
                        &graph,
                        camera(),
                        GraphRenderOptions {
                            size: [320, 240],
                            policy,
                            timer: timer.as_ref(),
                        },
                    )
                    .unwrap();
                frame.push(result.full_frame_ms);
                prepare.push(result.cpu_prepare_ms);
                readback.push(result.readback_wait_ms);
                if let Some(ms) = result.gpu_compute_ms {
                    compute.push(ms);
                }
            }
            println!(
                "field_graph_bench adapter={} backend={:?} profile={} nodes={} visits={} budget={} parse_ms={:?} compile_including_parse_ms={:?} cpu_sample_ms={:?} gpu_query_512_readback_ms={:?} gpu_frame_320x240_readback_ms={:?} cpu_prepare_ms={:?} readback_wait_ms={:?} gpu_compute_ms={:?} node_bytes={}",
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
                stats(prepare),
                stats(readback),
                (!compute.is_empty()).then(|| stats(compute)),
                graph.nodes().len() * 32
            );
        }
    }
}
