//! Native GPU evidence for the bounded mixed-depth diagnostic path.
use analytic_field::Sphere;
use analytic_renderer::{
    Camera, DrawOptions, GpuRenderer, MixedDrawOptions,
    field_graph_gpu::{GraphGpuRenderer, GraphTracePolicy},
    mixed::{MixedRenderOptions, MixedSource, render_mixed_rgba},
};
use spatial_math::{Transform, Vec3};
use std::sync::mpsc;
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

fn rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &wgpu::Texture,
    size: [u32; 2],
) -> Vec<u8> {
    let padded = (size[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mixed native test readback"),
        size: u64::from(padded) * u64::from(size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mixed native test copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size[1]),
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = slice.get_mapped_range().unwrap();
    let result = mapped
        .chunks_exact(padded as usize)
        .flat_map(|row| row[..(size[0] * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}

fn pixel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &wgpu::Texture,
    size: [u32; 2],
) -> [u8; 4] {
    let pixels = rgba(device, queue, image, size);
    let offset = ((size[1] / 2 * size[0] + size[0] / 2) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn write_bmp(path: &std::path::Path, pixels: &[u8], size: [u32; 2]) {
    let [width, height] = size;
    let stride = (width * 3).div_ceil(4) * 4;
    let data_bytes = stride * height;
    let mut bmp = Vec::with_capacity((54 + data_bytes) as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(54 + data_bytes).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(height as i32).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    for y in (0..height).rev() {
        let row_start = bmp.len();
        for pixel in
            pixels[(y * width * 4) as usize..((y + 1) * width * 4) as usize].chunks_exact(4)
        {
            bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        bmp.resize(row_start + stride as usize, 0);
    }
    std::fs::write(path, bmp).unwrap();
}

#[test]
#[ignore = "requires native GPU; actual mixed compute and image readback"]
fn native_mixed_depth_matches_reference_at_center() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let field_renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let size = [33, 33];
    let source = br#"{"format_version":1,"id":"native-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.6}]}"#;
    let mut world = WorldState::new(DeterministicSeed(34));
    world
        .spawn_sphere(Sphere::new(0.6).unwrap(), Transform::identity(), 0.0)
        .unwrap();
    let field = world
        .spawn_field(source, Transform::new(v(0.0, 0.0, 2.0), 1.0).unwrap())
        .unwrap();
    let native = |world: &WorldState| {
        let image = renderer
            .draw_mixed(
                &device,
                &queue,
                world,
                camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size,
                        normal_debug: false,
                        surface: None,
                        timer: None,
                    },
                    policy: GraphTracePolicy::default(),
                    show_analytic: true,
                    show_fields: true,
                    diagnostic: true,
                },
            )
            .unwrap();
        pixel(&device, &queue, &image, size)
    };
    let empty = WorldState::new(DeterministicSeed(35));
    let empty_pixel = native(&empty);
    assert!(empty_pixel[0] < 40 && empty_pixel[2] < 50);
    let behind = native(&world);
    let reference = render_mixed_rgba(
        &renderer,
        &field_renderer,
        &device,
        &queue,
        &world,
        camera,
        MixedRenderOptions {
            size,
            policy: GraphTracePolicy::default(),
        },
    )
    .unwrap();
    assert!(matches!(
        reference.sources[16 + 16 * 33],
        MixedSource::Analytic(_)
    ));
    world
        .set_field_transform(field, Transform::new(v(0.0, 0.0, -2.0), 1.0).unwrap())
        .unwrap();
    let front = native(&world);
    let reference = render_mixed_rgba(
        &renderer,
        &field_renderer,
        &device,
        &queue,
        &world,
        camera,
        MixedRenderOptions {
            size,
            policy: GraphTracePolicy::default(),
        },
    )
    .unwrap();
    assert_eq!(reference.sources[16 + 16 * 33], MixedSource::Field(field));
    assert_ne!(behind, front, "GPU depth compositor ignored front field");
    assert!(front[2] > front[0], "front field should be blue: {front:?}");
    world.set_field_enabled(field, false).unwrap();
    assert_eq!(native(&world), behind);
    let hidden = renderer
        .draw_mixed(
            &device,
            &queue,
            &world,
            camera,
            MixedDrawOptions {
                draw: DrawOptions {
                    size,
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
                policy: GraphTracePolicy::default(),
                show_analytic: false,
                show_fields: true,
                diagnostic: true,
            },
        )
        .unwrap();
    assert_eq!(pixel(&device, &queue, &hidden, size), empty_pixel);
    world.set_field_enabled(field, true).unwrap();
    world
        .spawn_sphere(
            Sphere::new(0.8).unwrap(),
            Transform::new(v(3.0, 0.0, 0.0), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    world
        .spawn_field(
            include_bytes!("../../../examples/field-graph/definitions/smooth-bridge.json"),
            Transform::new(v(-2.5, 0.0, 0.0), 0.6).unwrap(),
        )
        .unwrap();
    let size = [512, 288];
    let capture_camera =
        Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 1.0).unwrap();
    let image = renderer
        .draw_mixed(
            &device,
            &queue,
            &world,
            capture_camera,
            MixedDrawOptions {
                draw: DrawOptions {
                    size,
                    normal_debug: false,
                    surface: None,
                    timer: None,
                },
                policy: GraphTracePolicy::default(),
                show_analytic: true,
                show_fields: true,
                diagnostic: true,
            },
        )
        .unwrap();
    let pixels = rgba(&device, &queue, &image, size);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/mixed-captures/native-mixed.bmp");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_bmp(&path, &pixels, size);
    world
        .spawn_field(source, Transform::new(v(0.0, 0.0, -6.0), 1.0).unwrap())
        .unwrap();
    let diagnostic = native(&world);
    assert!(
        diagnostic[0] > diagnostic[1] * 3 && diagnostic[0] > diagnostic[2] * 3,
        "inside-start uncertainty must remain visible: {diagnostic:?}"
    );
}

#[test]
#[ignore = "requires native GPU; mixed analytic/implicit depth readback"]
fn mixed_depth_tracks_world_changes_and_retains_empty_paths() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let analytic =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let fields = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let mut world = WorldState::new(DeterministicSeed(30));
    let render = |world: &WorldState| {
        render_mixed_rgba(
            &analytic,
            &fields,
            &device,
            &queue,
            world,
            camera,
            MixedRenderOptions {
                size: [33, 33],
                policy: GraphTracePolicy::default(),
            },
        )
        .unwrap()
    };
    assert!(
        render_mixed_rgba(
            &analytic,
            &fields,
            &device,
            &queue,
            &world,
            camera,
            MixedRenderOptions {
                size: [33, 33],
                policy: GraphTracePolicy {
                    iterations: 0,
                    ..GraphTracePolicy::default()
                }
            }
        )
        .is_err()
    );
    let center = 16 + 16 * 33;
    assert_eq!(render(&world).sources[center], MixedSource::Miss);
    world
        .spawn_sphere(
            Sphere::new(0.6).unwrap(),
            Transform::new(Vec3::ZERO, 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    let source = br#"{"format_version":1,"id":"mixed-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.6}]}"#;
    let field = world
        .spawn_field(source, Transform::new(v(0.0, 0.0, 2.0), 1.0).unwrap())
        .unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    world
        .set_field_transform(field, Transform::new(v(0.0, 0.0, -2.0), 1.0).unwrap())
        .unwrap();
    assert_eq!(render(&world).sources[center], MixedSource::Field(field));
    world.set_field_enabled(field, false).unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    world.set_field_enabled(field, true).unwrap();
    let second = world
        .spawn_field(source, Transform::new(v(2.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    assert_eq!(world.field_definitions().len(), 1);
    assert_eq!(render(&world).sources[center], MixedSource::Field(field));
    world.remove_field(field).unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    assert_ne!(field, second);
    println!(
        "mixed_depth adapter={} backend={:?} pixels=1089",
        adapter.get_info().name,
        adapter.get_info().backend
    );
}
