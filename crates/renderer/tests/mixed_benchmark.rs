//! Opt-in release-profile timings for the native GPU mixed compute pass.
use analytic_field::Sphere;
use analytic_renderer::{
    Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
    GpuTimer, MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
};
use spatial_math::{Transform, Vec3};
use std::{process::Command, time::Instant};
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
#[ignore = "requires native GPU timestamps; matched interpreter/compiler and bounds matrix"]
fn native_compiler_matched_matrix() {
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
    let timer = GpuTimer::new(&device).expect("timestamp query required");
    let info = adapter.get_info();
    let (source_commit, source_dirty) = source_state();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let resolutions = [[1280, 720], [1920, 1080], [2560, 1440]];
    let cases = [
        ("field", 1),
        ("field", 8),
        ("field", 16),
        ("field", 32),
        ("empty", 32),
        ("occupied", 1),
        ("overlap", 8),
        ("shared", 8),
        ("mixed", 32),
    ];
    for (kind, nodes_requested) in cases {
        for size in if kind == "field" {
            &resolutions[..]
        } else {
            &resolutions[1..2]
        } {
            let mut world = WorldState::new(DeterministicSeed(401));
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
            if kind == "mixed" {
                world
                    .spawn_sphere(
                        Sphere::new(0.8).unwrap(),
                        Transform::new(v(0.5, 0.0, 0.5), 1.0).unwrap(),
                        0.0,
                    )
                    .unwrap();
            }
            let draw = |execution, guard| {
                let started = Instant::now();
                let _image = renderer
                    .draw_mixed_with_execution(
                        &device,
                        &queue,
                        &world,
                        camera,
                        MixedDrawOptions {
                            draw: DrawOptions {
                                size: *size,
                                normal_debug: false,
                                surface: None,
                                timer: Some(&timer),
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
                        GpuFieldRenderMode {
                            execution,
                            spatial_guard: guard,
                        },
                    )
                    .unwrap();
                let cpu_submit_ms = started.elapsed().as_secs_f64() * 1000.0;
                let gpu_compute_ms = timer.read_ms(&device, &queue).unwrap();
                (cpu_submit_ms, gpu_compute_ms)
            };
            let requests_before = renderer.compiler_stats().unwrap().requests;
            let compile_started = Instant::now();
            for _ in 0..500 {
                draw(GpuFieldExecution::Auto, true);
                let status = renderer.compiler_stats().unwrap();
                assert!(
                    status.last_error.is_none(),
                    "compiler failed: {:?}",
                    status.last_error
                );
                if status.active == GpuFieldPath::Specialized {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let ready_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
            let compiler = renderer.compiler_stats().unwrap();
            assert_eq!(compiler.active, GpuFieldPath::Specialized);
            for (label, execution, guard) in [
                (
                    "A_interpreter_direct",
                    GpuFieldExecution::Interpreter,
                    false,
                ),
                (
                    "B_interpreter_guarded",
                    GpuFieldExecution::Interpreter,
                    true,
                ),
                ("C_specialized_direct", GpuFieldExecution::Auto, false),
                ("D_specialized_guarded", GpuFieldExecution::Auto, true),
            ] {
                for _ in 0..3 {
                    draw(execution, guard);
                }
                let mut cpu = Vec::with_capacity(7);
                let mut gpu = Vec::with_capacity(7);
                for _ in 0..7 {
                    let (c, g) = draw(execution, guard);
                    cpu.push(c);
                    gpu.push(g);
                    if execution == GpuFieldExecution::Auto {
                        assert_eq!(
                            renderer.compiler_stats().unwrap().active,
                            GpuFieldPath::Specialized
                        );
                    }
                }
                println!(
                    "COMPILER_MIXED_BENCH {}",
                    serde_json::json!({
                        "benchmark": "native_compiler_matched_matrix", "source_commit": source_commit,
                        "source_dirty": source_dirty, "definition_hash": world.field_definitions()[0].revision().hex(),
                        "adapter": info.name, "backend": format!("{:?}", info.backend), "driver": info.driver,
                        "os": std::env::consts::OS, "cpu": std::env::var("PROCESSOR_IDENTIFIER").ok(),
                        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                        "kind": kind, "nodes": nodes_requested, "size": size, "mode": label,
                        "warmup": 3, "samples": 7, "gpu_timing_source": "timestamp_query",
                        "gpu_compute_ms": stats(&mut gpu), "cpu_draw_submit_ms": stats(&mut cpu),
                        "compiler_source_ms": compiler.source_ms, "compiler_pipeline_ms": compiler.pipeline_ms,
                        "compiler_source_bytes": compiler.source_bytes, "compile_ready_wall_ms": ready_ms,
                        "compile_requests_for_scene": compiler.requests - requests_before,
                        "cached_pipelines": compiler.cached_pipelines,
                    })
                );
            }
        }
    }
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

fn source_state() -> (Option<String>, Option<bool>) {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&workspace)
            .args(args)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let root = git(&["rev-parse", "--show-toplevel"])
        .and_then(|path| std::path::Path::new(&path).canonicalize().ok());
    if root.as_deref() != Some(workspace.as_path()) {
        return (None, None);
    }
    (
        git(&["rev-parse", "HEAD"]),
        git(&["status", "--porcelain"]).map(|s| !s.is_empty()),
    )
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
    let (source_commit, source_dirty) = source_state();
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
                    .draw_mixed_with_execution(
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
                        GpuFieldRenderMode {
                            execution: GpuFieldExecution::Interpreter,
                            spatial_guard: true,
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
                    "benchmark": "native_mixed_compute_matrix",
                    "source_commit": source_commit, "source_dirty": source_dirty,
                    "definition_hashes": world.field_definitions().iter().map(|d| d.revision().hex()).collect::<Vec<_>>(),
                    "adapter": info.name, "backend": format!("{:?}", info.backend), "driver": info.driver,
                    "os": std::env::consts::OS,
                    "cpu": std::env::var("PROCESSOR_IDENTIFIER").ok(),
                    "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                    "kind": kind, "nodes": nodes_requested, "size": size, "warmup": 3, "samples": 7,
                    "present_mode": "offscreen", "gpu_timing_source": "timestamp_query",
                    "cpu_timing_source": "Instant wall clock",
                    "cpu_draw_submit_ms": stats(&mut cpu),
                    "gpu_compute_ms": (!gpu.is_empty()).then(|| stats(&mut gpu)),
                    "timestamp_wait_ms": stats(&mut wait),
                })
            );
        }
    }
}

#[test]
#[ignore = "requires native GPU; offscreen frame pacing during rapid definition edits"]
fn rapid_definition_edit_latency() {
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
    let timer = GpuTimer::new(&device).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let mut world = WorldState::new(DeterministicSeed(402));
    let baseline = graph(32);
    let id = world.spawn_field(&baseline, Transform::identity()).unwrap();
    let draw = |world: &WorldState| {
        let started = Instant::now();
        let _image = renderer
            .draw_mixed(
                &device,
                &queue,
                world,
                camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size: [1280, 720],
                        normal_debug: false,
                        surface: None,
                        timer: Some(&timer),
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
        let submit_ms = started.elapsed().as_secs_f64() * 1000.0;
        let gpu_ms = timer.read_ms(&device, &queue).unwrap();
        (submit_ms, started.elapsed().as_secs_f64() * 1000.0, gpu_ms)
    };
    for _ in 0..500 {
        draw(&world);
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Specialized
    );
    let mut submit = Vec::new();
    let mut frame = Vec::new();
    let mut gpu = Vec::new();
    let requests_before = renderer.compiler_stats().unwrap().requests;
    for edit in 0..12 {
        let radius = 0.46 + f64::from(edit) * 0.01;
        let source = String::from_utf8(baseline.clone())
            .unwrap()
            .replace("\"radius\":0.45", &format!("\"radius\":{radius:.2}"));
        world.replace_field(id, source.as_bytes()).unwrap();
        let (s, f, g) = draw(&world);
        submit.push(s);
        frame.push(f);
        gpu.push(g);
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Interpreter
        );
    }
    let settle_started = Instant::now();
    for _ in 0..500 {
        draw(&world);
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let compiler = renderer.compiler_stats().unwrap();
    assert_eq!(compiler.active, GpuFieldPath::Specialized);
    println!(
        "COMPILER_EDIT_BENCH {}",
        serde_json::json!({
            "adapter": adapter.get_info().name, "backend": format!("{:?}", adapter.get_info().backend),
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "resolution": [1280,720], "edits": 12,
            "cpu_draw_submit_ms": stats(&mut submit), "offscreen_blocking_frame_ms": stats(&mut frame),
            "gpu_compute_ms": stats(&mut gpu), "final_revision_ready_wall_ms": settle_started.elapsed().as_secs_f64()*1000.0,
            "compiler_requests_during_edits": compiler.requests - requests_before,
            "final_pipeline_ms": compiler.pipeline_ms, "cached_pipelines": compiler.cached_pipelines,
            "timing_note": "blocking offscreen GPU timestamp read, not a VSync presentation interval",
        })
    );
}
