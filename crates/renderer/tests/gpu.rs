//! Opt-in native GPU parity and offscreen readback. Run with `cargo test -p analytic-renderer --test gpu -- --ignored --nocapture`.

use analytic_field::{AxisAlignedBox, Ray as FieldRay, RayOptions, RayOutcome, Sphere, trace};
use analytic_renderer::{Camera, DrawOptions, GpuRenderer, GpuTimer, Primitive, Ray, Scene};
use spatial_math::{Transform, Vec3};
use std::{sync::mpsc, time::Instant};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}
fn r(o: Vec3, d: Vec3) -> Ray {
    Ray::new(o, d, 0.0, 100.0).unwrap()
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
