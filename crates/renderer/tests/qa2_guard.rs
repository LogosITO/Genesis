//! Opt-in readback of the ordinary visual guard path, with an independent f64 sphere oracle.

use analytic_renderer::{
    Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
    GpuTimer, MixedDrawOptions, MixedGpuIdentity, field_graph_gpu::GraphTracePolicy,
};
use spatial_math::{Transform, Vec3};
use std::time::Instant;
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
#[ignore = "native GPU timestamps; release-profile matched visual guard workloads"]
fn visual_guard_gpu_cost() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let timer =
        GpuTimer::new(&device).expect("GPU timestamps required for this opt-in measurement");
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let source = br#"{"format_version":1,"id":"qa2-perf-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
    for (label, count, scale) in [("small", 1, 0.1), ("covering", 1, 2.0), ("overlap", 4, 1.0)] {
        let mut world = WorldState::new(DeterministicSeed(0x5142));
        for index in 0..count {
            world
                .spawn_field(
                    source,
                    Transform::new(v(index as f64 * 0.1, 0.0, 0.0), scale).unwrap(),
                )
                .unwrap();
        }
        let camera = Camera::look_at(v(0.0, 0.0, -4.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
        for guard in [false, true] {
            let mut gpu = Vec::new();
            let mut wall = Vec::new();
            for sample in 0..18 {
                let started = Instant::now();
                renderer
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
                            execution: GpuFieldExecution::Interpreter,
                            spatial_guard: guard,
                        },
                    )
                    .unwrap();
                let ms = timer.read_ms(&device, &queue).unwrap();
                if sample >= 3 {
                    gpu.push(ms);
                    wall.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            gpu.sort_by(f64::total_cmp);
            wall.sort_by(f64::total_cmp);
            println!(
                "QA2_GUARD_COST adapter={:?} scene={label} instances={count} guard={guard} samples={} gpu_p50={} gpu_p95={} wall_p50={} wall_p95={}",
                adapter.get_info(),
                gpu.len(),
                gpu[gpu.len() / 2],
                gpu[gpu.len() - 1],
                wall[wall.len() / 2],
                wall[wall.len() - 1]
            );
        }
    }
}

fn read(
    renderer: &GpuRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world: &WorldState,
    camera: Camera,
    policy: GraphTracePolicy,
    guard_and_diagnostic: [bool; 2],
) -> Vec<analytic_renderer::MixedGpuResult> {
    renderer
        .diagnose_mixed_camera(
            device,
            queue,
            world,
            camera,
            MixedDrawOptions {
                draw: DrawOptions {
                    size: [64, 64],
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
                policy,
                show_analytic: false,
                show_fields: true,
                diagnostic: guard_and_diagnostic[1],
                materials: None,
                simulation_seconds: 0.0,
                background: [0.0; 3],
                light_direction: [0.0, 1.0, 0.0],
            },
            GpuFieldRenderMode {
                execution: GpuFieldExecution::Interpreter,
                spatial_guard: guard_and_diagnostic[0],
            },
        )
        .unwrap()
}

fn read_one(
    renderer: &GpuRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world: &WorldState,
    camera: Camera,
    execution: GpuFieldExecution,
    guard: bool,
) -> analytic_renderer::MixedGpuResult {
    renderer
        .diagnose_mixed_camera(
            device,
            queue,
            world,
            camera,
            MixedDrawOptions {
                draw: DrawOptions {
                    size: [1, 1],
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
                policy: GraphTracePolicy::default(),
                show_analytic: false,
                show_fields: true,
                diagnostic: false,
                materials: None,
                simulation_seconds: 0.0,
                background: [0.0; 3],
                light_direction: [0.0, 1.0, 0.0],
            },
            GpuFieldRenderMode {
                execution,
                spatial_guard: guard,
            },
        )
        .unwrap()[0]
}

#[test]
#[ignore = "native GPU; independent symmetric smooth-union and nested-transform surface oracles"]
fn smooth_union_and_nested_transform_oracle() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let smooth = include_bytes!("../../../examples/field-graph/definitions/smooth-bridge.json");
    let nested = br#"{"format_version":1,"id":"qa2-nested","root":"q","nodes":[{"op":"sphere","id":"s","radius":0.5},{"op":"translate","id":"t","child":"s","offset":[0.5,0,0]},{"op":"scale","id":"q","child":"t","factor":2.0}]}"#;
    for (label, source, transform, center_x, radius, samples) in [
        (
            "smooth",
            smooth.as_slice(),
            Transform::identity(),
            0.0,
            0.0,
            vec![0.0, 0.25, 0.5, 0.65, 0.7],
        ),
        (
            "nested",
            nested.as_slice(),
            Transform::new(v(2.0, 0.0, 0.0), 0.5).unwrap(),
            2.5,
            0.5,
            vec![0.0, 0.1, 0.2],
        ),
    ] {
        let mut world = WorldState::new(DeterministicSeed(0x5144));
        let id = world.spawn_field(source, transform).unwrap();
        for y in samples {
            let camera = Camera::look_at(
                v(center_x, y, -4.0),
                v(center_x, y, 0.0),
                v(0.0, 1.0, 0.0),
                0.9,
            )
            .unwrap();
            // At x=0 the two symmetric exact-SDF branches agree, so smooth min
            // reduces analytically to sqrt(0.9²+y²+z²)-1.2.
            let z = if label == "smooth" {
                -(1.2_f64.powi(2) - 0.9_f64.powi(2) - y * y).sqrt()
            } else {
                -(radius * radius - y * y).sqrt()
            };
            let expected = 4.0 + z;
            let normal_length = (y * y + z * z).sqrt();
            for execution in [GpuFieldExecution::Interpreter, GpuFieldExecution::Combined] {
                for guard in [false, true] {
                    let mut result =
                        read_one(&renderer, &device, &queue, &world, camera, execution, guard);
                    if execution == GpuFieldExecution::Combined {
                        for _ in 0..100 {
                            if result.path == GpuFieldPath::Combined {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            result = read_one(
                                &renderer, &device, &queue, &world, camera, execution, guard,
                            );
                        }
                        assert_eq!(result.path, GpuFieldPath::Combined);
                    }
                    assert!(
                        result.hit && !result.uncertain && !result.exhausted,
                        "{label} y={y} {execution:?} guard={guard} {result:?}"
                    );
                    assert_eq!(
                        result.identity,
                        Some(MixedGpuIdentity::FieldInstance(id.value()))
                    );
                    assert!(
                        (f64::from(result.distance.unwrap()) - expected).abs() < 0.02,
                        "{label} y={y} expected={expected} {result:?}"
                    );
                    let normal = result.normal.unwrap();
                    assert!((f64::from(normal[1]) - y / normal_length).abs() < 0.04);
                    assert!((f64::from(normal[2]) - z / normal_length).abs() < 0.04);
                }
            }
        }
    }
    println!(
        "QA2_INDEPENDENT_SURFACES adapter={:?} cases=8 paths=interpreter,combined guard=on,off",
        adapter.get_info()
    );
}

#[test]
#[ignore = "native GPU; release-profile ordinary visual guard readback"]
fn visual_guard_sphere_oracle() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let source = br#"{"format_version":1,"id":"qa2-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
    let mut definite_hits = 0;
    let mut definite_misses = 0;
    let mut ambiguous = 0;
    let mut culls = 0;
    let mut hidden_uncertainty = 0;
    for (scale, x) in [(0.01, 0.0), (1.0, 0.0), (100.0, 0.0), (1.0, 999.0)] {
        let mut world = WorldState::new(DeterministicSeed(0x5142));
        let id = world
            .spawn_field(source, Transform::new(v(x, 0.0, 0.0), scale).unwrap())
            .unwrap();
        let camera = Camera::look_at(
            v(x, 0.0, -4.0 * scale),
            v(x, 0.0, 0.0),
            v(0.0, 1.0, 0.0),
            0.9,
        )
        .unwrap();
        let unguarded = read(
            &renderer,
            &device,
            &queue,
            &world,
            camera,
            GraphTracePolicy::default(),
            [false, false],
        );
        let guarded = read(
            &renderer,
            &device,
            &queue,
            &world,
            camera,
            GraphTracePolicy::default(),
            [true, false],
        );
        for (index, (a, b)) in unguarded.into_iter().zip(guarded).enumerate() {
            let ray = camera
                .ray(index as u32 % 64, index as u32 / 64, 64, 64)
                .unwrap();
            let o = ray.origin();
            let d = ray.direction();
            let q = [o.x() - x, o.y(), o.z()];
            let direction = [d.x(), d.y(), d.z()];
            let projection = q.iter().zip(direction).map(|(a, b)| a * b).sum::<f64>();
            let discriminant = projection * projection
                - (q.iter().map(|item| item * item).sum::<f64>() - scale * scale);
            let clearance = discriminant.abs().sqrt();
            // The tracer's finite hit threshold amplifies depth error near tangency.
            if clearance < scale * 0.5 {
                ambiguous += 1;
                continue;
            }
            if discriminant > 0.0 {
                let expected = -projection - discriminant.sqrt();
                if expected >= ray.interval().0 && expected <= ray.interval().1 {
                    definite_hits += 1;
                    assert!(
                        b.hit || b.uncertain || b.exhausted,
                        "guard hid hit: scale={scale} pixel={index} {b:?}"
                    );
                    if b.hit {
                        assert_eq!(
                            b.identity,
                            Some(MixedGpuIdentity::FieldInstance(id.value()))
                        );
                        assert!(
                            (f64::from(b.distance.unwrap()) - expected).abs()
                                < 0.004_f64.max(scale * 0.004),
                            "scale={scale} pixel={index} expected={expected} actual={b:?} unguarded={a:?}"
                        );
                    }
                }
            } else {
                definite_misses += 1;
                assert!(
                    !b.hit,
                    "guarded false hit: scale={scale} pixel={index} {b:?}"
                );
            }
            culls += usize::from(b.bounds_culled);
            if (a.uncertain || a.exhausted) && !(b.uncertain || b.exhausted) {
                hidden_uncertainty += 1;
            }
            assert!(
                !a.hit || b.hit || b.uncertain || b.exhausted,
                "guard changed definite tracer hit: scale={scale} pixel={index}"
            );
        }
    }
    println!(
        "QA2_GUARD adapter={:?} hits={definite_hits} misses={definite_misses} ambiguous={ambiguous} culls={culls} hidden_uncertainty={hidden_uncertainty}",
        adapter.get_info()
    );
    assert!(definite_hits > 0 && definite_misses > 0 && culls > 0);
    assert_eq!(hidden_uncertainty, 0);
}

#[test]
#[ignore = "native GPU; distinguish visual culling from diagnostic uncertainty preservation"]
fn visual_guard_exhaustion_contract() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let source = br#"{"format_version":1,"id":"qa2-exhaustion","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
    let mut world = WorldState::new(DeterministicSeed(0x5143));
    world.spawn_field(source, Transform::identity()).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -4.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let policy = GraphTracePolicy {
        iterations: 1,
        ..GraphTracePolicy::default()
    };
    let visual_off = read(
        &renderer,
        &device,
        &queue,
        &world,
        camera,
        policy,
        [false, false],
    );
    let visual_on = read(
        &renderer,
        &device,
        &queue,
        &world,
        camera,
        policy,
        [true, false],
    );
    let diagnostic_on = read(
        &renderer,
        &device,
        &queue,
        &world,
        camera,
        policy,
        [true, true],
    );
    let mut hidden = 0;
    let mut culls = 0;
    for ((off, on), diagnostic) in visual_off.iter().zip(&visual_on).zip(&diagnostic_on) {
        culls += usize::from(on.bounds_culled);
        hidden += usize::from(off.exhausted && on.bounds_culled && !on.exhausted);
        assert_eq!(diagnostic.exhausted, off.exhausted);
        assert!(!diagnostic.bounds_culled);
    }
    println!(
        "QA2_EXHAUSTION adapter={:?} visual_culls={culls} visual_hidden_exhausted={hidden} diagnostic_preserved=true",
        adapter.get_info()
    );
    assert!(culls > 0);
    assert!(
        hidden > 0,
        "the visual path must expose the uncertainty gap"
    );
}
