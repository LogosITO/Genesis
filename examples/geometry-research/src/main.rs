//! GR-5 data-driven hierarchy preview, using materialized analytic GPU execution.

mod gpu_hierarchy;
mod hierarchy;

use analytic_renderer::{Bvh, Camera, DrawOptions, GpuRenderer, GpuTimer, GpuTraversal};
use gpu_hierarchy::{HierarchyRenderer, View};
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
    if !(args.len() == 3
        || (args.len() == 4 && args[3] == "--detail")
        || (args.len() == 5 && args[3] == "--revision"))
    {
        return Err(
            "usage: geometry-research <definition.json> <output-directory> [--detail | --revision <revised.json>]".into(),
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
    let pack_started = Instant::now();
    let program = hierarchy.gpu_program()?;
    let pack_ms = pack_started.elapsed().as_secs_f64() * 1000.0;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
        ..Default::default()
    }))?;
    let pipeline_started = Instant::now();
    let renderer = pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm))?;
    let pipeline_ms = pipeline_started.elapsed().as_secs_f64() * 1000.0;
    let mut hierarchy_renderer = HierarchyRenderer::new(&device, &program)?;
    let timer = GpuTimer::new(&device);
    fs::create_dir_all(output)?;
    let mut log = fs::File::create(output.join("measurements.jsonl"))?;
    let mut base_pixels: Option<Vec<u8>> = None;
    let mut base_hierarchy_pixels: Option<Vec<u8>> = None;
    let mut base_view: Option<View> = None;

    for (label, scale) in [("base", 1.0), ("grown", 1.3), ("returned", 1.0)] {
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
        let mut hierarchy_times = Vec::new();
        let mut hierarchy_frame = None;
        let hierarchy_view = View {
            size: SIZE,
            center,
            distance,
            scale,
            prune: false,
        };
        if label == "base" {
            base_view = Some(hierarchy_view);
        }
        for repetition in 0..8 {
            let frame = hierarchy_renderer.render(&device, &queue, hierarchy_view)?;
            if repetition > 0 {
                hierarchy_times.push(frame.compute_ms);
            }
            hierarchy_frame = Some(frame);
        }
        let frame = hierarchy_frame.ok_or("missing hierarchy frame")?;
        let mut pruned_times = Vec::new();
        let mut pruned_frame = None;
        for repetition in 0..8 {
            let pruned = hierarchy_renderer.render(
                &device,
                &queue,
                View {
                    prune: true,
                    ..hierarchy_view
                },
            )?;
            if repetition > 0 {
                pruned_times.push(pruned.compute_ms);
            }
            pruned_frame = Some(pruned);
        }
        let pruned = pruned_frame.ok_or("missing pruned frame")?;
        let pruned_pixels = read_rgba(&device, &queue, &pruned.texture)?;
        let pruned_capture = output.join(format!("{label}-hierarchy-pruned.ppm"));
        save_ppm(&pruned_capture, &pruned_pixels)?;
        let pruned_identity_mismatches = frame
            .diagnostics
            .iter()
            .zip(&pruned.diagnostics)
            .filter(|(a, b)| a.meta[0] != b.meta[0] || (a.meta[0] != 0 && a.meta[1] != b.meta[1]))
            .count();
        let pruned_depth_mismatches = frame
            .diagnostics
            .iter()
            .zip(&pruned.diagnostics)
            .filter(|(a, b)| {
                a.meta[0] != 0 && b.meta[0] != 0 && (a.normal_t[3] - b.normal_t[3]).abs() > 0.002
            })
            .count();
        let pruned_exhausted = pruned.diagnostics.iter().filter(|d| d.meta[3] != 0).count();
        let hierarchy_pixels = read_rgba(&device, &queue, &frame.texture)?;
        let hierarchy_capture = output.join(format!("{label}-hierarchy.ppm"));
        save_ppm(&hierarchy_capture, &hierarchy_pixels)?;
        let exhausted = frame.diagnostics.iter().filter(|d| d.meta[3] != 0).count();
        let differing_pixels = captures[0]
            .2
            .chunks_exact(4)
            .zip(hierarchy_pixels.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        let max_channel_delta = captures[0]
            .2
            .iter()
            .zip(&hierarchy_pixels)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        let differing_coordinates: Vec<_> = captures[0]
            .2
            .chunks_exact(4)
            .zip(hierarchy_pixels.chunks_exact(4))
            .enumerate()
            .filter_map(|(index, (a, b))| (a != b).then_some(index))
            .collect();
        let mut different_hit = 0;
        let mut different_identity = 0;
        let mut different_depth = 0;
        let mut different_normal = 0;
        let mut first_differences = Vec::new();
        for chunk in differing_coordinates.chunks(65_536) {
            let rays: Vec<_> = chunk
                .iter()
                .map(|&index| {
                    camera.ray(
                        index as u32 % SIZE[0],
                        index as u32 / SIZE[0],
                        SIZE[0],
                        SIZE[1],
                    )
                })
                .collect::<Result<_, _>>()?;
            for (&index, hit) in
                chunk
                    .iter()
                    .zip(renderer.query(&device, &queue, &materialized.scene, &rays)?)
            {
                let diagnostic = frame.diagnostics[index];
                let hierarchy_hit = diagnostic.meta[0] != 0;
                if hit.is_some() != hierarchy_hit {
                    different_hit += 1;
                }
                if hit.map(|h| h.id) != hierarchy_hit.then_some(diagnostic.meta[1]) {
                    different_identity += 1;
                }
                if let Some(hit) = hit {
                    if (hit.distance - diagnostic.normal_t[3]).abs() > 0.002 {
                        different_depth += 1;
                    }
                    if (0..3)
                        .any(|axis| (hit.normal[axis] - diagnostic.normal_t[axis]).abs() > 0.02)
                    {
                        different_normal += 1;
                    }
                }
                if first_differences.len() < 8 {
                    first_differences.push(serde_json::json!({
                        "pixel":[index as u32 % SIZE[0], index as u32 / SIZE[0]],
                        "direct_id":hit.map(|h|h.id), "hierarchy_id":hierarchy_hit.then_some(diagnostic.meta[1]),
                        "direct_t":hit.map(|h|h.distance), "hierarchy_t":diagnostic.normal_t[3]
                    }));
                }
            }
        }
        let mut rays = Vec::new();
        let mut pixels = Vec::new();
        for y in 0..17 {
            for x in 0..17 {
                let px = x * (SIZE[0] - 1) / 16;
                let py = y * (SIZE[1] - 1) / 16;
                rays.push(camera.ray(px, py, SIZE[0], SIZE[1])?);
                pixels.push((py * SIZE[0] + px) as usize);
            }
        }
        let direct_hits = renderer.query(&device, &queue, &materialized.scene, &rays)?;
        let mut identity_mismatches = 0;
        let mut depth_mismatches = 0;
        let mut normal_mismatches = 0;
        for (pixel, direct) in pixels.into_iter().zip(direct_hits) {
            let diagnostic = frame.diagnostics[pixel];
            if direct.map(|h| h.id) != (diagnostic.meta[0] != 0).then_some(diagnostic.meta[1]) {
                identity_mismatches += 1;
                continue;
            }
            if let Some(hit) = direct {
                if (hit.distance - diagnostic.normal_t[3]).abs() > 0.002 {
                    depth_mismatches += 1;
                }
                if (0..3).any(|axis| (hit.normal[axis] - diagnostic.normal_t[axis]).abs() > 0.02) {
                    normal_mismatches += 1;
                }
            }
        }
        if exhausted != 0 {
            return Err(format!("GPU hierarchy exhausted on {exhausted} pixels").into());
        }
        match (label, &base_pixels, &base_hierarchy_pixels) {
            ("base", _, _) => {
                base_pixels = Some(captures[0].2.clone());
                base_hierarchy_pixels = Some(hierarchy_pixels.clone());
            }
            ("grown", Some(base), Some(base_hierarchy))
                if *base != captures[0].2 && *base_hierarchy != hierarchy_pixels => {}
            ("returned", Some(base), Some(base_hierarchy))
                if *base == captures[0].2 && *base_hierarchy == hierarchy_pixels => {}
            _ => return Err("dynamic GPU update or return-to-base failed".into()),
        }
        let hierarchy_summary = serde_json::json!({
            "packed_bytes":hierarchy_renderer.packed_bytes,
            "definition_bytes":program.definitions.len() * 48,
            "node_bytes":program.nodes.len() * 48,
            "edge_bytes":program.edges.len() * 48,
            "edges":program.edges.len(), "pack_ms":pack_ms,
            "pipeline_ms":hierarchy_renderer.pipeline_ms,
            "shader_source_bytes":include_str!("hierarchy.wgsl").len(),
            "compute_samples_ms":hierarchy_times,
            "blocking_last_ms":frame.blocking_ms,
            "upload_enqueue_last_ms":frame.upload_enqueue_ms,
            "exhausted_pixels":exhausted,
            "different_color_pixels":differing_pixels,
            "max_channel_delta":max_channel_delta,
            "different_hit_pixels":different_hit,
            "different_identity_pixels":different_identity,
            "different_depth_pixels":different_depth,
            "different_normal_pixels":different_normal,
            "first_color_differences":first_differences,
            "sampled_rays":rays.len(),
            "identity_mismatches":identity_mismatches,
            "depth_mismatches":depth_mismatches,
            "normal_mismatches":normal_mismatches,
            "max_node_visits":frame.diagnostics.iter().map(|d|d.work[0]).max(),
            "max_leaf_evaluations":frame.diagnostics.iter().map(|d|d.work[1]).max(),
            "max_stack_depth":frame.diagnostics.iter().map(|d|d.work[2]).max(),
            "capture":hierarchy_capture
        });
        let pruning_summary = serde_json::json!({
            "compute_samples_ms":pruned_times,
            "capture":pruned_capture,
            "identity_mismatches_vs_unpruned":pruned_identity_mismatches,
            "depth_mismatches_vs_unpruned":pruned_depth_mismatches,
            "exhausted_pixels":pruned_exhausted,
            "different_color_pixels_vs_unpruned":hierarchy_pixels.chunks_exact(4).zip(pruned_pixels.chunks_exact(4)).filter(|(a,b)|a!=b).count(),
            "total_culled_subtrees":pruned.diagnostics.iter().map(|d|u64::from(d.work[3])).sum::<u64>(),
            "max_leaf_evaluations":pruned.diagnostics.iter().map(|d|d.work[1]).max(),
            "max_node_visits":pruned.diagnostics.iter().map(|d|d.work[0]).max()
        });
        let summary = serde_json::json!({
            "kind":"gr6-compact-gpu-hierarchy", "source":source_path, "source_bytes":source.len(),
            "definitions":hierarchy.definition_count(), "authored_nodes":hierarchy.node_count(),
            "derived_features":materialized.scene.primitives().len(), "expanded_visits":materialized.visits,
            "dynamic_scale":scale, "state":label, "parse_ms":parse_ms, "materialize_ms":build_ms,
            "pipeline_create_ms":pipeline_ms, "shader_source_bytes":include_str!("../../../crates/renderer/shaders/analytic.wgsl").len(),
            "primitive_record_bytes":materialized.scene.primitives().len() * 64,
            "flat_diagnostic_json_bytes":flat_json.len(),
            "bvh":bvh_info, "first_bvh_upload":first_bvh_upload,
            "cpu_direct_bvh_parity_rays":parity_rays, "gpu_direct_bvh_pixel_equal":true,
            "gpu_compute_samples_ms":times, "blocking_frame_samples_ms":full_ms,
            "gpu_hierarchy":hierarchy_summary,
            "gpu_hierarchy_pruned":pruning_summary,
            "adapter":info.name, "backend":format!("{:?}",info.backend), "size":SIZE,
            "view":if detail { "detail" } else { "full" },
            "captures":[captures[0].1,captures[1].1], "bounds":{"min":bounds.min,"max":bounds.max},
            "execution":"matched materialized Direct/BVH and compact GPU hierarchy; hierarchy remains research-only"
        });
        writeln!(log, "{summary}")?;
        println!("{summary}");
    }
    if args.len() == 5 {
        let mut revised_bytes = Vec::new();
        fs::File::open(&args[4])?
            .take((MAX_SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut revised_bytes)?;
        let revised = Hierarchy::parse(&revised_bytes)?;
        let revised_program = revised.gpu_program()?;
        let revised_geometry = revised.materialize(1.0)?;
        let update_ms = hierarchy_renderer.update_program(&device, &revised_program)?;
        let view = base_view.ok_or("missing base view")?;
        let updated = hierarchy_renderer.render(&device, &queue, view)?;
        let fresh_renderer = HierarchyRenderer::new(&device, &revised_program)?;
        let fresh = fresh_renderer.render(&device, &queue, view)?;
        let updated_pixels = read_rgba(&device, &queue, &updated.texture)?;
        let fresh_pixels = read_rgba(&device, &queue, &fresh.texture)?;
        let changed_pixels = updated_pixels
            .chunks_exact(4)
            .zip(
                base_hierarchy_pixels
                    .as_ref()
                    .ok_or("missing base capture")?
                    .chunks_exact(4),
            )
            .filter(|(a, b)| a != b)
            .count();
        if changed_pixels == 0
            || updated_pixels != fresh_pixels
            || updated.diagnostics.iter().any(|d| d.meta[3] != 0)
        {
            return Err(
                "revised GPU hierarchy is stale, exhausted, or differs from fresh pipeline".into(),
            );
        }
        let capture = output.join("revision-hierarchy.ppm");
        save_ppm(&capture, &updated_pixels)?;
        let revision_summary = serde_json::json!({
            "kind":"gr6-dynamic-revision", "source":Path::new(&args[4]),
            "features":revised_geometry.scene.primitives().len(),
            "packed_bytes":hierarchy_renderer.packed_bytes,
            "update_enqueue_ms":update_ms,
            "pipeline_reused":true,
            "updated_equals_fresh":true,
            "changed_pixels_from_base":changed_pixels,
            "bounds":revised_geometry.bounds.map(|b| serde_json::json!({"min":b.min,"max":b.max})),
            "capture":capture
        });
        writeln!(log, "{revision_summary}")?;
        println!("{revision_summary}");
    }
    Ok(())
}
