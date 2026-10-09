//! Opt-in native GPU parity and offscreen readback. Run with `cargo test -p analytic-renderer --test gpu -- --ignored --nocapture`.

use analytic_field::{
    AxisAlignedBox, Capsule, Ray as FieldRay, RayOptions, RayOutcome, Sphere, trace,
};
use analytic_renderer::{
    Bvh, Camera, DrawOptions, GpuRenderer, GpuTimer, GpuTraversal, Primitive, Ray, Scene,
    SemanticTarget,
};
use spatial_math::{Transform, Vec3};
use std::{sync::mpsc, time::Instant};
use world_state::{DeterministicSeed, GrowthParameters, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}
fn r(o: Vec3, d: Vec3) -> Ray {
    Ray::new(o, d, 0.0, 100.0).unwrap()
}

fn four_organism_world() -> WorldState {
    let mut world = WorldState::new(DeterministicSeed(7));
    for index in 0..4 {
        world
            .spawn_organism(
                v(index as f64 - 1.5, 0.0, 0.0),
                GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
            )
            .unwrap();
    }
    world.spawn_source(v(0.0, 2.0, 0.0), 100.0, 10.0).unwrap();
    let mut time = world_simulation::SimulationTime::default();
    let step = world_simulation::SimulationStep::new(0.01).unwrap();
    while world
        .organisms()
        .iter()
        .any(|organism| organism.nodes().len() < 48)
    {
        assert!(time.ticks() < 200, "stress world failed to reach 192 nodes");
        world_simulation::advance_life(&mut world, &mut time, step, &[]).unwrap();
    }
    assert_eq!(
        world
            .organisms()
            .iter()
            .map(|o| o.nodes().len())
            .sum::<usize>(),
        192
    );
    world
}

#[test]
fn cpu_bvh_matches_real_381_primitive_world() {
    let scene = Scene::from_world(&four_organism_world()).unwrap();
    assert_eq!(scene.primitives().len(), 381);
    let bvh = Bvh::build(&scene).unwrap();
    for x in -12..=12 {
        for y in 0..=20 {
            let ray = r(v(x as f64 * 0.25, y as f64 * 0.25, -8.0), v(0.0, 0.0, 1.0));
            let direct = scene.intersect(ray);
            let accelerated = bvh.intersect(ray);
            assert_eq!(direct.map(|hit| hit.id), accelerated.map(|hit| hit.id));
            if let (Some(direct), Some(accelerated)) = (direct, accelerated) {
                assert_eq!(direct.distance, accelerated.distance);
                assert_eq!(direct.normal, accelerated.normal);
            }
        }
    }
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_bvh_broad_real_scene_differential() {
    let scene = Scene::from_world(&four_organism_world()).unwrap();
    let bvh = Bvh::build(&scene).unwrap();
    let mut rays: Vec<_> = (0..160)
        .flat_map(|x| {
            (0..100).map(move |y| {
                r(
                    v((x as f64 - 80.0) * 0.06, y as f64 * 0.06, -8.0),
                    v(0.0, 0.0, 1.0),
                )
            })
        })
        .collect();
    for x in -20..=20 {
        for y in 0..=20 {
            rays.push(r(
                v(-5.0, y as f64 * 0.25, -8.0),
                v(1.0, 0.0, 0.1 + x as f64 * 0.025),
            ));
        }
    }
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let direct = renderer.query(&device, &queue, &scene, &rays).unwrap();
    let accelerated = renderer
        .query_with_traversal(&device, &queue, &scene, &rays, GpuTraversal::Bvh)
        .unwrap();
    let mut hits = 0;
    let mut cpu_gpu_differences = 0;
    for (index, ((ray, direct), accelerated)) in rays
        .iter()
        .zip(direct.iter().copied())
        .zip(accelerated)
        .enumerate()
    {
        let cpu = scene.intersect(*ray);
        let cpu_bvh = bvh.intersect(*ray);
        assert_eq!(
            cpu.map(|hit| hit.id),
            cpu_bvh.map(|hit| hit.id),
            "CPU ray {index}"
        );
        if let (Some(cpu), Some(cpu_bvh)) = (cpu, cpu_bvh) {
            assert_eq!(cpu.distance, cpu_bvh.distance, "CPU ray {index}");
            assert_eq!(cpu.normal, cpu_bvh.normal, "CPU ray {index}");
        }
        assert_eq!(
            direct.map(|hit| hit.id),
            accelerated.map(|hit| hit.id),
            "GPU ray {index}"
        );
        if let (Some(direct), Some(accelerated)) = (direct, accelerated) {
            assert_eq!(direct.distance, accelerated.distance, "GPU ray {index}");
            assert_eq!(direct.normal, accelerated.normal, "GPU ray {index}");
            hits += 1;
        }
        match (cpu, direct) {
            (None, None) => {}
            (Some(cpu), Some(gpu)) => {
                let tolerance = 5e-4_f64.max(cpu.distance * 3e-4);
                assert!(
                    (cpu.distance - f64::from(gpu.distance)).abs() <= tolerance,
                    "CPU/GPU ray {index} distance"
                );
                if cpu.id == gpu.id {
                    let alignment = cpu.normal.x() * f64::from(gpu.normal[0])
                        + cpu.normal.y() * f64::from(gpu.normal[1])
                        + cpu.normal.z() * f64::from(gpu.normal[2]);
                    assert!(alignment >= 0.98, "CPU/GPU ray {index} normal");
                } else {
                    let selected = scene
                        .primitives()
                        .iter()
                        .find(|primitive| primitive.id == gpu.id)
                        .unwrap();
                    let mut isolated = Scene::default();
                    isolated.push(*selected).unwrap();
                    let alternate = isolated
                        .intersect(*ray)
                        .expect("GPU-selected primitive must intersect CPU ray");
                    assert!(
                        (alternate.distance - cpu.distance).abs() <= tolerance,
                        "CPU/GPU ray {index} is not a near tie"
                    );
                    cpu_gpu_differences += 1;
                }
            }
            _ => panic!("CPU/GPU ray {index} classification mismatch"),
        }
    }
    assert!(hits > 1000, "coverage must include visible geometry");
    eprintln!(
        "broad differential rays={} hits={hits} CPU/GPU ID differences={cpu_gpu_differences} nodes={} depth={} GPU BVH bytes={}",
        rays.len(),
        bvh.node_count(),
        bvh.depth(),
        renderer.acceleration_stats().unwrap().allocated_bytes
    );
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_bvh_analytic_edge_cases() {
    let mut scene = Scene::default();
    for (id, center, radius) in [
        (1, v(0.0, 0.0, 0.0), 1.0),
        (2, v(0.0, 0.0, 0.0), 1.0),
        (6, v(0.0, -2.0, 0.0), 0.0001),
        (7, v(0.0, 0.0, 3000.0), 1000.0),
        (8, v(0.0, 0.0, 0.00001), 1.0),
    ] {
        scene
            .push(
                Primitive::sphere(
                    id,
                    Sphere::new(radius).unwrap(),
                    Transform::new(center, 1.0).unwrap(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
    }
    scene
        .push(
            Primitive::axis_aligned_box(
                3,
                AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap(),
                Transform::new(v(3.0, 0.0, 0.0), 1.0).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .push(
            Primitive::capsule(
                4,
                Capsule::new(v(-3.0, 0.0, 0.0), v(-3.0, 0.0, 0.0), 0.5).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .push(
            Primitive::capsule(
                5,
                Capsule::new(v(-500.0, 2.0, 0.0), v(500.0, 2.0, 0.0), 0.2).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    let rays = [
        r(v(0.0, 0.0, -5.0), v(0.0, 0.0, 1.0)),
        r(v(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0)),
        r(v(-2.0, 1.0, 0.0), v(1.0, 0.0, 0.0)),
        r(v(-2.0, 1.00001, 0.0), v(1.0, 0.0, 0.0)),
        r(v(3.0, 0.0, -5.0), v(0.0, 0.0, 1.0)),
        r(v(4.0, 0.0, -5.0), v(0.0, 0.0, 1.0)),
        r(v(-3.0, 0.0, -5.0), v(0.0, 0.0, 1.0)),
        r(v(400.0, 2.0, -5.0), v(0.0, 0.0, 1.0)),
        r(v(0.0, -2.0, -0.01), v(0.0, 0.0, 1.0)),
        r(v(0.0, 0.0, 1500.0), v(0.0, 0.0, 1.0)),
        Ray::new(v(0.0, 0.0, -5.0), v(0.0, 0.0, 1.0), 4.5, 6.0).unwrap(),
        Ray::new(v(0.0, 0.0, -5.0), v(0.0, 0.0, 1.0), 0.0, 3.0).unwrap(),
    ];
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let direct = renderer.query(&device, &queue, &scene, &rays).unwrap();
    let accelerated = renderer
        .query_with_traversal(&device, &queue, &scene, &rays, GpuTraversal::Bvh)
        .unwrap();
    let bvh = Bvh::build(&scene).unwrap();
    for (index, ((ray, direct), accelerated)) in rays
        .iter()
        .zip(direct.iter().copied())
        .zip(accelerated)
        .enumerate()
    {
        let cpu = scene.intersect(*ray);
        let cpu_bvh = bvh.intersect(*ray);
        assert_eq!(
            cpu.map(|hit| hit.id),
            cpu_bvh.map(|hit| hit.id),
            "CPU edge ray {index}"
        );
        assert_eq!(
            direct.map(|hit| hit.id),
            accelerated.map(|hit| hit.id),
            "GPU edge ray {index}"
        );
        if let (Some(direct), Some(accelerated)) = (direct, accelerated) {
            assert_eq!(
                direct.distance, accelerated.distance,
                "GPU edge ray {index}"
            );
            assert_eq!(direct.normal, accelerated.normal, "GPU edge ray {index}");
        }
    }
    assert_eq!(
        direct[0].unwrap().id,
        1,
        "equal-depth ties retain insertion order"
    );
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_bvh_rebuilds_after_growth_and_source_movement() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let mut world = WorldState::new(DeterministicSeed(7));
    world
        .spawn_organism(
            Vec3::ZERO,
            GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
        )
        .unwrap();
    world.spawn_source(v(0.0, 2.0, 0.0), 100.0, 10.0).unwrap();
    let mut time = world_simulation::SimulationTime::default();
    let step = world_simulation::SimulationStep::new(0.01).unwrap();
    let rays: Vec<_> = (-16..=16)
        .flat_map(|x| {
            (0..=24).map(move |y| r(v(x as f64 * 0.2, y as f64 * 0.2, -6.0), v(0.0, 0.0, 1.0)))
        })
        .collect();
    for target in [1, 8, 16, 32, 48] {
        while world.organisms()[0].nodes().len() < target {
            world_simulation::advance_life(&mut world, &mut time, step, &[]).unwrap();
        }
        let original = world.clone();
        let scene = Scene::from_world(&world).unwrap();
        renderer
            .query_with_traversal(&device, &queue, &scene, &rays[..1], GpuTraversal::Auto)
            .unwrap();
        let auto_upload = renderer.last_upload_stats().unwrap();
        assert_eq!(auto_upload.bvh_bytes > 0, scene.primitives().len() > 4);
        let direct = renderer.query(&device, &queue, &scene, &rays).unwrap();
        let accelerated = renderer
            .query_with_traversal(&device, &queue, &scene, &rays, GpuTraversal::Bvh)
            .unwrap();
        for (direct, accelerated) in direct.into_iter().zip(accelerated) {
            assert_eq!(
                direct.map(|hit| hit.id),
                accelerated.map(|hit| hit.id),
                "nodes={target}"
            );
        }
        let builds = renderer.acceleration_stats().unwrap().rebuilds;
        renderer
            .query_with_traversal(&device, &queue, &scene, &rays[..1], GpuTraversal::Bvh)
            .unwrap();
        assert_eq!(
            renderer.acceleration_stats().unwrap().rebuilds,
            builds,
            "unchanged snapshot must reuse BVH"
        );
        assert_eq!(
            world, original,
            "renderer cannot mutate authoritative world"
        );
    }
    let previous = renderer.acceleration_stats().unwrap().rebuilds;
    let source = world.sources()[0].id();
    world
        .source_mut(source)
        .unwrap()
        .move_to(v(4.0, 2.0, 0.0))
        .unwrap();
    let moved = Scene::from_world(&world).unwrap();
    let ray = r(v(4.0, 2.0, -5.0), v(0.0, 0.0, 1.0));
    let direct = renderer.query(&device, &queue, &moved, &[ray]).unwrap();
    let accelerated = renderer
        .query_with_traversal(&device, &queue, &moved, &[ray], GpuTraversal::Bvh)
        .unwrap();
    assert_eq!(
        direct[0].map(|hit| hit.id),
        accelerated[0].map(|hit| hit.id)
    );
    assert_eq!(
        renderer.acceleration_stats().unwrap().rebuilds,
        previous + 1
    );
    let camera =
        Camera::look_at(v(0.0, 3.0, -10.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap();
    let render = |scene: &Scene, traversal| {
        let image = renderer
            .draw_with_traversal(
                &device,
                &queue,
                scene,
                camera,
                traversal,
                DrawOptions {
                    size: [800, 450],
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
            )
            .unwrap();
        read_rgb(&device, &queue, &image, 800, 450)
    };
    let before_pixels = render(&moved, GpuTraversal::Bvh);
    let organism = world.organisms()[0].id();
    let removed = world.prune_branch(organism, 1).unwrap();
    assert!(removed > 1);
    let pruned = Scene::from_world(&world).unwrap();
    assert!(pruned.primitives().len() < moved.primitives().len());
    assert!(
        pruned
            .primitives()
            .iter()
            .all(|p| p.target() != Some(SemanticTarget::Connection { organism, child: 1 }))
    );
    let before_prune_rebuilds = renderer.acceleration_stats().unwrap().rebuilds;
    let cpu_bvh = Bvh::build(&pruned).unwrap();
    let rays: Vec<_> = (-20..=20)
        .flat_map(|x| {
            (0..=24).map(move |y| r(v(x as f64 * 0.2, y as f64 * 0.2, -6.0), v(0.0, 0.0, 1.0)))
        })
        .collect();
    let direct = renderer.query(&device, &queue, &pruned, &rays).unwrap();
    let accelerated = renderer
        .query_with_traversal(&device, &queue, &pruned, &rays, GpuTraversal::Bvh)
        .unwrap();
    for ((ray, direct), accelerated) in rays.iter().zip(direct).zip(accelerated) {
        assert_eq!(
            pruned.intersect(*ray).map(|hit| hit.id),
            cpu_bvh.intersect(*ray).map(|hit| hit.id)
        );
        assert_eq!(direct.map(|hit| hit.id), accelerated.map(|hit| hit.id));
    }
    assert_eq!(
        renderer.acceleration_stats().unwrap().rebuilds,
        before_prune_rebuilds + 1
    );
    let after_direct = render(&pruned, GpuTraversal::Direct);
    let after_bvh = render(&pruned, GpuTraversal::Bvh);
    assert_eq!(after_direct, after_bvh);
    let changed_pixels = before_pixels
        .chunks_exact(3)
        .zip(after_bvh.chunks_exact(3))
        .filter(|(before, after)| before != after)
        .count();
    assert!(
        changed_pixels > 0,
        "pruning must visibly change the real GPU image"
    );
    if let Some(directory) = std::env::var_os("GENESIS_CAPTURE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        for (name, pixels) in [
            ("prune-before", &before_pixels),
            ("prune-after-direct", &after_direct),
            ("prune-after-bvh", &after_bvh),
        ] {
            let mut ppm = b"P6\n800 450\n255\n".to_vec();
            ppm.extend_from_slice(pixels);
            std::fs::write(
                std::path::Path::new(&directory).join(format!("{name}.ppm")),
                ppm,
            )
            .unwrap();
        }
    }
    eprintln!(
        "prune_gpu before={} after={} changed_pixels={changed_pixels} rebuilds={}",
        moved.primitives().len(),
        pruned.primitives().len(),
        renderer.acceleration_stats().unwrap().rebuilds
    );
}

#[test]
#[ignore = "local GPU timing experiment; run serially in release"]
fn gpu_pruning_update_benchmark() {
    fn stats(mut values: Vec<f64>) -> (f64, f64) {
        values.sort_by(f64::total_cmp);
        (
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1],
        )
    }
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let mut world = four_organism_world();
    let before = Scene::from_world(&world).unwrap();
    let camera =
        Camera::look_at(v(0.0, 3.0, -12.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap();
    let draw = |scene: &Scene| {
        let started = Instant::now();
        let _image = renderer
            .draw_with_traversal(
                &device,
                &queue,
                scene,
                camera,
                GpuTraversal::Bvh,
                DrawOptions {
                    size: [1280, 720],
                    normal_debug: false,
                    surface: None,
                    timer: timer.as_ref(),
                },
            )
            .unwrap();
        let gpu_ms = if let Some(timer) = &timer {
            Some(timer.read_ms(&device, &queue).unwrap())
        } else {
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            None
        };
        (started.elapsed().as_secs_f64() * 1e3, gpu_ms)
    };
    for _ in 0..3 {
        draw(&before);
    }
    let mut before_frame = Vec::new();
    let mut before_gpu = Vec::new();
    for _ in 0..10 {
        let (frame, gpu) = draw(&before);
        before_frame.push(frame);
        if let Some(gpu) = gpu {
            before_gpu.push(gpu);
        }
    }
    let organism = world.organisms()[0].id();
    let removed = world.prune_branch(organism, 1).unwrap();
    let after = Scene::from_world(&world).unwrap();
    let first_changed = draw(&after);
    let upload = renderer.last_upload_stats().unwrap();
    assert!(upload.bvh_bytes > 0);
    let mut after_frame = Vec::new();
    let mut after_gpu = Vec::new();
    for _ in 0..10 {
        let (frame, gpu) = draw(&after);
        after_frame.push(frame);
        if let Some(gpu) = gpu {
            after_gpu.push(gpu);
        }
    }
    assert_eq!(renderer.last_upload_stats().unwrap().bvh_bytes, 0);
    eprintln!(
        "prune_gpu_benchmark adapter={:?} before={} after={} removed_nodes={removed} resolution=1280x720 profile=release stable_before_frame_ms={:?} stable_before_gpu_ms={:?} first_changed={first_changed:?} first_upload={upload:?} stable_after_frame_ms={:?} stable_after_gpu_ms={:?}",
        adapter.get_info(),
        before.primitives().len(),
        after.primitives().len(),
        stats(before_frame),
        if before_gpu.is_empty() {
            None
        } else {
            Some(stats(before_gpu))
        },
        stats(after_frame),
        if after_gpu.is_empty() {
            None
        } else {
            Some(stats(after_gpu))
        }
    );
}

#[test]
#[ignore = "timing experiment; run serially and explicitly"]
fn cpu_real_381_bvh_benchmark() {
    fn stats(mut values: Vec<f64>) -> (f64, f64) {
        values.sort_by(f64::total_cmp);
        (
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1],
        )
    }
    let scene = Scene::from_world(&four_organism_world()).unwrap();
    let rays: Vec<_> = (-12..=12)
        .flat_map(|x| {
            (0..=20).map(move |y| r(v(x as f64 * 0.25, y as f64 * 0.25, -8.0), v(0.0, 0.0, 1.0)))
        })
        .collect();
    let mut build_ms = Vec::new();
    for _ in 0..25 {
        let started = Instant::now();
        std::hint::black_box(analytic_renderer::Bvh::build(&scene).unwrap());
        build_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let bvh = Bvh::build(&scene).unwrap();
    let mut direct_ms = Vec::new();
    let mut bvh_ms = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        for ray in &rays {
            std::hint::black_box(scene.intersect(*ray));
        }
        direct_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        for ray in &rays {
            std::hint::black_box(bvh.intersect(*ray));
        }
        bvh_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    eprintln!(
        "cpu_real_381 rays={} nodes={} depth={} bvh_bytes={} build_ms={:?} direct_ms={:?} bvh_ms={:?}",
        rays.len(),
        bvh.node_count(),
        bvh.depth(),
        bvh.allocated_bytes(),
        stats(build_ms),
        stats(direct_ms),
        stats(bvh_ms)
    );
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_analytic_parity_and_image_readback() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    eprintln!("GPU parity adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();

    let empty = Scene::default();
    assert!(
        renderer
            .query(
                &device,
                &queue,
                &empty,
                &[r(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0))]
            )
            .unwrap()[0]
            .is_none()
    );

    let mut scene = Scene::default();
    scene
        .push(
            Primitive::sphere(
                11,
                Sphere::new(1.0).unwrap(),
                Transform::identity(),
                [0.2, 0.7, 0.9],
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .push(
            Primitive::sphere(
                33,
                Sphere::new(1.0).unwrap(),
                Transform::new(v(0.0, 0.0, 4.0), 1.0).unwrap(),
                [0.8, 0.2, 0.7],
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .push(
            Primitive::axis_aligned_box(
                22,
                AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap(),
                Transform::new(v(3.0, 0.0, 2.0), 1.0).unwrap(),
                [0.9, 0.4, 0.2],
            )
            .unwrap(),
        )
        .unwrap();
    let rays = [
        r(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0)),   // sphere center
        r(v(-2.0, 1.0, 0.0), v(1.0, 0.0, 0.0)),   // tangent
        r(v(-2.0, 1.001, 0.0), v(1.0, 0.0, 0.0)), // near tangent miss
        r(Vec3::ZERO, v(1.0, 0.0, 0.0)),          // sphere interior
        r(v(0.0, 3.0, -3.0), v(0.0, 0.0, 1.0)),   // miss
        r(v(3.0, 0.0, -3.0), v(0.0, 0.0, 1.0)),   // box center
        r(v(4.5, 0.0, -3.0), v(0.0, 0.0, 1.0)),   // parallel box miss
        r(v(3.0, 0.0, 2.0), v(1.0, 0.0, 0.0)),    // box interior
        r(v(0.0, 0.0, -1.0), v(0.0, 0.0, 1.0)),   // sphere surface
        Ray::new(v(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), 0.001, 100.0).unwrap(), // deeper sphere
    ];
    let gpu = renderer.query(&device, &queue, &scene, &rays).unwrap();
    let gpu_bvh = renderer
        .query_with_traversal(&device, &queue, &scene, &rays, GpuTraversal::Bvh)
        .unwrap();
    for (direct, accelerated) in gpu.iter().zip(&gpu_bvh) {
        assert_eq!(direct.map(|hit| hit.id), accelerated.map(|hit| hit.id));
        if let (Some(direct), Some(accelerated)) = (direct, accelerated) {
            assert_eq!(direct.distance, accelerated.distance);
            assert_eq!(direct.normal, accelerated.normal);
        }
    }
    for (index, (ray, actual)) in rays.iter().zip(gpu).enumerate() {
        let expected = scene.intersect(*ray);
        match (expected, actual) {
            (None, None) => {}
            (Some(cpu), Some(gpu)) => {
                assert_eq!(cpu.id, gpu.id, "ray {index} object identity");
                assert!(
                    (cpu.distance - f64::from(gpu.distance)).abs()
                        <= 2e-4_f64.max(cpu.distance * 2e-4),
                    "ray {index} distance CPU={} GPU={}",
                    cpu.distance,
                    gpu.distance
                );
                let alignment = cpu.normal.x() * f64::from(gpu.normal[0])
                    + cpu.normal.y() * f64::from(gpu.normal[1])
                    + cpu.normal.z() * f64::from(gpu.normal[2]);
                assert!(alignment >= 0.999, "ray {index} normal dot={alignment}");
            }
            other => panic!("ray {index} classification mismatch: {other:?}"),
        }
    }
    let mut capsule_scene = Scene::default();
    capsule_scene
        .push(
            Primitive::capsule(
                77,
                Capsule::new(v(0.0, 0.0, 0.0), v(0.0, 2.0, 0.0), 0.5).unwrap(),
                [0.3, 0.8, 0.4],
            )
            .unwrap(),
        )
        .unwrap();
    let capsule_rays = [
        r(v(0.0, 1.0, -3.0), v(0.0, 0.0, 1.0)),
        r(v(0.0, 3.0, 0.0), v(0.0, -1.0, 0.0)),
        r(v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)),
        r(v(0.5, 1.0, -2.0), v(0.0, 0.0, 1.0)),
        r(v(0.501, 1.0, -2.0), v(0.0, 0.0, 1.0)),
        r(v(0.51, 1.0, -3.0), v(0.0, 1e-6, 1.0)),
        r(v(0.0, 2.0, -3.0), v(0.0, 0.0, 1.0)),
    ];
    for (index, (ray, actual)) in capsule_rays
        .iter()
        .zip(
            renderer
                .query(&device, &queue, &capsule_scene, &capsule_rays)
                .unwrap(),
        )
        .enumerate()
    {
        match (capsule_scene.intersect(*ray), actual) {
            (None, None) => {}
            (Some(cpu), Some(gpu)) => {
                assert_eq!(cpu.id, gpu.id, "capsule ray {index}");
                assert!(
                    (cpu.distance - f64::from(gpu.distance)).abs() <= 3e-4,
                    "capsule ray {index} distance"
                );
                let alignment = cpu.normal.x() * f64::from(gpu.normal[0])
                    + cpu.normal.y() * f64::from(gpu.normal[1])
                    + cpu.normal.z() * f64::from(gpu.normal[2]);
                assert!(alignment >= 0.998, "capsule ray {index} normal");
            }
            other => panic!("capsule ray {index} classification {other:?}"),
        }
    }
    let mut degenerate = Scene::default();
    degenerate
        .push(
            Primitive::capsule(
                78,
                Capsule::new(Vec3::ZERO, Vec3::ZERO, 0.5).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        (f64::from(
            renderer
                .query(
                    &device,
                    &queue,
                    &degenerate,
                    &[r(v(0.0, 0.0, -2.0), v(0.0, 0.0, 1.0))]
                )
                .unwrap()[0]
                .unwrap()
                .distance
        ) - 1.5)
            .abs()
            < 3e-4
    );
    let mut short = Scene::default();
    short
        .push(
            Primitive::capsule(
                79,
                Capsule::new(Vec3::ZERO, v(0.0, 1e-8, 0.0), 0.5).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    let short_ray = r(v(0.0, 0.0, -2.0), v(0.0, 0.0, 1.0));
    let cpu = short.intersect(short_ray).unwrap();
    let gpu = renderer
        .query(&device, &queue, &short, &[short_ray])
        .unwrap()[0]
        .unwrap();
    assert!((cpu.distance - f64::from(gpu.distance)).abs() < 3e-4);
    // Existing bounded SDF query agrees for an exterior, non-grazing sphere ray.
    let reference = trace(
        &Sphere::new(1.0).unwrap(),
        FieldRay::new(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0), 5.0).unwrap(),
        RayOptions::new(1e-8, 64).unwrap(),
    );
    assert!(
        matches!(reference, RayOutcome::Hit { distance, .. } if (distance - 2.0).abs() <= 1e-8)
    );
    let box_reference = trace(
        &AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap(),
        FieldRay::new(v(0.0, 0.0, -5.0), v(0.0, 0.0, 1.0), 10.0).unwrap(),
        RayOptions::new(1e-8, 64).unwrap(),
    );
    assert!(
        matches!(box_reference, RayOutcome::Hit { distance, .. } if (distance - 4.0).abs() <= 1e-8)
    );
    assert!(
        renderer
            .query(&device, &queue, &Scene::default(), &rays[..1])
            .unwrap()[0]
            .is_none()
    );

    for (radius, distance, expected) in [(0.001, 0.01, 0.009), (1000.0, 3000.0, 2000.0)] {
        let mut scaled = Scene::default();
        scaled
            .push(
                Primitive::sphere(
                    44,
                    Sphere::new(radius).unwrap(),
                    Transform::identity(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        let ray = Ray::new(
            v(0.0, 0.0, -distance),
            v(0.0, 0.0, 1.0),
            0.0,
            distance * 2.0,
        )
        .unwrap();
        let hit = renderer.query(&device, &queue, &scaled, &[ray]).unwrap()[0].unwrap();
        assert_eq!(hit.id, 44);
        assert!((f64::from(hit.distance) - expected).abs() <= 2e-4_f64.max(expected * 2e-4));
    }

    let camera = Camera::look_at(v(0.0, 0.0, -3.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let image = renderer
        .draw(
            &device,
            &queue,
            &scene,
            camera,
            DrawOptions {
                size: [3, 3],
                normal_debug: true,
                surface: None,
                timer: None,
            },
        )
        .unwrap();
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("offscreen readback"),
        size: 256 * 3,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("offscreen copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(3),
            },
        },
        wgpu::Extent3d {
            width: 3,
            height: 3,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = output.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let center = &bytes[256 + 4..256 + 8];
    let corner = &bytes[0..4];
    assert_eq!(center[3], 255);
    assert_ne!(
        center, corner,
        "computed center pixel must differ from background"
    );
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_resolution_and_timestamp_smoke() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let mut scene = Scene::default();
    scene
        .push(
            Primitive::sphere(
                0,
                Sphere::new(1.0).unwrap(),
                Transform::identity(),
                [0.2, 0.7, 0.9],
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .push(
            Primitive::axis_aligned_box(
                1,
                AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap(),
                Transform::new(v(2.5, 0.0, 0.0), 1.0).unwrap(),
                [0.9, 0.4, 0.2],
            )
            .unwrap(),
        )
        .unwrap();
    let camera = Camera::look_at(v(0.0, 1.0, -7.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.95).unwrap();
    for [width, height] in [[1280, 720], [1920, 1080], [2560, 1440]] {
        let mut cpu_ms = Vec::new();
        let mut gpu_ms = Vec::new();
        for i in 0..8 {
            let start = Instant::now();
            let _image = renderer
                .draw(
                    &device,
                    &queue,
                    &scene,
                    camera,
                    DrawOptions {
                        size: [width, height],
                        normal_debug: false,
                        surface: None,
                        timer: timer.as_ref(),
                    },
                )
                .unwrap();
            let submit_ms = start.elapsed().as_secs_f64() * 1000.0;
            let compute_ms = timer.as_ref().map(|t| t.read_ms(&device, &queue).unwrap());
            if i >= 3 {
                cpu_ms.push(submit_ms);
                if let Some(ms) = compute_ms {
                    assert!(ms.is_finite() && ms > 0.0);
                    gpu_ms.push(ms);
                }
            }
        }
        cpu_ms.sort_by(f64::total_cmp);
        gpu_ms.sort_by(f64::total_cmp);
        eprintln!(
            "resolution={}x{} objects=2 profile=dev cpu_submit_median_ms={:.3} gpu_compute_median_ms={} adapter={}",
            width,
            height,
            cpu_ms[cpu_ms.len() / 2],
            gpu_ms
                .get(gpu_ms.len() / 2)
                .map(|n| format!("{n:.3}"))
                .unwrap_or_else(|| "unavailable".to_string()),
            adapter.get_info().name
        );
    }
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_first_life_snapshot_matches_cpu() {
    let mut world = WorldState::new(DeterministicSeed(7));
    world
        .spawn_organism(
            Vec3::ZERO,
            GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
        )
        .unwrap();
    world.spawn_source(v(0.0, 2.0, 0.0), 4.0, 1.0).unwrap();
    let mut time = world_simulation::SimulationTime::default();
    let step = world_simulation::SimulationStep::new(1.0 / 60.0).unwrap();
    for _ in 0..120 {
        world_simulation::advance_life(&mut world, &mut time, step, &[]).unwrap();
    }
    let scene = Scene::from_world(&world).unwrap();
    assert_eq!(
        scene.primitives().len(),
        world.organisms()[0].nodes().len() * 2
    );
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let rays: Vec<_> = scene
        .primitives()
        .iter()
        .map(|primitive| {
            let center = primitive.center();
            r(v(center.x(), center.y(), -3.0), v(0.0, 0.0, 1.0))
        })
        .collect();
    let gpu = renderer.query(&device, &queue, &scene, &rays).unwrap();
    for (index, (ray, actual)) in rays.iter().zip(gpu).enumerate() {
        let expected = scene.intersect(*ray).unwrap();
        let actual = actual.expect("GPU hit");
        assert_eq!(actual.id, expected.id, "ray {index} identity");
        assert!(
            (f64::from(actual.distance) - expected.distance).abs() <= 2e-4,
            "ray {index} distance"
        );
    }
    let timer = GpuTimer::new(&device);
    let camera =
        Camera::look_at(v(0.0, 1.0, -7.0), v(0.0, 1.0, 0.0), v(0.0, 1.0, 0.0), 0.95).unwrap();
    let mut submit = Vec::new();
    let mut compute = Vec::new();
    for i in 0..8 {
        let started = Instant::now();
        let _image = renderer
            .draw_with_traversal(
                &device,
                &queue,
                &scene,
                camera,
                GpuTraversal::Direct,
                DrawOptions {
                    size: [1280, 720],
                    normal_debug: false,
                    surface: None,
                    timer: timer.as_ref(),
                },
            )
            .unwrap();
        let cpu = started.elapsed().as_secs_f64() * 1000.0;
        let gpu = timer
            .as_ref()
            .map(|timer| timer.read_ms(&device, &queue).unwrap());
        if i >= 3 {
            submit.push(cpu);
            if let Some(gpu) = gpu {
                compute.push(gpu);
            }
        }
    }
    submit.sort_by(f64::total_cmp);
    compute.sort_by(f64::total_cmp);
    eprintln!(
        "first_life_gpu_parity nodes={} primitives={} resolution=1280x720 profile=dev cpu_submit_median_ms={:.3} gpu_compute_median_ms={} adapter={} backend={:?}",
        world.organisms()[0].nodes().len(),
        scene.primitives().len(),
        submit[submit.len() / 2],
        compute
            .get(compute.len() / 2)
            .map(|n| format!("{n:.3}"))
            .unwrap_or_else(|| "unavailable".to_string()),
        adapter.get_info().name,
        adapter.get_info().backend
    );
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_growth_scaling_measurements() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let camera =
        Camera::look_at(v(0.0, 4.0, -12.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.95).unwrap();
    let mut world = WorldState::new(DeterministicSeed(7));
    world
        .spawn_organism(
            Vec3::ZERO,
            GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
        )
        .unwrap();
    world.spawn_source(v(0.0, 2.0, 0.0), 100.0, 10.0).unwrap();
    let mut time = world_simulation::SimulationTime::default();
    let step = world_simulation::SimulationStep::new(0.01).unwrap();
    for target in [1, 8, 16, 32, 48] {
        while world.organisms()[0].nodes().len() < target {
            world_simulation::advance_life(&mut world, &mut time, step, &[]).unwrap();
        }
        let scene = Scene::from_world(&world).unwrap();
        let mut submit = Vec::new();
        let mut frame = Vec::new();
        let mut compute = Vec::new();
        for i in 0..8 {
            let started = Instant::now();
            renderer
                .draw_with_traversal(
                    &device,
                    &queue,
                    &scene,
                    camera,
                    GpuTraversal::Direct,
                    DrawOptions {
                        size: [1280, 720],
                        normal_debug: false,
                        surface: None,
                        timer: timer.as_ref(),
                    },
                )
                .unwrap();
            let submitted = started.elapsed().as_secs_f64() * 1000.0;
            let gpu = timer
                .as_ref()
                .map(|timer| timer.read_ms(&device, &queue).unwrap());
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if i >= 3 {
                submit.push(submitted);
                frame.push(elapsed);
                if let Some(value) = gpu {
                    compute.push(value);
                }
            }
        }
        submit.sort_by(f64::total_cmp);
        frame.sort_by(f64::total_cmp);
        compute.sort_by(f64::total_cmp);
        eprintln!(
            "growth_scale nodes={} primitives={} resolution=1280x720 cpu_submit_median_ms={:.3} full_frame_median_ms={:.3} gpu_compute_median_ms={} adapter={} backend={:?}",
            world.organisms()[0].nodes().len(),
            scene.primitives().len(),
            submit[2],
            frame[2],
            compute
                .get(compute.len() / 2)
                .map(|x| format!("{x:.3}"))
                .unwrap_or_else(|| "unavailable".into()),
            adapter.get_info().name,
            adapter.get_info().backend
        );
    }
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_fixed_scene_scale_benchmark() {
    fn stats(mut values: Vec<f64>) -> (f64, f64) {
        values.sort_by(f64::total_cmp);
        (
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1],
        )
    }
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let camera = Camera::look_at(v(0.0, 0.0, -20.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.95).unwrap();
    eprintln!(
        "fixed_scale adapter={:?} limits={:?} timestamp={}",
        adapter.get_info(),
        device.limits(),
        timer.is_some()
    );
    let mut world = WorldState::new(DeterministicSeed(7));
    for count in [1, 13, 48, 128, 256, 381] {
        while world.entities().len() < count {
            let index = world.entities().len();
            let x = (index % 16) as f64 * 0.45 - 3.375;
            let y = (index / 16) as f64 * 0.45 - 5.4;
            world
                .spawn_sphere(
                    Sphere::new(0.18).unwrap(),
                    Transform::new(v(x, y, 0.0), 1.0).unwrap(),
                    0.0,
                )
                .unwrap();
        }
        let mut snapshot_ms = Vec::new();
        let mut scene = None;
        for _ in 0..25 {
            let start = Instant::now();
            match Scene::from_world(&world) {
                Ok(value) => scene = Some(value),
                Err(error) => {
                    eprintln!("fixed_scale count={count} unsupported={error}");
                    break;
                }
            }
            snapshot_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let Some(scene) = scene else { continue };
        let mut submit_ms = Vec::new();
        let mut frame_ms = Vec::new();
        let mut gpu_ms = Vec::new();
        for i in 0..13 {
            let start = Instant::now();
            renderer
                .draw_with_traversal(
                    &device,
                    &queue,
                    &scene,
                    camera,
                    GpuTraversal::Direct,
                    DrawOptions {
                        size: [1280, 720],
                        normal_debug: false,
                        surface: None,
                        timer: timer.as_ref(),
                    },
                )
                .unwrap();
            let submitted = start.elapsed().as_secs_f64() * 1000.0;
            let gpu = timer
                .as_ref()
                .map(|value| value.read_ms(&device, &queue).unwrap());
            let frame = start.elapsed().as_secs_f64() * 1000.0;
            if i >= 3 {
                submit_ms.push(submitted);
                frame_ms.push(frame);
                if let Some(gpu) = gpu {
                    gpu_ms.push(gpu);
                }
            }
        }
        eprintln!(
            "fixed_scale count={count} snapshot_ms={:?} submit_ms={:?} frame_ms={:?} gpu_ms={:?} primitive_bytes={}",
            stats(snapshot_ms),
            stats(submit_ms),
            stats(frame_ms),
            if gpu_ms.is_empty() {
                None
            } else {
                Some(stats(gpu_ms))
            },
            count * 64
        );
    }
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_real_381_scene_and_dynamic_source() {
    let mut world = four_organism_world();
    let original_world = world.clone();
    let scene = Scene::from_world(&world).unwrap();
    assert_eq!(scene.primitives().len(), 381);
    let ids: std::collections::HashSet<_> = scene.primitives().iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 381);

    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();

    let rays: Vec<_> = (-12..=12)
        .flat_map(|x| {
            (0..=20).map(move |y| r(v(x as f64 * 0.25, y as f64 * 0.25, -8.0), v(0.0, 0.0, 1.0)))
        })
        .collect();
    let gpu = renderer.query(&device, &queue, &scene, &rays).unwrap();
    let mut compared_hits = 0;
    let accelerated = renderer
        .query_with_traversal(&device, &queue, &scene, &rays, GpuTraversal::Bvh)
        .unwrap();
    for (direct, accelerated) in gpu.iter().zip(accelerated) {
        assert_eq!(direct.map(|hit| hit.id), accelerated.map(|hit| hit.id));
        if let (Some(direct), Some(accelerated)) = (direct, accelerated) {
            assert_eq!(direct.distance, accelerated.distance);
            assert_eq!(direct.normal, accelerated.normal);
        }
    }
    let mut near_tie_ids = 0;
    for (index, (ray, actual)) in rays.iter().zip(gpu).enumerate() {
        match (scene.intersect(*ray), actual) {
            (None, None) => {}
            (Some(cpu), Some(gpu)) => {
                let tolerance = 5e-4_f64.max(cpu.distance * 3e-4);
                assert!(
                    (cpu.distance - f64::from(gpu.distance)).abs() <= tolerance,
                    "ray {index} distance"
                );
                if cpu.id != gpu.id {
                    let selected = scene
                        .primitives()
                        .iter()
                        .find(|p| p.id == gpu.id)
                        .expect("GPU ID exists in snapshot");
                    let mut isolated = Scene::default();
                    isolated.push(*selected).unwrap();
                    let candidate = isolated
                        .intersect(*ray)
                        .expect("GPU-selected primitive intersects CPU ray");
                    // f64 and f32 can order nearly coincident surfaces differently.
                    assert!(
                        (candidate.distance - cpu.distance).abs() <= tolerance,
                        "ray {index} identity is not a near tie"
                    );
                    near_tie_ids += 1;
                } else {
                    let alignment = cpu.normal.x() * f64::from(gpu.normal[0])
                        + cpu.normal.y() * f64::from(gpu.normal[1])
                        + cpu.normal.z() * f64::from(gpu.normal[2]);
                    assert!(
                        alignment >= 0.98,
                        "ray {index} normal alignment={alignment}"
                    );
                }
                compared_hits += 1;
            }
            other => panic!("ray {index} classification mismatch: {other:?}"),
        }
    }
    assert!(
        compared_hits > 20,
        "stress rays must cover visible geometry: {compared_hits} hits"
    );
    eprintln!(
        "real_381 gpu parity: {compared_hits} hits, {near_tie_ids} near-tie ID differences across {} rays",
        rays.len()
    );

    let camera =
        Camera::look_at(v(0.0, 2.0, -8.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.95).unwrap();
    for size in [[320, 180], [640, 360]] {
        renderer
            .draw(
                &device,
                &queue,
                &scene,
                camera,
                DrawOptions {
                    size,
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
            )
            .unwrap();
    }
    let limited = pollster::block_on(GpuRenderer::with_primitive_budget(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        256 * 64,
    ))
    .unwrap();
    assert!(matches!(
        limited.query(&device, &queue, &scene, &rays[..1]),
        Err(analytic_renderer::RenderError::PrimitiveCapacity {
            requested_bytes: 24_384,
            allowed_bytes: 16_384
        })
    ));
    assert_eq!(
        world, original_world,
        "GPU capacity errors cannot mutate WorldState"
    );

    let source = world.sources()[0].id();
    world
        .source_mut(source)
        .unwrap()
        .move_to(v(5.0, 2.0, 0.0))
        .unwrap();
    let moved = Scene::from_world(&world).unwrap();
    assert_eq!(moved.primitives().len(), 381);
    let source_ray = r(v(5.0, 2.0, -5.0), v(0.0, 0.0, 1.0));
    let expected = moved.intersect(source_ray).unwrap();
    let actual = renderer
        .query_with_traversal(&device, &queue, &moved, &[source_ray], GpuTraversal::Bvh)
        .unwrap()[0]
        .unwrap();
    assert_eq!(actual.id, expected.id);
    assert_eq!(actual.id, moved.primitives().last().unwrap().id);
    assert!((f64::from(actual.distance) - expected.distance).abs() < 5e-4);
}

#[test]
#[ignore = "requires a compatible native GPU; run serially and explicitly"]
fn gpu_real_381_resolution_benchmark() {
    fn stats(mut values: Vec<f64>) -> (f64, f64) {
        values.sort_by(f64::total_cmp);
        (
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1],
        )
    }
    let world = four_organism_world();
    let scene = Scene::from_world(&world).unwrap();
    assert_eq!(scene.primitives().len(), 381);
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    let camera =
        Camera::look_at(v(0.0, 3.0, -12.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap();
    let mut snapshots = Vec::new();
    for _ in 0..25 {
        let started = Instant::now();
        std::hint::black_box(Scene::from_world(&world).unwrap());
        snapshots.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    eprintln!(
        "real_381 adapter={:?} snapshot_ms={:?} primitive_bytes=24384",
        adapter.get_info(),
        stats(snapshots)
    );
    for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
        let mut submit_ms = Vec::new();
        let mut frame_ms = Vec::new();
        let mut gpu_ms = Vec::new();
        for index in 0..13 {
            let started = Instant::now();
            renderer
                .draw_with_traversal(
                    &device,
                    &queue,
                    &scene,
                    camera,
                    GpuTraversal::Direct,
                    DrawOptions {
                        size,
                        normal_debug: false,
                        surface: None,
                        timer: timer.as_ref(),
                    },
                )
                .unwrap();
            let submitted = started.elapsed().as_secs_f64() * 1000.0;
            let gpu = timer
                .as_ref()
                .map(|value| value.read_ms(&device, &queue).unwrap());
            let frame = started.elapsed().as_secs_f64() * 1000.0;
            if index >= 3 {
                submit_ms.push(submitted);
                frame_ms.push(frame);
                if let Some(gpu) = gpu {
                    gpu_ms.push(gpu);
                }
            }
        }
        eprintln!(
            "real_381 size={size:?} submit_ms={:?} full_frame_ms={:?} gpu_compute_ms={:?}",
            stats(submit_ms),
            stats(frame_ms),
            if gpu_ms.is_empty() {
                None
            } else {
                Some(stats(gpu_ms))
            }
        );
    }
}

#[test]
#[ignore = "requires a compatible native GPU; run serially and explicitly"]
fn gpu_bvh_comparison_benchmark() {
    fn stats(mut values: Vec<f64>) -> (f64, f64) {
        values.sort_by(f64::total_cmp);
        (
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1],
        )
    }
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let timer = GpuTimer::new(&device);
    eprintln!(
        "bvh_comparison adapter={:?} timestamp={}",
        adapter.get_info(),
        timer.is_some()
    );
    let measure = |label: &str,
                   scene: &Scene,
                   camera: Camera,
                   size: [u32; 2],
                   mode: GpuTraversal| {
        let mut submit_ms = Vec::new();
        let mut frame_ms = Vec::new();
        let mut gpu_ms = Vec::new();
        let mut first_upload = None;
        for sample in 0..13 {
            let started = Instant::now();
            renderer
                .draw_with_traversal(
                    &device,
                    &queue,
                    scene,
                    camera,
                    mode,
                    DrawOptions {
                        size,
                        normal_debug: false,
                        surface: None,
                        timer: timer.as_ref(),
                    },
                )
                .unwrap();
            let submitted = started.elapsed().as_secs_f64() * 1000.0;
            if sample == 0 {
                first_upload = renderer.last_upload_stats();
            }
            let gpu = timer
                .as_ref()
                .map(|value| value.read_ms(&device, &queue).unwrap());
            let frame = started.elapsed().as_secs_f64() * 1000.0;
            if sample >= 3 {
                submit_ms.push(submitted);
                frame_ms.push(frame);
                if let Some(gpu) = gpu {
                    gpu_ms.push(gpu);
                }
            }
        }
        eprintln!(
            "bvh_comparison scene={label} primitives={} size={size:?} mode={mode:?} submit_ms={:?} full_frame_ms={:?} gpu_ms={:?} first_upload={first_upload:?} acceleration={:?}",
            scene.primitives().len(),
            stats(submit_ms),
            stats(frame_ms),
            if gpu_ms.is_empty() {
                None
            } else {
                Some(stats(gpu_ms))
            },
            if mode == GpuTraversal::Bvh {
                renderer.acceleration_stats()
            } else {
                None
            }
        );
    };
    let fixed_camera =
        Camera::look_at(v(0.0, 0.0, -20.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.95).unwrap();
    let mut world = WorldState::new(DeterministicSeed(7));
    for count in [1, 13, 48, 128, 256, 381] {
        while world.entities().len() < count {
            let index = world.entities().len();
            world
                .spawn_sphere(
                    Sphere::new(0.18).unwrap(),
                    Transform::new(
                        v(
                            (index % 16) as f64 * 0.45 - 3.375,
                            (index / 16) as f64 * 0.45 - 5.4,
                            0.0,
                        ),
                        1.0,
                    )
                    .unwrap(),
                    0.0,
                )
                .unwrap();
        }
        let scene = Scene::from_world(&world).unwrap();
        let mut builds = Vec::new();
        for _ in 0..25 {
            let started = Instant::now();
            std::hint::black_box(Bvh::build(&scene).unwrap());
            builds.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        eprintln!("bvh_build fixed_count={count} ms={:?}", stats(builds));
        for mode in [GpuTraversal::Direct, GpuTraversal::Bvh] {
            measure("fixed-grid", &scene, fixed_camera, [1280, 720], mode);
        }
    }
    let real = Scene::from_world(&four_organism_world()).unwrap();
    let real_camera =
        Camera::look_at(v(0.0, 3.0, -12.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap();
    let mut builds = Vec::new();
    for _ in 0..25 {
        let started = Instant::now();
        std::hint::black_box(Bvh::build(&real).unwrap());
        builds.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    eprintln!("bvh_build real_381 ms={:?}", stats(builds));
    for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
        for mode in [GpuTraversal::Direct, GpuTraversal::Bvh] {
            measure("real-381", &real, real_camera, size, mode);
        }
    }
}

fn read_rgb(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let row_bytes = width * 4;
    let padded = row_bytes.div_ceil(256) * 256;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("analytic capture readback"),
        size: u64::from(padded) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("analytic capture copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = output.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = slice.get_mapped_range().unwrap();
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for row in mapped.chunks_exact(padded as usize) {
        for pixel in row[..row_bytes as usize].chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    rgb
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_body_snapshot_follows_authoritative_fixed_step() {
    use world_simulation::{
        EnvironmentEvent, EnvironmentEventKind, SimulationStep, SimulationTime, advance_life,
    };
    let mut world = WorldState::new(DeterministicSeed(23));
    let id = world.spawn_body(v(-1.0, 0.0, 0.0), 0.2).unwrap();
    let before = Scene::from_world(&world).unwrap();
    let mut time = SimulationTime::default();
    advance_life(
        &mut world,
        &mut time,
        SimulationStep::new(0.5).unwrap(),
        &[EnvironmentEvent {
            tick: 1,
            order: 0,
            kind: EnvironmentEventKind::SetBodyVelocity {
                id,
                velocity: v(2.0, 0.0, 0.0),
            },
        }],
    )
    .unwrap();
    let after = Scene::from_world(&world).unwrap();
    assert_eq!(
        after.primitives()[0].target(),
        Some(SemanticTarget::Body(id))
    );
    assert_eq!(world.body().unwrap().position(), Vec3::ZERO);
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let ray = r(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0));
    assert!(renderer.query(&device, &queue, &before, &[ray]).unwrap()[0].is_none());
    assert_eq!(
        renderer
            .query_with_traversal(&device, &queue, &after, &[ray], GpuTraversal::Bvh)
            .unwrap()[0]
            .unwrap()
            .id,
        after.intersect(ray).unwrap().id
    );
    let camera = Camera::look_at(v(0.0, 0.0, -4.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let render = |scene: &Scene, mode| {
        let image = renderer
            .draw_with_traversal(
                &device,
                &queue,
                scene,
                camera,
                mode,
                DrawOptions {
                    size: [256, 256],
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
            )
            .unwrap();
        read_rgb(&device, &queue, &image, 256, 256)
    };
    let old = render(&before, GpuTraversal::Direct);
    let direct = render(&after, GpuTraversal::Direct);
    let accelerated = render(&after, GpuTraversal::Bvh);
    assert_ne!(old, direct);
    assert_eq!(direct, accelerated);
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_ecology_snapshot_matches_authoritative_growth() {
    use world_simulation::{SimulationStep, SimulationTime, advance_life};
    let mut world = WorldState::new(DeterministicSeed(33));
    let parameters = GrowthParameters::new(0.14, 0.32, 0.5, 1.0).unwrap();
    world.spawn_organism(v(-0.6, 0.0, 0.0), parameters).unwrap();
    world.spawn_organism(v(0.6, 0.0, 0.0), parameters).unwrap();
    world
        .spawn_finite_source(v(0.0, 2.0, 0.0), 4.0, 10.0, 0.0, 0.2, 0.2)
        .unwrap();
    let before = Scene::from_world(&world).unwrap();
    let mut time = SimulationTime::default();
    for _ in 0..10 {
        advance_life(
            &mut world,
            &mut time,
            SimulationStep::new(0.1).unwrap(),
            &[],
        )
        .unwrap();
    }
    let after = Scene::from_world(&world).unwrap();
    assert!(after.primitives().len() > before.primitives().len());
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let camera =
        Camera::look_at(v(0.0, 1.5, -6.0), v(0.0, 0.8, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap();
    const SIZE: u32 = 512;
    let render = |scene: &Scene, mode| {
        let image = renderer
            .draw_with_traversal(
                &device,
                &queue,
                scene,
                camera,
                mode,
                DrawOptions {
                    size: [SIZE, SIZE],
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
            )
            .unwrap();
        read_rgb(&device, &queue, &image, SIZE, SIZE)
    };
    let old = render(&before, GpuTraversal::Direct);
    let direct = render(&after, GpuTraversal::Direct);
    let bvh = render(&after, GpuTraversal::Bvh);
    assert_ne!(old, direct);
    assert_eq!(direct, bvh);
    if let Ok(path) = std::env::var("GENESIS_ECOLOGY_CAPTURE") {
        let mut ppm = format!("P6\n{SIZE} {SIZE}\n255\n").into_bytes();
        ppm.extend_from_slice(&direct);
        std::fs::write(path, ppm).unwrap();
    }
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_bvh_offscreen_image_parity() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let world = four_organism_world();
    let stress = Scene::from_world(&world).unwrap();
    let mut branch_world = WorldState::new(DeterministicSeed(7));
    branch_world
        .spawn_organism(
            Vec3::ZERO,
            GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
        )
        .unwrap();
    branch_world
        .spawn_source(v(0.0, 2.0, 0.0), 100.0, 10.0)
        .unwrap();
    let mut time = world_simulation::SimulationTime::default();
    let step = world_simulation::SimulationStep::new(0.01).unwrap();
    while branch_world.organisms()[0].nodes().len() < 48 {
        world_simulation::advance_life(&mut branch_world, &mut time, step, &[]).unwrap();
    }
    let branching = Scene::from_world(&branch_world).unwrap();
    let scenes = [
        (
            "branching-organism",
            &branching,
            Camera::look_at(v(0.0, 3.0, -10.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap(),
        ),
        (
            "four-organism",
            &stress,
            Camera::look_at(v(0.0, 3.0, -12.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap(),
        ),
        (
            "overlapping-capsules",
            &stress,
            Camera::look_at(
                v(-1.5, 1.0, -2.5),
                v(-1.5, 1.0, 0.0),
                v(0.0, 1.0, 0.0),
                0.65,
            )
            .unwrap(),
        ),
        (
            "moved-camera",
            &stress,
            Camera::look_at(v(4.0, 3.0, -9.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap(),
        ),
    ];
    let (width, height) = (800, 450);
    for (name, scene, camera) in scenes {
        let draw = |traversal| {
            let image = renderer
                .draw_with_traversal(
                    &device,
                    &queue,
                    scene,
                    camera,
                    traversal,
                    DrawOptions {
                        size: [width, height],
                        normal_debug: false,
                        surface: None,
                        timer: None,
                    },
                )
                .unwrap();
            read_rgb(&device, &queue, &image, width, height)
        };
        let direct = draw(GpuTraversal::Direct);
        let accelerated = draw(GpuTraversal::Bvh);
        assert_eq!(direct, accelerated, "{name}: offscreen pixels differ");
        if let Some(directory) = std::env::var_os("GENESIS_CAPTURE_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            for (mode, pixels) in [("direct", &direct), ("bvh", &accelerated)] {
                let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
                ppm.extend_from_slice(pixels);
                std::fs::write(
                    std::path::Path::new(&directory).join(format!("{name}-{mode}.ppm")),
                    ppm,
                )
                .unwrap();
            }
        }
        eprintln!(
            "bvh_image_parity scene={name} pixels={} mismatches=0",
            width * height
        );
    }
    assert_eq!(
        renderer.acceleration_stats().unwrap().rebuilds,
        1,
        "camera moves must reuse the stress-scene BVH"
    );
}

#[test]
#[ignore = "requires a compatible native GPU; run explicitly"]
fn gpu_first_scale_reference_captures() {
    fn capture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &GpuRenderer,
        name: &str,
        scene: &Scene,
        camera: Camera,
    ) {
        let (width, height) = (800u32, 450u32);
        let image = renderer
            .draw(
                device,
                queue,
                scene,
                camera,
                DrawOptions {
                    size: [width, height],
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
            )
            .unwrap();
        let rgb = read_rgb(device, queue, &image, width, height);
        let geometry_pixels = rgb.chunks_exact(3).filter(|pixel| pixel[1] > 70).count();
        assert!(
            geometry_pixels > 100,
            "{name}: no visible analytic geometry"
        );
        if let Some(directory) = std::env::var_os("GENESIS_CAPTURE_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
            ppm.extend_from_slice(&rgb);
            std::fs::write(
                std::path::Path::new(&directory).join(format!("{name}.ppm")),
                ppm,
            )
            .unwrap();
        }
        eprintln!(
            "capture={name} primitives={} geometry_pixels={geometry_pixels}",
            scene.primitives().len()
        );
    }

    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("GPU adapter required");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();

    let mut capsule = Scene::default();
    capsule
        .push(
            Primitive::capsule(
                1,
                Capsule::new(v(0.0, 0.0, 0.0), v(0.0, 2.0, 0.0), 0.4).unwrap(),
                [0.3, 0.9, 0.4],
            )
            .unwrap(),
        )
        .unwrap();
    capture(
        &device,
        &queue,
        &renderer,
        "single-capsule",
        &capsule,
        Camera::look_at(v(0.0, 1.0, -5.0), v(0.0, 1.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap(),
    );

    let mut one = WorldState::new(DeterministicSeed(7));
    one.spawn_organism(
        Vec3::ZERO,
        GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
    )
    .unwrap();
    one.spawn_source(v(0.0, 2.0, 0.0), 100.0, 10.0).unwrap();
    let mut time = world_simulation::SimulationTime::default();
    let step = world_simulation::SimulationStep::new(0.01).unwrap();
    while one.organisms()[0].nodes().len() < 48 {
        world_simulation::advance_life(&mut one, &mut time, step, &[]).unwrap();
    }
    let branching = Scene::from_world(&one).unwrap();
    capture(
        &device,
        &queue,
        &renderer,
        "branching-organism",
        &branching,
        Camera::look_at(v(0.0, 3.0, -10.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap(),
    );

    let stress = Scene::from_world(&four_organism_world()).unwrap();
    assert_eq!(stress.primitives().len(), 381);
    capture(
        &device,
        &queue,
        &renderer,
        "four-organism-381",
        &stress,
        Camera::look_at(v(0.0, 3.0, -12.0), v(0.0, 2.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap(),
    );
    capture(
        &device,
        &queue,
        &renderer,
        "overlapping-connections",
        &stress,
        Camera::look_at(
            v(-1.5, 1.0, -2.5),
            v(-1.5, 1.0, 0.0),
            v(0.0, 1.0, 0.0),
            0.65,
        )
        .unwrap(),
    );

    let mut many = Scene::default();
    for index in 0..381 {
        many.push(
            Primitive::sphere(
                index,
                Sphere::new(0.11).unwrap(),
                Transform::new(
                    v(
                        (index % 21) as f64 * 0.32 - 3.2,
                        (index / 21) as f64 * 0.32 - 2.72,
                        0.0,
                    ),
                    1.0,
                )
                .unwrap(),
                [0.3, 0.8, 0.9],
            )
            .unwrap(),
        )
        .unwrap();
    }
    capture(
        &device,
        &queue,
        &renderer,
        "many-analytic-primitives",
        &many,
        Camera::look_at(v(0.0, 0.0, -12.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.65).unwrap(),
    );
}
