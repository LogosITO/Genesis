//! Opt-in release-profile measurements of an actually validated combined WorldState.

use analytic_field::Sphere;
use analytic_renderer::{
    Bvh, Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
    GpuTimer, MixedDrawOptions, Scene,
    field_graph_gpu::GraphTracePolicy,
    material::{Material, MaterialPattern, MaterialSet},
};
use spatial_math::{Transform, Vec3};
use std::{hint::black_box, time::Instant};
use world_runtime::Runtime;
use world_simulation::EnvironmentEventKind;
use world_simulation::SimulationStep;
use world_state::{
    DeterministicSeed, GrowthParameters, MAX_NODES, ParameterBinding, ParameterComponent,
    ParameterRule, WorldState,
};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

fn world(spheres: usize, nodes_per_organism: usize, dense_authored: bool) -> WorldState {
    assert!((1..=MAX_NODES).contains(&nodes_per_organism));
    let mut world = WorldState::new(DeterministicSeed(0x5141));
    let up = v(0.0, 1.0, 0.0);
    for i in 0..4 {
        let id = world
            .spawn_organism(
                v(i as f64 * 3.0, 0.0, 0.0),
                GrowthParameters::new(0.1, 0.1, 1.0, 1.0)
                    .unwrap()
                    .with_max_children(1)
                    .unwrap(),
            )
            .unwrap();
        let tree = &mut world.organisms_mut()[i];
        assert_eq!(tree.id(), id);
        while tree.nodes().len() < nodes_per_organism {
            let len = tree.nodes().len();
            let samples = (0..len)
                .map(|index| (f64::from(index + 1 == len), up))
                .collect::<Vec<_>>();
            assert_eq!(tree.grow(&samples).unwrap(), 1);
        }
    }
    for i in 0..4 {
        world
            .spawn_finite_source(v(i as f64 * 3.0, 0.0, 0.0), 2.0, 2.0, 1.0, 1.0, 0.0)
            .unwrap();
    }
    for index in 0..spheres {
        world
            .spawn_sphere(
                Sphere::new(0.08).unwrap(),
                Transform::new(v(index as f64 * 0.025 - 6.0, -3.0, 0.0), 1.0).unwrap(),
                0.0,
            )
            .unwrap();
    }
    if dense_authored {
        for index in 0..4 {
            let source = format!(
                r#"{{"format_version":1,"id":"qa1-dense-{index}","revision":1,"axiom":"FF","rules":[{{"symbol":"F","replacement":"FF"}}],"iterations":8,"step_length":0.01,"yaw_degrees":0.0,"pitch_degrees":0.0,"branch_radius":0.005,"budgets":{{"max_symbols":1024,"max_stack_depth":12,"max_segments":512,"max_work":8192}}}}"#
            );
            for occurrence in 0..2 {
                world
                    .spawn_authored(
                        source.as_bytes(),
                        Transform::new(
                            v(-8.0 + index as f64 * 4.0 + occurrence as f64, 0.0, 0.0),
                            1.0,
                        )
                        .unwrap(),
                        true,
                    )
                    .unwrap();
            }
        }
    } else {
        for (index, source) in [
            include_bytes!("../../authoring/branch-a.json").as_slice(),
            include_bytes!("../../authoring/branch-b.json").as_slice(),
        ]
        .into_iter()
        .enumerate()
        {
            for occurrence in 0..4 {
                world
                    .spawn_authored(
                        source,
                        Transform::new(
                            v(-8.0 + index as f64 * 8.0 + occurrence as f64, 0.0, 0.0),
                            1.0,
                        )
                        .unwrap(),
                        true,
                    )
                    .unwrap();
            }
        }
    }
    for (index, source) in [
        include_bytes!("../../field-graph/definitions/twin.json").as_slice(),
        include_bytes!("../../field-graph/definitions/plinth.json").as_slice(),
        include_bytes!("../../field-graph/definitions/morph-lobes.json").as_slice(),
        include_bytes!("../../field-graph/definitions/smooth-bridge.json").as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        for occurrence in 0..2 {
            let id = world
                .spawn_field(
                    source,
                    Transform::new(
                        v(
                            -3.0 + index as f64 * 2.0 + occurrence as f64 * 0.5,
                            0.0,
                            0.0,
                        ),
                        1.0,
                    )
                    .unwrap(),
                )
                .unwrap();
            if index == 2 {
                world
                    .enable_field_parameters(
                        id,
                        vec![ParameterRule {
                            binding: ParameterBinding {
                                node: "left_ball".into(),
                                component: ParameterComponent::SphereRadius,
                            },
                            uptake: 1.0,
                            gain_per_unit: 0.1,
                            maximum: 1.0,
                            positive_site: true,
                            feedback: true,
                        }],
                    )
                    .unwrap();
            }
        }
    }
    world.spawn_body(v(0.0, 0.0, -5.0), 0.2).unwrap();
    world.validate().unwrap();
    world
}

fn measure<T>(mut run: impl FnMut() -> T, samples: usize) -> (f64, f64) {
    for _ in 0..3 {
        black_box(run());
    }
    let mut values = (0..samples)
        .map(|_| {
            let start = Instant::now();
            black_box(run());
            start.elapsed().as_secs_f64() * 1_000.0
        })
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    (
        values[samples / 2],
        values[(samples * 95).div_ceil(100) - 1],
    )
}

fn measure_tick(initial: &Runtime, samples: usize) -> (f64, f64) {
    let mut values = Vec::with_capacity(samples);
    for index in 0..samples + 3 {
        let mut runtime = initial.clone();
        let start = Instant::now();
        runtime.tick().unwrap();
        let ms = start.elapsed().as_secs_f64() * 1_000.0;
        black_box(runtime);
        if index >= 3 {
            values.push(ms);
        }
    }
    values.sort_by(f64::total_cmp);
    (
        values[samples / 2],
        values[(samples * 95).div_ceil(100) - 1],
    )
}

#[test]
#[ignore = "local release-profile maximum-valid-world CPU probe; run serially"]
fn combined_world_cpu_phases() {
    for count in [128, 256, 512] {
        let world = world(count, MAX_NODES, false);
        let initial = Runtime::new(world.clone(), SimulationStep::new(0.1).unwrap());
        let scene = Scene::from_world(&world).unwrap();
        let save = initial.save_bytes().unwrap();
        let validation = measure(|| world.validate().unwrap(), 21);
        let snapshot = measure(|| black_box(Scene::from_world(&world).unwrap()), 21);
        let bvh = measure(|| black_box(Bvh::build(&scene).unwrap()), 21);
        let serialize = measure(|| black_box(initial.save_bytes().unwrap()), 21);
        let deserialize = measure(|| black_box(Runtime::load_bytes(&save).unwrap()), 21);
        let tick = measure_tick(&initial, 21);
        let tree = Bvh::build(&scene).unwrap();
        println!(
            "QA1_VALID_WORLD {{\"spheres\":{count},\"organisms\":4,\"nodes_per_organism\":48,\"sources\":4,\"authored_instances\":8,\"field_definitions\":4,\"field_instances\":8,\"primitives\":{},\"bvh_nodes\":{},\"bvh_depth\":{},\"save_bytes\":{},\"validate_ms\":{:?},\"snapshot_ms\":{:?},\"bvh_ms\":{:?},\"serialize_ms\":{:?},\"deserialize_ms\":{:?},\"tick_ms\":{:?}}}",
            scene.primitives().len(),
            tree.node_count(),
            tree.depth(),
            save.len(),
            validation,
            snapshot,
            bvh,
            serialize,
            deserialize,
            tick
        );
    }
}

#[test]
#[ignore = "local release-profile organism-node scaling probe; run serially"]
fn organism_node_tick_scaling() {
    for nodes in [1, 24, 48] {
        let world = world(0, nodes, false);
        let runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        println!(
            "QA1_NODE_SCALING nodes_per_organism={nodes} tick_ms_p50_p95={:?}",
            measure_tick(&runtime, 51)
        );
    }
}

#[test]
#[ignore = "local release-profile simultaneous WorldState capacity probe"]
fn dense_authored_world_reaches_combined_limits() {
    let world = world(512, MAX_NODES, true);
    let authored = world.authored_capsules().unwrap().len();
    assert_eq!(authored, 4096);
    let scene = Scene::from_world(&world).unwrap();
    let runtime = Runtime::new(world.clone(), SimulationStep::new(0.1).unwrap());
    let save = runtime.save_bytes().unwrap();
    if let Some(path) = std::env::var_os("GENESIS_QA2_FIXTURE_OUT") {
        std::fs::write(
            path,
            Runtime::new(world.clone(), SimulationStep::new(0.016_666_667).unwrap())
                .save_bytes()
                .unwrap(),
        )
        .unwrap();
    }
    let loaded = Runtime::load_bytes(&save).unwrap();
    assert_eq!(loaded, runtime);
    let validate = measure(|| world.validate().unwrap(), 11);
    let snapshot = measure(|| Scene::from_world(&world).unwrap(), 11);
    let bvh = measure(|| Bvh::build(&scene).unwrap(), 11);
    let tick = measure_tick(&runtime, 11);
    let tree = Bvh::build(&scene).unwrap();
    println!(
        "QA1_DENSE_VALID_WORLD authored_segments={authored} primitives={} bvh_nodes={} bvh_depth={} save_bytes={} validate_ms={validate:?} snapshot_ms={snapshot:?} bvh_ms={bvh:?} tick_ms={tick:?}",
        scene.primitives().len(),
        tree.node_count(),
        tree.depth(),
        save.len()
    );
}

#[test]
#[ignore = "native GPU; dense dynamic Interpreter/Combined hit and material readback"]
fn dense_dynamic_combined_diagnostic_parity() {
    let mut runtime = Runtime::new(
        world(512, MAX_NODES, true),
        SimulationStep::new(0.016_666_667).unwrap(),
    );
    let source = runtime.world().sources()[0].id();
    runtime
        .schedule(
            30,
            EnvironmentEventKind::MoveSource {
                id: source,
                position: v(2.0, 1.0, 0.0),
            },
        )
        .unwrap();
    runtime
        .schedule(
            60,
            EnvironmentEventKind::MoveSource {
                id: source,
                position: v(0.0, 2.0, 0.0),
            },
        )
        .unwrap();
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let camera =
        Camera::look_at(v(-3.0, 0.0, -5.0), v(-3.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let mut materials = MaterialSet::default();
    materials.set_field(
        runtime.world().field_instances()[0].id(),
        Material::new(
            [0.8, 0.2, 0.1],
            [0.0; 3],
            [0.0; 3],
            MaterialPattern::Constant,
        )
        .unwrap(),
    );
    let read = |world: &WorldState, execution| {
        renderer
            .diagnose_mixed_camera(
                &device,
                &queue,
                world,
                camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size: [17, 17],
                        normal_debug: false,
                        surface: None,
                        timer: None,
                    },
                    policy: GraphTracePolicy::default(),
                    show_analytic: true,
                    show_fields: true,
                    diagnostic: true,
                    materials: Some(&materials),
                    simulation_seconds: 0.0,
                    background: [0.0; 3],
                    light_direction: [0.0, 1.0, 0.0],
                },
                GpuFieldRenderMode {
                    execution,
                    spatial_guard: true,
                },
            )
            .unwrap()
    };
    for tick in [0, 30, 60] {
        while runtime.time().ticks() < tick {
            runtime.tick().unwrap();
        }
        let reference = read(runtime.world(), GpuFieldExecution::Interpreter);
        let mut compiled = None;
        for _ in 0..200 {
            let result = read(runtime.world(), GpuFieldExecution::Combined);
            if result[0].path == GpuFieldPath::Combined {
                compiled = Some(result);
                break;
            }
            assert!(renderer.compiler_stats().unwrap().last_error.is_none());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let compiled = compiled.expect("Combined did not become active");
        let mut field_hits = 0;
        let mut material_hits = 0;
        for (left, right) in reference.iter().zip(&compiled) {
            assert_eq!(
                (
                    left.hit,
                    left.field,
                    left.uncertain,
                    left.exhausted,
                    left.ambiguous
                ),
                (
                    right.hit,
                    right.field,
                    right.uncertain,
                    right.exhausted,
                    right.ambiguous
                ),
                "tick={tick}"
            );
            assert_eq!(
                (left.identity, left.definition, left.material),
                (right.identity, right.definition, right.material)
            );
            if let (Some(a), Some(b)) = (left.distance, right.distance) {
                assert!((a - b).abs() < 0.02, "tick={tick} distance {a} != {b}");
            }
            if let (Some(a), Some(b)) = (left.normal, right.normal) {
                assert!(a.into_iter().zip(b).all(|(x, y)| (x - y).abs() < 0.04));
            }
            field_hits += usize::from(left.hit && left.field);
            if left.hit && left.field {
                assert!(left.definition.is_some());
                material_hits += usize::from(left.material.is_some());
            }
        }
        assert!(field_hits > 0, "tick={tick} lacked field coverage");
        assert!(
            material_hits > 0,
            "tick={tick} lacked assigned material coverage"
        );
        println!(
            "AC1_DENSE_PARITY tick={tick} rays={} field_hits={field_hits} requests={}",
            reference.len(),
            renderer.compiler_stats().unwrap().requests
        );
    }
    assert_eq!(renderer.compiler_stats().unwrap().requests, 1);
}

#[test]
#[ignore = "release-profile dense scene delta at one scheduled source move"]
fn dense_source_move_primitive_delta() {
    let mut runtime = Runtime::new(
        world(512, MAX_NODES, true),
        SimulationStep::new(0.016_666_667).unwrap(),
    );
    runtime
        .schedule(
            30,
            EnvironmentEventKind::MoveSource {
                id: runtime.world().sources()[0].id(),
                position: v(2.0, 1.0, 0.0),
            },
        )
        .unwrap();
    runtime
        .schedule(
            60,
            EnvironmentEventKind::MoveSource {
                id: runtime.world().sources()[0].id(),
                position: v(0.0, 2.0, 0.0),
            },
        )
        .unwrap();
    for tick in [30, 60] {
        while runtime.time().ticks() < tick - 1 {
            runtime.tick().unwrap();
        }
        let before = Scene::from_world(runtime.world()).unwrap();
        runtime.tick().unwrap();
        let after = Scene::from_world(runtime.world()).unwrap();
        assert_eq!(before.primitives().len(), 4993);
        assert_eq!(after.primitives().len(), 4993);
        let changed = before
            .primitives()
            .iter()
            .zip(after.primitives())
            .filter(|(a, b)| {
                a.kind != b.kind
                    || a.center() != b.center()
                    || a.dimensions() != b.dimensions()
                    || a.capsule_radius() != b.capsule_radius()
                    || a.target() != b.target()
            })
            .map(|(_, b)| (b.id, b.target()))
            .collect::<Vec<_>>();
        assert!(!changed.is_empty());
        println!("AC1_SOURCE_DELTA tick={tick} changed={changed:?}");
    }
}

#[test]
#[ignore = "native GPU; complete 720p frame at simultaneous valid WorldState caps"]
fn dense_authored_world_gpu_frame() {
    let world = world(512, MAX_NODES, true);
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device).unwrap();
    let camera =
        Camera::look_at(v(0.0, 1.0, -15.0), v(0.0, 1.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    for (label, mode) in [
        ("interpreter", GpuFieldExecution::Interpreter),
        ("combined", GpuFieldExecution::Combined),
    ] {
        let mut cpu = Vec::new();
        let mut gpu = Vec::new();
        let mut total = Vec::new();
        for index in 0..9 {
            let started = Instant::now();
            let _image = renderer
                .draw_mixed_with_execution(
                    &device,
                    &queue,
                    &world,
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
                        background: [0.0; 3],
                        light_direction: [0.0, 1.0, 0.0],
                    },
                    GpuFieldRenderMode {
                        execution: mode,
                        spatial_guard: true,
                    },
                )
                .unwrap();
            let submit_ms = started.elapsed().as_secs_f64() * 1_000.0;
            let gpu_ms = timer.read_ms(&device, &queue).unwrap();
            let total_ms = started.elapsed().as_secs_f64() * 1_000.0;
            if index == 0 {
                println!(
                    "QA1_DENSE_COLD mode={label} active={:?} submit_ms={submit_ms} gpu_ms={gpu_ms} offscreen_ms={total_ms} upload={:?} acceleration={:?}",
                    renderer.compiler_stats().unwrap().active,
                    renderer.last_upload_stats(),
                    renderer.acceleration_stats()
                );
            }
            if index >= 2 {
                cpu.push(submit_ms);
                gpu.push(gpu_ms);
                total.push(total_ms);
            }
        }
        let rank = |mut values: Vec<f64>| {
            values.sort_by(f64::total_cmp);
            (values[values.len() / 2], values[values.len() - 1])
        };
        println!(
            "QA1_DENSE_GPU adapter={:?} mode={label} active={:?} cpu_submit_p50_max={:?} gpu_compute_p50_max={:?} offscreen_p50_max={:?} draw={:?} upload={:?} acceleration={:?}",
            adapter.get_info(),
            renderer.compiler_stats().unwrap().active,
            rank(cpu),
            rank(gpu),
            rank(total),
            renderer.last_draw_stats(),
            renderer.last_upload_stats(),
            renderer.acceleration_stats()
        );
    }
}

#[test]
#[ignore = "native GPU timestamps; release-profile complete offscreen mixed frame"]
fn combined_world_offscreen_gpu_frame() {
    let world = world(512, MAX_NODES, false);
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let camera =
        Camera::look_at(v(0.0, 1.0, -15.0), v(0.0, 1.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let draw = |size: [u32; 2], mode| {
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
                        timer: timer.as_ref(),
                    },
                    policy: GraphTracePolicy::default(),
                    show_analytic: true,
                    show_fields: true,
                    diagnostic: false,
                    materials: None,
                    simulation_seconds: 0.0,
                    background: [0.0; 3],
                    light_direction: [0.0, 1.0, 0.0],
                },
                GpuFieldRenderMode {
                    execution: mode,
                    spatial_guard: true,
                },
            )
            .unwrap();
        let submit_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let gpu_ms = timer
            .as_ref()
            .map(|timer| timer.read_ms(&device, &queue).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let total_ms = started.elapsed().as_secs_f64() * 1_000.0;
        (submit_ms, gpu_ms, total_ms)
    };
    // Request compilation on a small image, then wait for a selected Combined program.
    for _ in 0..100 {
        draw([16, 16], GpuFieldExecution::Combined);
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Combined {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("QA1_COMPILER_WARM {:?}", renderer.compiler_stats().unwrap());
    for (label, mode) in [
        ("interpreter", GpuFieldExecution::Interpreter),
        ("combined", GpuFieldExecution::Combined),
    ] {
        for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
            for _ in 0..2 {
                draw(size, mode);
            }
            let mut submit = Vec::new();
            let mut gpu = Vec::new();
            let mut total = Vec::new();
            for _ in 0..7 {
                let (a, b, c) = draw(size, mode);
                submit.push(a);
                if let Some(b) = b {
                    gpu.push(b);
                }
                total.push(c);
            }
            let rank = |mut values: Vec<f64>| {
                values.sort_by(f64::total_cmp);
                (values[values.len() / 2], values[values.len() - 1])
            };
            println!(
                "QA1_MIXED_FRAME mode={label} size={size:?} active={:?} submit_ms_p50_max={:?} gpu_ms_p50_max={:?} offscreen_ms_p50_max={:?} draw={:?} upload={:?} fields={:?}",
                renderer.compiler_stats().unwrap().active,
                rank(submit),
                (!gpu.is_empty()).then(|| rank(gpu)),
                rank(total),
                renderer.last_draw_stats().unwrap(),
                renderer.last_upload_stats(),
                renderer.field_cache_stats()
            );
        }
    }
}
