//! Bounded independent f64 spatial oracles for actual native GPU readback.

use analytic_field::Capsule;
use analytic_renderer::{
    Camera, DrawOptions, GpuFieldExecution, GpuFieldRenderMode, GpuRenderer, GpuTraversal,
    MixedDrawOptions, MixedGpuIdentity, Primitive, Ray, Scene,
    field_graph_gpu::{GraphGpuRenderer, GraphTracePolicy, WorldFieldGpuOutcome},
};
use spatial_math::{Transform, Vec3};
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

fn diagnose(
    renderer: &GpuRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world: &WorldState,
    camera: Camera,
    policy: GraphTracePolicy,
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
                policy,
                show_analytic: false,
                show_fields: true,
                diagnostic: true,
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
        .unwrap()[0]
}

// Independent ray/sphere quadratic in f64, using the camera's actual ray.
fn sphere_entry(ray: analytic_renderer::Ray, center: Vec3, radius: f64) -> Option<(f64, [f64; 3])> {
    let o = ray.origin();
    let d = ray.direction();
    let q = [o.x() - center.x(), o.y() - center.y(), o.z() - center.z()];
    let direction = [d.x(), d.y(), d.z()];
    let b = q.iter().zip(direction).map(|(a, b)| a * b).sum::<f64>();
    let c = q.iter().map(|x| x * x).sum::<f64>() - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    let distance = -b - discriminant.sqrt();
    let (near, far) = ray.interval();
    if distance < near || distance > far {
        return None;
    }
    Some((
        distance,
        std::array::from_fn(|i| (q[i] + distance * direction[i]) / radius),
    ))
}

#[test]
#[ignore = "native GPU; release-profile independent f64 ray/sphere diagnostic corpus"]
fn native_sphere_spatial_oracle() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let direct = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let source = br#"{"format_version":1,"id":"qa1-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
    let policy = GraphTracePolicy::default();
    let mut qualified_hits = 0;
    let mut qualified_misses = 0;
    let mut ambiguous = 0;
    let mut false_misses = Vec::new();
    let mut false_hits = Vec::new();
    let mut wrong_depths = Vec::new();
    let mut hidden_uncertainty = Vec::new();
    for (scale, x) in [
        (0.01, 0.0),
        (1.0, 0.0),
        (100.0, 0.0),
        (0.01, 999.0),
        (1.0, 999.0),
    ] {
        for offset in [
            0.0,
            0.25,
            0.75,
            0.95,
            0.99,
            1.0,
            1.0 + 1e-8,
            1.01,
            1.05,
            1.25,
        ] {
            let mut world = WorldState::new(DeterministicSeed(0x5141));
            let id = world
                .spawn_field(source, Transform::new(v(x, 0.0, 0.0), scale).unwrap())
                .unwrap();
            let camera = Camera::look_at(
                v(x, scale * offset, -4.0 * scale),
                v(x, scale * offset, 0.0),
                v(0.0, 1.0, 0.0),
                0.9,
            )
            .unwrap();
            let ray = camera.ray(0, 0, 1, 1).unwrap();
            let oracle = sphere_entry(ray, v(x, 0.0, 0.0), scale);
            // The existing direct query rejects far/scale > 10_000; that API limit is
            // recorded separately and does not constrain the native mixed viewport.
            let reference = (scale >= 0.1).then(|| {
                direct
                    .query_world_fields(&device, &queue, &world, camera, &[ray], policy)
                    .unwrap()[0]
            });
            for guard in [false, true] {
                let result = diagnose(&renderer, &device, &queue, &world, camera, policy, guard);
                let label = format!("scale={scale} x={x} offset={offset} guard={guard}");
                let clear = (offset - 1.0).abs() * scale > 4.0 * f64::from(policy.tolerance)
                    && (offset - 1.0).abs() > 0.02;
                if !clear {
                    ambiguous += 1;
                    if (offset - (1.0 + 1e-8)).abs() < 1e-12 && scale == 1.0 && x == 0.0 && guard {
                        println!(
                            "QA1_KNOWN_TANGENT {label} oracle={} direct={reference:?} mixed_hit={} mixed_uncertain={} mixed_exhausted={} depth={:?}",
                            oracle.is_some(),
                            result.hit,
                            result.uncertain,
                            result.exhausted,
                            result.distance
                        );
                    }
                    continue;
                }
                match oracle {
                    Some((distance, normal)) => {
                        qualified_hits += 1;
                        if !result.hit && !result.uncertain && !result.exhausted {
                            false_misses.push(label.clone());
                        }
                        if !result.hit
                            && matches!(
                                reference,
                                Some(
                                    WorldFieldGpuOutcome::Uncertain { .. }
                                        | WorldFieldGpuOutcome::Exhausted { .. }
                                )
                            )
                            && !result.uncertain
                            && !result.exhausted
                        {
                            hidden_uncertainty.push(label.clone());
                        }
                        if result.hit {
                            assert_eq!(
                                result.identity,
                                Some(MixedGpuIdentity::FieldInstance(id.value())),
                                "{label}"
                            );
                            let actual = f64::from(result.distance.unwrap());
                            if (actual - distance).abs() > 0.003_f64.max(scale * 0.003) {
                                wrong_depths
                                    .push(format!("{label}: expected {distance}, got {actual}"));
                            }
                            let actual_normal = result.normal.unwrap();
                            assert!(
                                actual_normal
                                    .iter()
                                    .zip(normal)
                                    .all(|(a, b)| (f64::from(*a) - b).abs() < 0.03),
                                "{label}: {actual_normal:?} vs {normal:?}"
                            );
                        }
                    }
                    None => {
                        qualified_misses += 1;
                        if result.hit {
                            false_hits.push(label);
                        }
                    }
                }
            }
        }
    }
    println!(
        "QA1_NUMERICAL adapter={:?} qualified_hits={qualified_hits} qualified_misses={qualified_misses} ambiguous={ambiguous} false_misses={false_misses:?} false_hits={false_hits:?} wrong_depths={wrong_depths:?} hidden_uncertainty={hidden_uncertainty:?}",
        adapter.get_info()
    );
    assert!(
        false_misses.is_empty(),
        "silent false misses: {false_misses:?}"
    );
    assert!(false_hits.is_empty(), "false hits: {false_hits:?}");
    assert!(wrong_depths.is_empty(), "depth errors: {wrong_depths:?}");
    assert!(
        hidden_uncertainty.is_empty(),
        "hidden uncertainty: {hidden_uncertainty:?}"
    );
}

#[test]
#[ignore = "native GPU; independent f64 transformed-box face and uncertainty probes"]
fn native_box_face_oracle_and_exhaustion() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let source = br#"{"format_version":1,"id":"qa1_box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.4,0.5,0.3]}]}"#;
    let policy = GraphTracePolicy::default();
    let mut qualified_hits = 0;
    let mut qualified_misses = 0;
    let mut ambiguous = 0;
    for (scale, x) in [
        (0.1, 0.0),
        (1.0, 0.0),
        (10.0, 0.0),
        (0.1, 999.0),
        (1.0, 999.0),
    ] {
        for offset in [0.0, 0.25, 0.8, 0.99, 1.0, 1.0 + 1e-8, 1.05, 1.25] {
            let mut world = WorldState::new(DeterministicSeed(0x5142));
            let id = world
                .spawn_field(source, Transform::new(v(x, 0.0, 0.0), scale).unwrap())
                .unwrap();
            let camera = Camera::look_at(
                v(x, offset * 0.5 * scale, -4.0 * scale),
                v(x, offset * 0.5 * scale, 0.0),
                v(0.0, 1.0, 0.0),
                0.9,
            )
            .unwrap();
            // The independent ray/AABB slab solution is 3.7*scale on the -Z face
            // whenever both transverse coordinates lie inside their half extents.
            let expected = (offset <= 1.0).then_some(3.7 * scale);
            let clear = (offset - 1.0).abs() * 0.5 * scale > 4.0 * f64::from(policy.tolerance)
                && (offset - 1.0).abs() > 0.02;
            for guard in [false, true] {
                let result = diagnose(&renderer, &device, &queue, &world, camera, policy, guard);
                if !clear {
                    ambiguous += 1;
                    continue;
                }
                match expected {
                    Some(distance) => {
                        qualified_hits += 1;
                        assert!(
                            result.hit || result.uncertain || result.exhausted,
                            "silent box miss scale={scale} x={x} offset={offset} guard={guard}: {result:?}"
                        );
                        if result.hit {
                            assert_eq!(
                                result.identity,
                                Some(MixedGpuIdentity::FieldInstance(id.value()))
                            );
                            assert!(
                                (f64::from(result.distance.unwrap()) - distance).abs()
                                    < 0.003_f64.max(scale * 0.003),
                                "box depth: {result:?}"
                            );
                            assert!(result.normal.unwrap()[2] < -0.99, "box normal: {result:?}");
                        }
                    }
                    None => {
                        qualified_misses += 1;
                        assert!(
                            !result.hit,
                            "false box hit scale={scale} x={x} offset={offset} guard={guard}: {result:?}"
                        );
                    }
                }
            }
        }
    }
    let mut world = WorldState::new(DeterministicSeed(0x5143));
    world.spawn_field(source, Transform::identity()).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -4.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let exhausted = diagnose(
        &renderer,
        &device,
        &queue,
        &world,
        camera,
        GraphTracePolicy {
            iterations: 1,
            ..policy
        },
        true,
    );
    assert!(
        exhausted.exhausted && !exhausted.hit,
        "iteration budget hidden: {exhausted:?}"
    );
    let inside = Camera::look_at(Vec3::ZERO, v(0.0, 0.0, 1.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let inside = diagnose(&renderer, &device, &queue, &world, inside, policy, true);
    assert!(
        inside.uncertain && !inside.hit,
        "interior start hidden: {inside:?}"
    );
    println!(
        "QA1_BOX adapter={:?} qualified_hits={qualified_hits} qualified_misses={qualified_misses} ambiguous={ambiguous} exhausted={:?} inside={:?}",
        adapter.get_info(),
        exhausted.exhausted,
        inside.uncertain
    );
}

#[test]
#[ignore = "native GPU; independent first-boundary oracle for a two-sphere hard union"]
fn native_hard_union_first_boundary() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let source = br#"{"format_version":1,"id":"qa1_union","root":"u","nodes":[{"op":"sphere","id":"a","radius":0.5},{"op":"translate","id":"left","child":"a","offset":[-0.8,0,0]},{"op":"sphere","id":"b","radius":0.5},{"op":"translate","id":"right","child":"b","offset":[0.8,0,0]},{"op":"union","id":"u","left":"left","right":"right"}]}"#;
    let mut world = WorldState::new(DeterministicSeed(0x5144));
    let id = world.spawn_field(source, Transform::identity()).unwrap();
    for x in [-0.8, -0.4, 0.0, 0.4, 0.8] {
        let camera =
            Camera::look_at(v(x, 0.0, -4.0), v(x, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
        let ray = camera.ray(0, 0, 1, 1).unwrap();
        let expected = [v(-0.8, 0.0, 0.0), v(0.8, 0.0, 0.0)]
            .into_iter()
            .filter_map(|center| sphere_entry(ray, center, 0.5))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        for guard in [false, true] {
            let actual = diagnose(
                &renderer,
                &device,
                &queue,
                &world,
                camera,
                GraphTracePolicy::default(),
                guard,
            );
            match expected {
                Some((distance, normal)) => {
                    assert!(
                        actual.hit && !actual.uncertain && !actual.exhausted,
                        "x={x} guard={guard}: {actual:?}"
                    );
                    assert_eq!(
                        actual.identity,
                        Some(MixedGpuIdentity::FieldInstance(id.value()))
                    );
                    assert!(
                        (f64::from(actual.distance.unwrap()) - distance).abs() < 0.003,
                        "x={x}: {actual:?}"
                    );
                    assert!(
                        actual
                            .normal
                            .unwrap()
                            .iter()
                            .zip(normal)
                            .all(|(a, b)| (f64::from(*a) - b).abs() < 0.03),
                        "x={x}: {actual:?}"
                    );
                }
                None => assert!(!actual.hit, "x={x} guard={guard}: {actual:?}"),
            }
        }
    }
    println!(
        "QA1_HARD_UNION adapter={:?} qualified_rays=10",
        adapter.get_info()
    );
}

#[test]
#[ignore = "native GPU; independent f64 axial-capsule side/cap oracle"]
fn native_capsule_direct_and_bvh_oracle() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let mut scene = Scene::default();
    scene
        .push(
            Primitive::capsule(
                77,
                Capsule::new(v(0.0, 0.0, 0.0), v(0.0, 2.0, 0.0), 0.5).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    let mut rays = Vec::new();
    let mut references = Vec::new();
    let mut ambiguous = 0;
    for y in [-0.51_f64, -0.25, 0.5, 1.0, 2.25, 2.49, 2.51] {
        for x in [0.0, 0.2, 0.49, 0.5, 0.51] {
            let radial2: f64 = x * x + (y - y.clamp(0.0, 2.0)).powi(2);
            if (radial2 - 0.25).abs() < 0.01 {
                ambiguous += 1;
                continue;
            }
            rays.push(Ray::new(v(x, y, -3.0), v(0.0, 0.0, 1.0), 0.0, 10.0).unwrap());
            references.push((radial2 < 0.25).then(|| {
                let z = -(0.25 - radial2).sqrt();
                (3.0 + z, [2.0 * x, 2.0 * (y - y.clamp(0.0, 2.0)), 2.0 * z])
            }));
        }
    }
    for traversal in [GpuTraversal::Direct, GpuTraversal::Bvh] {
        let actual = renderer
            .query_with_traversal(&device, &queue, &scene, &rays, traversal)
            .unwrap();
        for (index, ((ray, expected), gpu)) in rays.iter().zip(&references).zip(actual).enumerate()
        {
            let cpu = scene.intersect(*ray);
            match (expected, cpu, gpu) {
                (None, None, None) => {}
                (Some((distance, normal)), Some(cpu), Some(gpu)) => {
                    assert_eq!(cpu.id, 77, "CPU ray {index}");
                    assert_eq!(gpu.id, 77, "GPU {traversal:?} ray {index}");
                    assert!((cpu.distance - distance).abs() < 1e-9, "CPU ray {index}");
                    assert!(
                        (f64::from(gpu.distance) - distance).abs() < 0.001,
                        "GPU {traversal:?} ray {index}: expected {distance}, got {}",
                        gpu.distance
                    );
                    assert!(
                        gpu.normal
                            .iter()
                            .zip(normal)
                            .all(|(a, b)| (f64::from(*a) - b).abs() < 0.01),
                        "GPU {traversal:?} ray {index} normal {:?}",
                        gpu.normal
                    );
                }
                other => panic!("{traversal:?} ray {index}: {other:?}"),
            }
        }
    }
    println!(
        "QA1_CAPSULE adapter={:?} qualified_rays={} ambiguous={} traversals=2",
        adapter.get_info(),
        rays.len(),
        ambiguous
    );
}
