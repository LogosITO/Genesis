//! Opt-in release-profile timings for the native GPU mixed compute pass.
use analytic_field::Sphere;
use analytic_renderer::{
    Camera, DrawOptions, GpuRenderer, GpuTimer, MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
};
use spatial_math::{Transform, Vec3};
use std::time::Instant;
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

fn graph(nodes_requested: usize) -> Vec<u8> {
    if nodes_requested == 1 {
        return br#"{"format_version":1,"id":"single","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.9}]}"#.to_vec();
    }
    let leaves = nodes_requested / 2;
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
        r#"{{"format_version":1,"id":"mixed-bench","root":"{}","nodes":[{}]}}"#,
        level[0],
        nodes.join(",")
    )
    .into_bytes()
}

fn stats(values: &mut [f64]) -> [f64; 2] {
    values.sort_by(f64::total_cmp);
    [
        values[values.len() / 2],
        values[((values.len() * 95).div_ceil(100)).saturating_sub(1)],
    ]
}

#[test]
#[ignore = "requires native GPU timestamps; release-profile mixed compute matrix"]
fn native_mixed_compute_matrix() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let info = adapter.get_info();
    let resolutions = [[1280, 720], [1920, 1080], [2560, 1440]];
    let cases = [
        ("field", 1),
        ("field", 8),
        ("field", 16),
        ("field", 32),
        ("analytic", 0),
        ("mixed", 8),
        ("mixed", 32),
        ("empty", 32),
        ("occupied", 1),
        ("overlap", 8),
        ("shared", 8),
    ];
    for (kind, nodes_requested) in cases {
        for size in if matches!(kind, "empty" | "occupied" | "overlap" | "shared") {
            &resolutions[1..2]
        } else {
            &resolutions[..]
        } {
            let mut world = WorldState::new(DeterministicSeed(44));
            if kind == "analytic" || kind == "mixed" {
                world
                    .spawn_sphere(
                        Sphere::new(0.8).unwrap(),
                        Transform::new(v(0.5, 0.0, 0.5), 1.0).unwrap(),
                        0.0,
                    )
                    .unwrap();
            }
            if nodes_requested > 0 {
                let source = if kind == "occupied" {
                    br#"{"format_version":1,"id":"large","root":"s","nodes":[{"op":"sphere","id":"s","radius":3}]}"#.to_vec()
                } else {
                    graph(nodes_requested)
                };
                let x = if kind == "empty" { 40.0 } else { 0.0 };
                world
                    .spawn_field(&source, Transform::new(v(x, 0.0, 0.0), 1.0).unwrap())
                    .unwrap();
                if kind == "overlap" || kind == "shared" {
                    let x = if kind == "overlap" { 0.2 } else { 2.0 };
                    world
                        .spawn_field(&source, Transform::new(v(x, 0.0, 0.0), 1.0).unwrap())
                        .unwrap();
                }
            }
            let draw = || {
                let started = Instant::now();
                let _image = renderer
                    .draw_mixed(
                        &device,
                        &queue,
                        &world,
                        camera,
                        MixedDrawOptions {
                            draw: DrawOptions {
                                size: *size,
                                normal_debug: false,
                                surface: None,
                                timer: timer.as_ref(),
                            },
                            policy: GraphTracePolicy::default(),
                            show_analytic: true,
                            show_fields: true,
                            diagnostic: false,
                            materials: None,
                            simulation_seconds: 0.0,
                            background: [0.05, 0.075, 0.115],
                            light_direction: [0.5, 0.8, -0.6],
                        },
                    )
                    .unwrap();
                let cpu_submit_ms = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                let gpu_compute_ms = timer
                    .as_ref()
                    .map(|timer| timer.read_ms(&device, &queue).unwrap());
                let timestamp_wait_ms = started.elapsed().as_secs_f64() * 1000.0;
                (cpu_submit_ms, gpu_compute_ms, timestamp_wait_ms)
            };
            for _ in 0..3 {
                draw();
            }
            let mut cpu = Vec::new();
            let mut gpu = Vec::new();
            let mut wait = Vec::new();
            for _ in 0..7 {
                let (c, g, w) = draw();
                cpu.push(c);
                if let Some(g) = g {
                    gpu.push(g);
                }
                wait.push(w);
            }
            println!(
                "MIXED_BENCH {}",
                serde_json::json!({
                    "adapter": info.name, "backend": format!("{:?}", info.backend), "driver": info.driver,
                    "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                    "kind": kind, "nodes": nodes_requested, "size": size, "warmup": 3, "samples": 7,
                    "cpu_draw_submit_ms": stats(&mut cpu),
                    "gpu_compute_ms": (!gpu.is_empty()).then(|| stats(&mut gpu)),
                    "timestamp_wait_ms": stats(&mut wait),
                })
            );
        }
    }
}
