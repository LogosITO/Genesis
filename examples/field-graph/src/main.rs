//! Headless, isolated field graph preview. Output pixels come from the GPU interpreter.

use analytic_field::Sphere;
use analytic_renderer::{
    Camera, GpuRenderer, Ray,
    field_graph_gpu::{GraphGpuOutcome, GraphGpuRenderer, GraphTracePolicy, WorldFieldGpuOutcome},
    mixed::{MixedRenderOptions, MixedSource, render_mixed_rgba},
};
use spatial_math::{Transform, Vec3};
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use world_authoring::field_graph::{
    Graph, MAX_SOURCE_BYTES, SolidRayOptions, TraceOptions, compile_json, trace,
    trace_exterior_entry_report,
};
use world_state::{DeterministicSeed, WorldState};

fn read_graph_source(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(world_authoring::AuthoringError::InputTooLarge.into());
    }
    Ok(bytes)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite literal")
}

/// Candidate is compiled before replacement; errors leave the previous graph intact.
fn reload(active: &mut Graph, source: &[u8]) -> Result<(), Box<dyn Error>> {
    let candidate = compile_json(source)?;
    *active = candidate;
    Ok(())
}

fn save_ppm(path: &Path, rgba: &[u8], size: [u32; 2]) -> Result<(), Box<dyn Error>> {
    let mut file = fs::File::create(path)?;
    write!(file, "P6\n{} {}\n255\n", size[0], size[1])?;
    for pixel in rgba.chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
    file.sync_all()?;
    Ok(())
}

fn preview(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &GraphGpuRenderer,
    graph: &Graph,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8)?;
    let policy = GraphTracePolicy::default();
    let size = [640, 480];
    let frame = renderer.render_rgba(device, queue, graph, camera, size, policy)?;
    fs::create_dir_all(output)?;
    let revision = graph.revision().hex();
    let capture = output.join(format!("{}-{}.ppm", graph.id(), &revision[..12]));
    save_ppm(&capture, &frame.rgba, size)?;
    let ray = Ray::new(v(0.0, 0.0, -6.0), v(0.0, 0.0, 1.0), 0.001, 12.0)?;
    let cpu_options = TraceOptions {
        near: 0.001,
        far: 12.0,
        tolerance: f64::from(policy.tolerance),
        iterations: policy.iterations,
        safety: f64::from(policy.safety),
    };
    let cpu = trace(graph, ray.origin(), ray.direction(), cpu_options)?;
    let solid = trace_exterior_entry_report(
        graph,
        ray.origin(),
        ray.direction(),
        SolidRayOptions {
            ray: cpu_options,
            max_work: graph.visits() * policy.iterations as usize,
        },
    )?;
    let bounds = graph.bounds();
    let gpu = renderer.query(device, queue, graph, camera, &[ray], policy)?;
    let gpu_summary = match gpu[0] {
        GraphGpuOutcome::Hit { distance, .. } => format!("hit:{distance:.6}"),
        GraphGpuOutcome::Miss => "miss".into(),
        GraphGpuOutcome::Uncertain(issue) => format!("uncertain:{issue:?}"),
        GraphGpuOutcome::Exhausted => "exhausted".into(),
    };
    println!(
        "{}",
        serde_json::json!({"id":graph.id(),"sha256":revision,"nodes":graph.nodes().len(),"visits_per_sample":graph.visits(),"distance_semantics":format!("{:?}",graph.distance_semantics()),"ideal_lipschitz":graph.ideal_lipschitz_bound(),"bounds":{"min":[bounds.min.x(),bounds.min.y(),bounds.min.z()],"max":[bounds.max.x(),bounds.max.y(),bounds.max.z()]},"sample_origin":graph.sample(Vec3::ZERO)?,"cpu_center_ray":format!("{cpu:?}"),"solid_center_ray":format!("{:?}",solid.outcome),"solid_samples":solid.samples,"solid_charged_visits":solid.charged_visits,"gpu_center_ray":gpu_summary,"capture":capture,"full_frame_ms":frame.full_frame_ms,"rgba_bytes":frame.rgba.len()})
    );
    Ok(())
}

fn preview_world_fixture() -> Result<(), Box<dyn Error>> {
    let mut world = WorldState::new(DeterministicSeed(12));
    let first = world.spawn_field(
        include_bytes!("../definitions/twin.json"),
        Transform::new(v(-1.7, 0.0, 0.0), 1.0)?,
    )?;
    let second = world.spawn_field(
        include_bytes!("../definitions/plinth.json"),
        Transform::new(v(1.7, 0.0, 0.0), 1.0)?,
    )?;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device))?;
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8)?;
    let size = [128, 128];
    let rays = (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| camera.ray(x, y, size[0], size[1])))
        .collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    let mut outcomes = Vec::with_capacity(rays.len());
    for chunk in rays.chunks(4096) {
        outcomes.extend(renderer.query_world_fields(
            &device,
            &queue,
            &world,
            camera,
            chunk,
            GraphTracePolicy::default(),
        )?);
    }
    let full_query_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut rgba = Vec::with_capacity(outcomes.len() * 4);
    let mut counts = [0usize; 4];
    for outcome in outcomes {
        let color = match outcome {
            WorldFieldGpuOutcome::Hit { id, normal, .. } => {
                counts[0] += 1;
                let shade = (0.2 + 0.8 * normal[2].abs()).clamp(0.0, 1.0);
                let base = if id == first {
                    [40.0, 180.0, 225.0]
                } else {
                    [230.0, 155.0, 50.0]
                };
                base.map(|channel| (channel * shade) as u8)
            }
            WorldFieldGpuOutcome::Miss => {
                counts[1] += 1;
                [9, 14, 23]
            }
            WorldFieldGpuOutcome::Uncertain { .. } => {
                counts[2] += 1;
                [230, 25, 25]
            }
            WorldFieldGpuOutcome::Exhausted { .. } => {
                counts[3] += 1;
                [230, 190, 15]
            }
        };
        rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
    }
    let output = Path::new("target/field-graph-captures");
    fs::create_dir_all(output)?;
    let capture = output.join("world-instances.ppm");
    save_ppm(&capture, &rgba, size)?;
    println!(
        "{}",
        serde_json::json!({"capture":capture,"adapter":adapter.get_info().name,"instances":[first.value(),second.value()],"hits":counts[0],"misses":counts[1],"uncertain":counts[2],"exhausted":counts[3],"full_query_ms":full_query_ms,"size":size})
    );
    Ok(())
}

fn preview_mixed_fixture() -> Result<(), Box<dyn Error>> {
    let mut world = WorldState::new(DeterministicSeed(31));
    world.spawn_sphere(
        Sphere::new(0.65)?,
        Transform::new(v(-1.0, 0.0, -1.0), 1.0)?,
        0.0,
    )?;
    world.spawn_authored(
        include_bytes!("../../authoring/world-single.json"),
        Transform::new(v(0.0, -0.9, -0.5), 1.0)?,
        false,
    )?;
    let source = br#"{"format_version":1,"id":"mixed-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.75}]}"#;
    let rear = world.spawn_field(source, Transform::new(v(-1.0, 0.0, 1.0), 1.0)?)?;
    let front = world.spawn_field(source, Transform::new(v(1.0, 0.0, -1.0), 1.0)?)?;
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let analytic = pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm))?;
    let fields = pollster::block_on(GraphGpuRenderer::new(&device))?;
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8)?;
    let started = Instant::now();
    let frame = render_mixed_rgba(
        &analytic,
        &fields,
        &device,
        &queue,
        &world,
        camera,
        MixedRenderOptions {
            size: [256, 256],
            policy: GraphTracePolicy::default(),
        },
    )?;
    let output = Path::new("target/field-graph-captures");
    fs::create_dir_all(output)?;
    let capture = output.join("mixed-world.ppm");
    save_ppm(&capture, &frame.rgba, frame.size)?;
    let count = |target| {
        frame
            .sources
            .iter()
            .filter(|&&source| source == target)
            .count()
    };
    println!(
        "{}",
        serde_json::json!({"capture":capture,"adapter":adapter.get_info().name,"backend":format!("{:?}",adapter.get_info().backend),"analytic_pixels":frame.sources.iter().filter(|source| matches!(source,MixedSource::Analytic(_))).count(),"rear_field_pixels":count(MixedSource::Field(rear)),"front_field_pixels":count(MixedSource::Field(front)),"uncertain_pixels":count(MixedSource::Uncertain),"exhausted_pixels":count(MixedSource::Exhausted),"ambiguous_pixels":count(MixedSource::Ambiguous),"full_blocking_ms":started.elapsed().as_secs_f64()*1000.0})
    );
    Ok(())
}

fn inspect(path: &Path) -> Result<(), Box<dyn Error>> {
    let source = read_graph_source(path)?;
    let graph = compile_json(&source)?;
    let bounds = graph.bounds();
    println!(
        "{}",
        serde_json::json!({
            "id": graph.id(),
            "sha256": graph.revision().hex(),
            "nodes": graph.nodes().len(),
            "visits_per_sample": graph.visits(),
            "distance_semantics": format!("{:?}", graph.distance_semantics()),
            "bounds": {"min": [bounds.min.x(), bounds.min.y(), bounds.min.z()],
                       "max": [bounds.max.x(), bounds.max.y(), bounds.max.z()]}
        })
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let first = args
        .next()
        .ok_or("usage: field-graph-preview <definition.json> [--watch] | --inspect <definition.json> | --world-fixture | --mixed-world-fixture")?;
    if first == "--mixed-world-fixture" {
        if args.next().is_some() {
            return Err("--mixed-world-fixture accepts no other arguments".into());
        }
        return preview_mixed_fixture();
    }
    if first == "--inspect" {
        let path = args.next().ok_or("--inspect requires a definition path")?;
        if args.next().is_some() {
            return Err("--inspect accepts one path".into());
        }
        return inspect(Path::new(&path));
    }
    if first == "--world-fixture" {
        if args.next().is_some() {
            return Err("--world-fixture accepts no other arguments".into());
        }
        return preview_world_fixture();
    }
    let path = PathBuf::from(first);
    let watch = match args.next().as_deref() {
        None => false,
        Some("--watch") => true,
        _ => return Err("expected --watch".into()),
    };
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let mut source = read_graph_source(&path)?;
    let mut active = compile_json(&source)?;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let renderer = pollster::block_on(GraphGpuRenderer::new(&device))?;
    eprintln!(
        "GPU: {} ({:?})",
        adapter.get_info().name,
        adapter.get_info().backend
    );
    let output = Path::new("target/field-graph-captures");
    preview(&device, &queue, &renderer, &active, output)?;
    if watch {
        eprintln!(
            "watching {}; edit the JSON and press Ctrl-C to stop",
            path.display()
        );
        loop {
            thread::sleep(Duration::from_millis(500));
            let started = Instant::now();
            match read_graph_source(&path) {
                Ok(candidate) if candidate != source => {
                    source = candidate;
                    let previous = active.clone();
                    match reload(&mut active, &source)
                        .and_then(|()| preview(&device, &queue, &renderer, &active, output))
                    {
                        Ok(()) => eprintln!(
                            "reload complete in {:.3} ms",
                            started.elapsed().as_secs_f64() * 1000.0
                        ),
                        Err(error) => {
                            active = previous;
                            eprintln!("reload rejected; previous preview retained: {error}");
                        }
                    }
                }
                Err(error) => eprintln!("read failed; previous preview retained: {error}"),
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_reload_preserves_preview() {
        let original = include_bytes!("../definitions/twin.json");
        let mut graph = compile_json(original).unwrap();
        let revision = graph.revision();
        assert!(reload(&mut graph, b"{invalid").is_err());
        assert_eq!(graph.revision(), revision);
        reload(&mut graph, include_bytes!("../definitions/plinth.json")).unwrap();
        assert_ne!(graph.revision(), revision);
    }

    #[test]
    fn file_read_stops_at_source_budget() {
        let path =
            std::env::temp_dir().join(format!("field-graph-oversize-{}.json", std::process::id()));
        fs::write(&path, vec![b' '; MAX_SOURCE_BYTES + 1]).unwrap();
        assert!(read_graph_source(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
