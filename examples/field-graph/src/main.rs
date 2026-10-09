//! Headless, isolated field graph preview. Output pixels come from the GPU interpreter.

use analytic_renderer::{
    Camera, Ray,
    field_graph_gpu::{GraphGpuOutcome, GraphGpuRenderer, GraphTracePolicy},
};
use spatial_math::Vec3;
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use world_authoring::field_graph::{Graph, MAX_SOURCE_BYTES, TraceOptions, compile_json, trace};

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
    let cpu = trace(
        graph,
        ray.origin(),
        ray.direction(),
        TraceOptions {
            near: 0.001,
            far: 12.0,
            tolerance: f64::from(policy.tolerance),
            iterations: policy.iterations,
            safety: f64::from(policy.safety),
        },
    )?;
    let gpu = renderer.query(device, queue, graph, camera, &[ray], policy)?;
    let gpu_summary = match gpu[0] {
        GraphGpuOutcome::Hit { distance, .. } => format!("hit:{distance:.6}"),
        GraphGpuOutcome::Miss => "miss".into(),
        GraphGpuOutcome::Uncertain(issue) => format!("uncertain:{issue:?}"),
        GraphGpuOutcome::Exhausted => "exhausted".into(),
    };
    println!(
        "{}",
        serde_json::json!({"id":graph.id(),"sha256":revision,"nodes":graph.nodes().len(),"visits_per_sample":graph.visits(),"exact_sdf":graph.is_exact_sdf(),"ideal_lipschitz":graph.ideal_lipschitz_bound(),"sample_origin":graph.sample(Vec3::ZERO)?,"cpu_center_ray":format!("{cpu:?}"),"gpu_center_ray":gpu_summary,"capture":capture,"full_frame_ms":frame.full_frame_ms,"rgba_bytes":frame.rgba.len()})
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: field-graph-preview <definition.json> [--watch]")?,
    );
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
