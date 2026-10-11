//! GR-5 data-driven hierarchy preview, using materialized analytic GPU execution.

mod hierarchy;

use analytic_renderer::{Bvh, Camera, DrawOptions, GpuRenderer, GpuTimer, GpuTraversal};
use hierarchy::{Hierarchy, MAX_SOURCE_BYTES};
use spatial_math::Vec3;
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    time::Instant,
};

const SIZE: [u32; 2] = [640, 480];

fn vec3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2]).expect("bounded camera literal")
}

fn read_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let row = SIZE[0] * 4;
    let padded = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("GR-5 actual GPU readback"),
        size: u64::from(padded * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GR-5 copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(SIZE[1]),
            },
        },
        wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let data = slice.get_mapped_range()?;
    let mut rgba = Vec::with_capacity((row * SIZE[1]) as usize);
    for line in data.chunks_exact(padded as usize) {
        rgba.extend_from_slice(&line[..row as usize]);
    }
    drop(data);
    buffer.unmap();
    Ok(rgba)
}

fn save_ppm(path: &Path, rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut file = fs::File::create(path)?;
    write!(file, "P6\n{} {}\n255\n", SIZE[0], SIZE[1])?;
    for pixel in rgba.chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
    file.sync_all()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--detail")) {
        return Err(
            "usage: geometry-research <definition.json> <output-directory> [--detail]".into(),
        );
    }
    let detail = args.len() == 4;
    let source_path = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let mut source = Vec::new();
    fs::File::open(source_path)?
        .take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut source)?;
    let started = Instant::now();
    let hierarchy = Hierarchy::parse(&source)?;
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))?;
    let pipeline_started = Instant::now();
    let renderer = pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm))?;
    let pipeline_ms = pipeline_started.elapsed().as_secs_f64() * 1000.0;
    let timer = GpuTimer::new(&device);
    fs::create_dir_all(output)?;
    let mut log = fs::File::create(output.join("measurements.jsonl"))?;
    let mut base_pixels: Option<Vec<u8>> = None;

    for (label, scale) in [("base", 1.0), ("grown", 1.3)] {
        let build_started = Instant::now();
        let materialized = hierarchy.materialize(scale)?;
        let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
        let flat_records: Vec<_> = materialized
            .scene
            .primitives()
            .iter()
            .map(|primitive| {
                let center = primitive.center();
                let dimensions = primitive.dimensions();
                serde_json::json!({
                    "id":primitive.id, "kind":format!("{:?}",primitive.kind),
                    "center":[center.x(),center.y(),center.z()],
                    "dimensions":[dimensions.x(),dimensions.y(),dimensions.z()],
                    "radius":primitive.capsule_radius(), "color":primitive.color()
                })
            })
            .collect();
        let flat_json = serde_json::to_vec(&flat_records)?;
        fs::write(
            output.join(format!("{label}-flat-diagnostic.json")),
            &flat_json,
        )?;
        let bounds = materialized
            .bounds
            .ok_or("empty specimen cannot be captured")?;
        let center = [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
            (bounds.min[2] + bounds.max[2]) * 0.5,
        ];
        let height = bounds.max[1] - bounds.min[1];
        let width = bounds.max[0] - bounds.min[0];
        let distance = (height.max(width) * 1.7).max(8.0) * if detail { 0.45 } else { 1.0 };
        let camera = Camera::look_at(
            vec3([center[0], center[1], center[2] - distance]),
            vec3(center),
            vec3([0.0, 1.0, 0.0]),
            0.8,
        )?;
        let bvh = Bvh::build(&materialized.scene)?;
        let mut parity_rays = 0;
        for y in 0..9 {
            for x in 0..9 {
                let ray = camera.ray(
                    x * (SIZE[0] - 1) / 8,
                    y * (SIZE[1] - 1) / 8,
                    SIZE[0],
                    SIZE[1],
                )?;
                let direct = materialized.scene.intersect(ray);
                let indexed = bvh.intersect(ray);
                if direct.map(|v| v.id) != indexed.map(|v| v.id) {
                    return Err("CPU direct/BVH identity mismatch".into());
                }
                parity_rays += 1;
            }
        }
        let mut captures = Vec::new();
        let mut times = Vec::new();
        let mut full_ms = Vec::new();
        let mut bvh_info = None;
        let mut first_bvh_upload = None;
        for traversal in [GpuTraversal::Direct, GpuTraversal::Bvh] {
            let name = if traversal == GpuTraversal::Direct {
                "direct"
            } else {
                "bvh"
            };
            let mut rgba = Vec::new();
            for repetition in 0..8 {
                let frame_started = Instant::now();
                let texture = renderer.draw_with_traversal(
                    &device,
                    &queue,
                    &materialized.scene,
                    camera,
                    traversal,
                    DrawOptions {
                        size: SIZE,
                        normal_debug: false,
                        surface: None,
                        timer: timer.as_ref(),
                    },
                )?;
                if traversal == GpuTraversal::Bvh && repetition == 0 {
                    bvh_info = renderer.acceleration_stats().map(|stats| {
                        serde_json::json!({
                            "nodes":stats.node_count, "depth":stats.depth,
                            "allocated_bytes":stats.allocated_bytes, "rebuilds":stats.rebuilds
                        })
                    });
                    first_bvh_upload = renderer.last_upload_stats().map(|stats| {
                        serde_json::json!({
                            "primitive_bytes":stats.primitive_bytes, "bvh_bytes":stats.bvh_bytes,
                            "enqueue_ms":stats.enqueue_ms, "bvh_build_ms":stats.bvh_build_ms
                        })
                    });
                }
                let pixels = read_rgba(&device, &queue, &texture)?;
                let elapsed = frame_started.elapsed().as_secs_f64() * 1000.0;
                if repetition > 0 {
                    full_ms.push((name, elapsed));
                    if let Some(timer) = &timer {
                        times.push((name, timer.read_ms(&device, &queue)?));
                    }
                }
                rgba = pixels;
            }
            let capture = output.join(format!("{label}-{name}.ppm"));
            save_ppm(&capture, &rgba)?;
            captures.push((name, capture, rgba));
        }
        if captures[0].2 != captures[1].2 {
            return Err("GPU direct/BVH output differs".into());
        }
        if let Some(previous) = &base_pixels {
            if *previous == captures[0].2 {
                return Err("dynamic geometry did not change GPU pixels".into());
            }
        } else {
            base_pixels = Some(captures[0].2.clone());
        }
        let summary = serde_json::json!({
            "kind":"gr5-materialized-gpu", "source":source_path, "source_bytes":source.len(),
            "definitions":hierarchy.definition_count(), "authored_nodes":hierarchy.node_count(),
            "derived_features":materialized.scene.primitives().len(), "expanded_visits":materialized.visits,
            "dynamic_scale":scale, "state":label, "parse_ms":parse_ms, "materialize_ms":build_ms,
            "pipeline_create_ms":pipeline_ms, "shader_source_bytes":include_str!("../../../crates/renderer/shaders/analytic.wgsl").len(),
            "primitive_record_bytes":materialized.scene.primitives().len() * 64,
            "flat_diagnostic_json_bytes":flat_json.len(),
            "bvh":bvh_info, "first_bvh_upload":first_bvh_upload,
            "cpu_direct_bvh_parity_rays":parity_rays, "gpu_direct_bvh_pixel_equal":true,
            "gpu_compute_samples_ms":times, "blocking_frame_samples_ms":full_ms,
            "adapter":info.name, "backend":format!("{:?}",info.backend), "size":SIZE,
            "view":if detail { "detail" } else { "full" },
            "captures":[captures[0].1,captures[1].1], "bounds":{"min":bounds.min,"max":bounds.max},
            "execution":"materialized analytic primitives; BVH is derived acceleration, not lazy hierarchy"
        });
        writeln!(log, "{summary}")?;
        println!("{summary}");
    }
    Ok(())
}
