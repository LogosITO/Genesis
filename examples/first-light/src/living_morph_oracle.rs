//! Independent exact ray/sphere entry oracle for the two supported living union graphs.

use analytic_renderer::{
    Camera, GpuRenderer, Ray, Scene,
    field_graph_gpu::{GraphGpuRenderer, GraphTracePolicy, WorldFieldGpuOutcome},
    mixed::{MixedRenderOptions, MixedSource, render_mixed_rgba},
};
use spatial_math::Vec3;
use world_authoring::field_graph::{TraceOptions, TraceOutcome, trace};
use world_simulation::SimulationStep;
use world_state::EntityId;

fn sphere_entry(ray: Ray, center: Vec3, radius: f64) -> Option<(f64, Vec3, f64)> {
    let o = ray.origin();
    let d = ray.direction();
    let x = o.x() - center.x();
    let y = o.y() - center.y();
    let z = o.z() - center.z();
    let b = x * d.x() + y * d.y() + z * d.z();
    let discriminant = b * b - (x * x + y * y + z * z - radius * radius);
    if discriminant < 0.0 {
        return None;
    }
    let (near, far) = ray.interval();
    let distance = -b - discriminant.sqrt();
    if distance < near || distance > far {
        return None;
    }
    let point = Vec3::new(
        o.x() + distance * d.x(),
        o.y() + distance * d.y(),
        o.z() + distance * d.z(),
    )
    .ok()?;
    let normal = Vec3::new(
        (point.x() - center.x()) / radius,
        (point.y() - center.y()) / radius,
        (point.z() - center.z()) / radius,
    )
    .ok()?;
    Some((distance, normal, discriminant))
}

fn field_oracle(
    world: &world_state::WorldState,
    ray: Ray,
) -> (Vec<(EntityId, f64, Vec3, f64)>, f64) {
    let mut hits = Vec::new();
    let mut closest_tangent = f64::INFINITY;
    for instance in world.field_instances().iter().filter(|item| item.enabled()) {
        let radii = instance
            .morphology()
            .map(|morph| morph.radii())
            .unwrap_or([0.9; 2]);
        let transform = instance.transform();
        for (lobe, radius) in radii.into_iter().enumerate() {
            let center = Vec3::new(
                transform.translation().x()
                    + (if lobe == 0 { -0.65 } else { 0.65 }) * transform.scale(),
                transform.translation().y(),
                transform.translation().z(),
            )
            .unwrap();
            let delta = ray.origin().checked_sub(center).unwrap();
            let b = delta.x() * ray.direction().x()
                + delta.y() * ray.direction().y()
                + delta.z() * ray.direction().z();
            let discriminant = b * b
                - (delta.x() * delta.x() + delta.y() * delta.y() + delta.z() * delta.z()
                    - (radius * transform.scale()).powi(2));
            closest_tangent = closest_tangent.min(discriminant.abs());
            if let Some((distance, normal, discriminant)) =
                sphere_entry(ray, center, radius * transform.scale())
            {
                hits.push((instance.id(), distance, normal, discriminant));
            }
        }
    }
    hits.sort_by(|a, b| a.1.total_cmp(&b.1));
    (hits, closest_tangent)
}

#[test]
#[ignore = "requires native GPU; independent union-of-spheres oracle on real living states"]
fn living_morph_mixed_ray_oracle() {
    let mut runtime = super::living_morph::initial(
        SimulationStep::new(super::STEP.as_secs_f64()).unwrap(),
        true,
    )
    .unwrap();
    let mut states = vec![(0, runtime.clone())];
    for tick in 1..=2_000 {
        if tick == 61 {
            runtime
                .schedule(tick, super::living::intervention(&runtime))
                .unwrap();
        }
        runtime.tick().unwrap();
        if [60, 61, 120, 240, 2_000].contains(&tick) {
            states.push((tick, runtime.clone()));
        }
    }
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("native GPU required");
    eprintln!("morph_oracle_adapter={:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let analytic =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let fields = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let camera = Camera::look_at(
        Vec3::new(0.0, 2.7, -8.0).unwrap(),
        Vec3::new(0.0, 1.1, 0.0).unwrap(),
        Vec3::new(0.0, 1.0, 0.0).unwrap(),
        0.95,
    )
    .unwrap();
    let size = [144, 80];
    let rays = (0..size[0] * size[1])
        .map(|index| {
            camera
                .ray(index % size[0], index / size[0], size[0], size[1])
                .unwrap()
        })
        .collect::<Vec<_>>();
    let policy = GraphTracePolicy::default();
    let mut checked = 0;
    let mut classified_ambiguous = 0;
    let mut gpu_uncertain = 0;
    let mut field_hits = 0;
    let mut analytic_hits = 0;
    let mut field_selected = 0;
    for (tick, state) in states {
        let world = state.world();
        let gpu_fields = fields
            .query_world_fields(&device, &queue, world, camera, &rays, policy)
            .unwrap();
        let scene = Scene::from_world(world).unwrap();
        let gpu_analytic = analytic.query(&device, &queue, &scene, &rays).unwrap();
        let mixed = render_mixed_rgba(
            &analytic,
            &fields,
            &device,
            &queue,
            world,
            camera,
            MixedRenderOptions { size, policy },
        )
        .unwrap();
        for (index, ray) in rays.iter().copied().enumerate() {
            let (oracle, tangent_discriminant) = field_oracle(world, ray);
            let first = oracle.first().copied();
            let analytic_cpu = scene.intersect(ray);
            let analytic_gpu = gpu_analytic[index];
            match (analytic_cpu, analytic_gpu) {
                (None, None) => {}
                (Some(a), Some(b)) => {
                    analytic_hits += 1;
                    assert_eq!(a.id, b.id, "tick {tick} ray {index} analytic identity");
                    assert!(
                        (a.distance - f64::from(b.distance)).abs() < 0.015,
                        "tick {tick} ray {index} analytic depth"
                    );
                }
                _ => panic!("tick {tick} ray {index} analytic hit/miss"),
            }
            // A tolerance-sized shell can register as a GPU hit just outside an ideal tangent.
            let weak = tangent_discriminant < 0.003
                || first.is_some_and(|hit| oracle.get(1).is_some_and(|next| next.1 - hit.1 < 0.02));
            if weak {
                classified_ambiguous += 1;
                continue;
            }
            match (first, gpu_fields[index]) {
                (None, WorldFieldGpuOutcome::Miss) => {}
                (
                    Some((id, distance, normal, _)),
                    WorldFieldGpuOutcome::Hit {
                        id: actual,
                        distance: gpu_distance,
                        normal: gpu_normal,
                    },
                ) => {
                    field_hits += 1;
                    assert_eq!(id, actual, "tick {tick} ray {index} field identity");
                    assert!(
                        (distance - f64::from(gpu_distance)).abs() < 0.025,
                        "tick {tick} ray {index} field depth {distance}/{gpu_distance}"
                    );
                    let dot = normal.x() * f64::from(gpu_normal[0])
                        + normal.y() * f64::from(gpu_normal[1])
                        + normal.z() * f64::from(gpu_normal[2]);
                    assert!(dot > 0.9, "tick {tick} ray {index} field normal {dot}");
                    let bounds = world.field_bounds(id).unwrap();
                    let point = ray
                        .origin()
                        .checked_add(ray.direction().checked_scale(distance).unwrap())
                        .unwrap();
                    assert!(
                        point.x() >= bounds.min.x() - 1e-8 && point.x() <= bounds.max.x() + 1e-8
                    );
                    let field = world.field_instance(id).unwrap();
                    let scale = field.transform().scale();
                    let cpu = trace(
                        world.field_graph(id).unwrap(),
                        field.transform().to_local(ray.origin()).unwrap(),
                        ray.direction(),
                        TraceOptions {
                            near: ray.interval().0 / scale,
                            far: ray.interval().1 / scale,
                            tolerance: f64::from(policy.tolerance) / scale,
                            iterations: policy.iterations,
                            safety: f64::from(policy.safety),
                        },
                    )
                    .unwrap();
                    assert!(
                        matches!(cpu, TraceOutcome::Hit { distance: local } if (local * scale - distance).abs() < 0.025),
                        "tick {tick} ray {index} CPU graph trace {cpu:?}"
                    );
                }
                (
                    _,
                    WorldFieldGpuOutcome::Uncertain { .. } | WorldFieldGpuOutcome::Exhausted { .. },
                ) => {
                    gpu_uncertain += 1;
                    continue;
                }
                _ => panic!(
                    "tick {tick} ray {index} field hit/miss mismatch: oracle={first:?} gpu={:?} tangent_discriminant={tangent_discriminant}",
                    gpu_fields[index]
                ),
            }
            let expected = match (first, analytic_cpu) {
                (None, None) => MixedSource::Miss,
                (Some((_, distance, _, _)), Some(hit))
                    if (distance - hit.distance).abs() < 0.03 =>
                {
                    classified_ambiguous += 1;
                    continue;
                }
                (Some((id, distance, _, _)), Some(hit)) if distance < hit.distance => {
                    MixedSource::Field(id)
                }
                (Some(_), Some(hit)) => MixedSource::Analytic(hit.id),
                (Some((id, _, _, _)), None) => MixedSource::Field(id),
                (None, Some(hit)) => MixedSource::Analytic(hit.id),
            };
            assert_eq!(
                mixed.sources[index], expected,
                "tick {tick} ray {index} mixed selection"
            );
            if matches!(expected, MixedSource::Field(_)) {
                field_selected += 1;
            }
            checked += 1;
        }
        eprintln!(
            "morph_oracle_tick={tick} checked={checked} field_hits={field_hits} analytic_hits={analytic_hits} field_selected={field_selected} classified_ambiguous={classified_ambiguous} gpu_uncertain={gpu_uncertain}"
        );
    }
    assert!(checked > 60_000);
    assert!(field_hits > 1_000 && analytic_hits > 500 && field_selected > 1_000);
    assert!(gpu_uncertain < 50);
}
