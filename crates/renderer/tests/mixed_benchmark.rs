//! Opt-in release-profile timings for the native GPU mixed compute pass.
use analytic_field::Sphere;
use analytic_renderer::{
    Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
    GpuTimer, MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
};
use spatial_math::{Transform, Vec3};
use std::{process::Command, time::Instant};
use world_state::{DeterministicSeed, EntityId, WorldState};

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
            let structural_started = Instant::now();
            for _ in 0..500 {
                draw(GpuFieldExecution::Structural, true);
                let status = renderer.compiler_stats().unwrap();
                assert!(
                    status.last_error.is_none(),
                    "structural compiler failed: {:?}",
                    status.last_error
                );
                if status.active == GpuFieldPath::Structural {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let structural_ready_ms = structural_started.elapsed().as_secs_f64() * 1000.0;
            let structural = renderer.compiler_stats().unwrap();
            assert_eq!(structural.active, GpuFieldPath::Structural);
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
                ("E_structural_guarded", GpuFieldExecution::Structural, true),
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
                    if execution != GpuFieldExecution::Interpreter {
                        assert_eq!(
                            renderer.compiler_stats().unwrap().active,
                            if execution == GpuFieldExecution::Auto {
                                GpuFieldPath::Specialized
                            } else {
                                GpuFieldPath::Structural
                            }
                        );
                    }
                }
                let (selected, selected_ready_ms) = if execution == GpuFieldExecution::Structural {
                    (&structural, structural_ready_ms)
                } else {
                    (&compiler, ready_ms)
                };
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
                        "compiler_source_ms": selected.source_ms, "compiler_pipeline_ms": selected.pipeline_ms,
                        "compiler_source_bytes": selected.source_bytes, "compile_ready_wall_ms": selected_ready_ms,
                        "compile_requests_for_scene": selected.requests - requests_before,
                        "cached_pipelines": selected.cached_pipelines,
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

#[test]
#[ignore = "requires native GPU timestamps; measured structural parameter edits"]
fn structural_parameter_edit_latency() {
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
    let baseline = String::from_utf8(graph(32)).unwrap();
    let mut world = WorldState::new(DeterministicSeed(403));
    let id = world
        .spawn_field(baseline.as_bytes(), Transform::identity())
        .unwrap();
    let draw = |world: &WorldState| {
        let started = Instant::now();
        let _image = renderer
            .draw_mixed_with_execution(
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
                GpuFieldRenderMode {
                    execution: GpuFieldExecution::Structural,
                    spatial_guard: true,
                },
            )
            .unwrap();
        let submit_ms = started.elapsed().as_secs_f64() * 1000.0;
        let gpu_ms = timer.read_ms(&device, &queue).unwrap();
        (submit_ms, started.elapsed().as_secs_f64() * 1000.0, gpu_ms)
    };
    for _ in 0..500 {
        draw(&world);
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Structural {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Structural
    );
    let requests = renderer.compiler_stats().unwrap().requests;
    for delay_ms in [0, 30] {
        let mut update = Vec::new();
        let mut enqueue = Vec::new();
        let mut submit = Vec::new();
        let mut frame = Vec::new();
        let mut gpu = Vec::new();
        for edit in 0..12 {
            let radius = 0.46 + f64::from(edit + if delay_ms == 0 { 0 } else { 12 }) * 0.01;
            let source = baseline.replace("\"radius\":0.45", &format!("\"radius\":{radius:.2}"));
            let started = Instant::now();
            world.replace_field(id, source.as_bytes()).unwrap();
            update.push(started.elapsed().as_secs_f64() * 1000.0);
            let (s, f, g) = draw(&world);
            submit.push(s);
            frame.push(f);
            gpu.push(g);
            enqueue.push(renderer.field_cache_stats().unwrap().last_node_enqueue_ms);
            assert_eq!(
                renderer.compiler_stats().unwrap().active,
                GpuFieldPath::Structural
            );
            assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
        println!(
            "STRUCTURAL_EDIT_BENCH {}",
            serde_json::json!({
                "adapter": adapter.get_info().name, "backend": format!("{:?}", adapter.get_info().backend),
                "nodes": 32, "size": [1280, 720], "edits": 12, "delay_ms": delay_ms,
                "world_replace_ms": stats(&mut update), "node_enqueue_ms": stats(&mut enqueue),
                "cpu_draw_submit_ms": stats(&mut submit), "offscreen_blocking_frame_ms": stats(&mut frame),
                "gpu_compute_ms": stats(&mut gpu), "compiler_requests_after_ready": renderer.compiler_stats().unwrap().requests - requests,
                "node_uploads": renderer.field_cache_stats().unwrap().node_uploads,
                "node_bytes": renderer.field_cache_stats().unwrap().node_bytes,
            })
        );
    }
}

#[test]
#[ignore = "requires native GPU timestamps; bounded multi-definition world matrix"]
fn multi_definition_matrix() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let timer = GpuTimer::new(&device).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -7.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let (source_commit, source_dirty) = source_state();
    for topology in ["shared", "heterogeneous"] {
        for definitions in [1, 2, 4] {
            let mut world = WorldState::new(DeterministicSeed(404));
            for index in 0..definitions {
                let source = if topology == "shared" {
                    String::from_utf8(graph(8))
                        .unwrap()
                        .replace(
                            "\"radius\":0.45",
                            &format!("\"radius\":{:.2}", 0.45 + index as f64 * 0.1),
                        )
                        .into_bytes()
                } else {
                    graph([1, 4, 8, 16][index])
                };
                let x = (index as f64 - (definitions as f64 - 1.0) / 2.0) * 1.5;
                for y in [-0.3, 0.3] {
                    world
                        .spawn_field(&source, Transform::new(v(x, y, 0.0), 1.0).unwrap())
                        .unwrap();
                }
            }
            assert_eq!(world.field_definitions().len(), definitions);
            let renderer =
                pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm))
                    .unwrap();
            for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
                let draw = |execution| {
                    let started = Instant::now();
                    let _image = renderer
                        .draw_mixed_with_execution(
                            &device,
                            &queue,
                            &world,
                            camera,
                            MixedDrawOptions {
                                draw: DrawOptions {
                                    size,
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
                                spatial_guard: true,
                            },
                        )
                        .unwrap();
                    let cpu = started.elapsed().as_secs_f64() * 1000.0;
                    (cpu, timer.read_ms(&device, &queue).unwrap())
                };
                if topology == "shared" || definitions == 1 {
                    for _ in 0..500 {
                        draw(GpuFieldExecution::Structural);
                        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Structural {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    assert_eq!(
                        renderer.compiler_stats().unwrap().active,
                        GpuFieldPath::Structural
                    );
                }
                if definitions == 1 {
                    for _ in 0..500 {
                        draw(GpuFieldExecution::Auto);
                        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    assert_eq!(
                        renderer.compiler_stats().unwrap().active,
                        GpuFieldPath::Specialized
                    );
                }
                for _ in 0..500 {
                    draw(GpuFieldExecution::Combined);
                    if renderer.compiler_stats().unwrap().active == GpuFieldPath::Combined {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                assert_eq!(
                    renderer.compiler_stats().unwrap().active,
                    GpuFieldPath::Combined,
                    "combined compilation failed: {:?}",
                    renderer.compiler_stats().unwrap().last_error
                );
                for (label, execution) in [
                    ("interpreter", GpuFieldExecution::Interpreter),
                    ("auto", GpuFieldExecution::Auto),
                    ("structural", GpuFieldExecution::Structural),
                    ("combined", GpuFieldExecution::Combined),
                ] {
                    for _ in 0..3 {
                        draw(execution);
                    }
                    let mut cpu = Vec::new();
                    let mut gpu = Vec::new();
                    for _ in 0..7 {
                        let (c, g) = draw(execution);
                        cpu.push(c);
                        gpu.push(g);
                    }
                    let compiler = renderer.compiler_stats().unwrap();
                    assert_eq!(
                        compiler.active,
                        if execution == GpuFieldExecution::Combined {
                            GpuFieldPath::Combined
                        } else if (topology == "shared" || definitions == 1)
                            && execution == GpuFieldExecution::Structural
                        {
                            GpuFieldPath::Structural
                        } else if definitions == 1 && execution == GpuFieldExecution::Auto {
                            GpuFieldPath::Specialized
                        } else {
                            GpuFieldPath::Interpreter
                        }
                    );
                    println!(
                        "MULTI_DEFINITION_BENCH {}",
                        serde_json::json!({
                            "source_commit": source_commit, "source_dirty": source_dirty,
                            "adapter": adapter.get_info().name, "backend": format!("{:?}", adapter.get_info().backend),
                            "topology": topology, "definitions": definitions, "instances": world.field_instances().len(),
                            "definition_hashes": world.field_definitions().iter().map(|d| d.revision().hex()).collect::<Vec<_>>(),
                            "size": size, "mode": label, "selected": format!("{:?}", compiler.active),
                            "gpu_compute_ms": stats(&mut gpu), "cpu_draw_submit_ms": stats(&mut cpu),
                            "requests": compiler.requests, "cached_pipelines": compiler.cached_pipelines,
                            "source_bytes": compiler.source_bytes, "node_bytes": renderer.field_cache_stats().unwrap().node_bytes,
                            "warmup": 3, "samples": 7,
                        })
                    );
                }
            }
        }
    }
}

fn evolve_visual_world(world: &mut WorldState, ids: [EntityId; 3], step: usize) {
    let x = -1.2 + (step % 5) as f64 * 0.04;
    world
        .set_field_transform(ids[0], Transform::new(v(x, -0.3, 0.0), 1.0).unwrap())
        .unwrap();
    world
        .set_field_transform(
            ids[2],
            Transform::new(v(1.2 - (step % 7) as f64 * 0.03, 0.3, 0.0), 1.0).unwrap(),
        )
        .unwrap();
    let source = if step == 20 {
        Some(br#"{"format_version":1,"id":"temporary-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.5,0.6,0.5]}]}"#.to_vec())
    } else if step == 25 || (step.is_multiple_of(2) && !(20..=25).contains(&step)) {
        Some(format!(r#"{{"format_version":1,"id":"evolving","root":"s","nodes":[{{"op":"sphere","id":"s","radius":{:.2}}}]}}"#, 0.5 + step as f64 * 0.01).into_bytes())
    } else {
        None
    };
    if let Some(source) = source {
        for id in &ids[..2] {
            world.replace_field(*id, &source).unwrap();
        }
    }
}

#[test]
#[ignore = "requires native GPU timestamps; deterministic changing-world visual workload"]
fn evolving_visual_world_stress() {
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
    let mut world = WorldState::new(DeterministicSeed(405));
    let mut ids = Vec::new();
    for (radius, x) in [(0.5, -1.2), (0.8, 1.2)] {
        let source = format!(
            r#"{{"format_version":1,"id":"group","root":"s","nodes":[{{"op":"sphere","id":"s","radius":{radius:.2}}}]}}"#
        );
        for y in [-0.3, 0.3] {
            ids.push(
                world
                    .spawn_field(
                        source.as_bytes(),
                        Transform::new(v(x, y, 0.0), 1.0).unwrap(),
                    )
                    .unwrap(),
            );
        }
    }
    let ids = [ids[0], ids[1], ids[2]];
    let mut replay = world.clone();
    let draw = |world: &WorldState| {
        let started = Instant::now();
        let _image = renderer
            .draw_mixed_with_execution(
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
                    diagnostic: true,
                    materials: None,
                    simulation_seconds: 0.0,
                    background: [0.05, 0.075, 0.115],
                    light_direction: [0.5, 0.8, -0.6],
                },
                GpuFieldRenderMode {
                    execution: GpuFieldExecution::Structural,
                    spatial_guard: true,
                },
            )
            .unwrap();
        let submit_ms = started.elapsed().as_secs_f64() * 1000.0;
        (submit_ms, timer.read_ms(&device, &queue).unwrap())
    };
    for _ in 0..500 {
        draw(&world);
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Structural {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Structural
    );
    let requests = renderer.compiler_stats().unwrap().requests;
    for step in 0..40 {
        let started = Instant::now();
        evolve_visual_world(&mut world, ids, step);
        let update_ms = started.elapsed().as_secs_f64() * 1000.0;
        evolve_visual_world(&mut replay, ids, step);
        assert_eq!(world, replay);
        let (submit_ms, gpu_ms) = draw(&world);
        let compiler = renderer.compiler_stats().unwrap();
        assert_eq!(compiler.requests, requests);
        assert_eq!(
            compiler.active,
            if (20..25).contains(&step) {
                GpuFieldPath::Interpreter
            } else {
                GpuFieldPath::Structural
            }
        );
        let fields = renderer.field_cache_stats().unwrap();
        println!(
            "EVOLVING_WORLD_FRAME {}",
            serde_json::json!({
                "step": step, "definitions": world.field_definitions().len(), "instances": world.field_instances().len(),
                "revision": world.field_instances()[0].revision().hex(), "mode": format!("{:?}", compiler.active),
                "world_update_ms": update_ms, "cpu_draw_submit_ms": submit_ms, "gpu_compute_ms": gpu_ms,
                "node_enqueue_ms": fields.last_node_enqueue_ms, "node_bytes": fields.node_bytes,
                "compiler_requests": compiler.requests, "cached_pipelines": compiler.cached_pipelines,
            })
        );
    }
}
