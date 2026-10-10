//! One resource-coupled implicit organism beside the original analytical ecosystem.

use serde_json::{Value, json};
use spatial_math::{Transform, Vec3};
use std::error::Error;
use world_runtime::Runtime;
use world_simulation::SimulationStep;
use world_state::EntityId;

const TWIN: &[u8] = include_bytes!("../../field-graph/definitions/twin.json");

pub fn initial(step: SimulationStep) -> Result<Runtime, Box<dyn Error>> {
    let base = super::living::initial(step)?;
    let mut world = base.world().clone();
    let field = world.spawn_field(TWIN, Transform::new(Vec3::new(0.0, 0.65, -1.3)?, 0.34)?)?;
    world.enable_field_growth(field, 1.2, 0.28, 1.2)?;
    Ok(Runtime::from_snapshot(world, base.time(), step)?)
}

fn field_id(runtime: &Runtime) -> EntityId {
    runtime
        .world()
        .field_instances()
        .iter()
        .find(|field| field.growth().is_some())
        .expect("implicit scene has one growing field")
        .id()
}

fn summary(runtime: &Runtime) -> Result<Value, Box<dyn Error>> {
    let id = field_id(runtime);
    let field = runtime.world().field_instance(id).expect("current field");
    let growth = field.growth().expect("growing field");
    Ok(json!({
        "tick": runtime.time().ticks(),
        "digest_fnv1a64": super::living::digest(runtime)?,
        "analytical_nodes": runtime.world().organisms().iter().map(|tree| tree.nodes().len()).collect::<Vec<_>>(),
        "resource_stocks": runtime.world().sources().iter().map(|source| source.reservoir().expect("finite source").stored()).collect::<Vec<_>>(),
        "resource_allocated": runtime.world().sources().iter().map(|source| source.reservoir().expect("finite source").total_allocated()).collect::<Vec<_>>(),
        "implicit": {
            "id": id.value(),
            "definition_revision": field.revision(),
            "parameter_revision": field.generation(),
            "scale": field.transform().scale(),
            "total_allocated": growth.total_allocated(),
            "last_allocated": growth.last_allocated(),
        }
    }))
}

/// A self-contained initial v8 snapshot plus one typed event and matched replay.
pub fn experiment() -> Result<Value, Box<dyn Error>> {
    let initial = initial(SimulationStep::new(super::STEP.as_secs_f64())?)?;
    let initial_save = initial.save_bytes()?;
    let (mut control, mut changed) = (initial.clone(), initial.clone());
    let mut checkpoints = vec![json!({
        "control": summary(&control)?,
        "intervention": summary(&changed)?,
        "world_equal": true,
    })];
    let event = super::living::intervention(&changed);
    for tick in 1..=240 {
        if tick == 61 {
            changed.schedule(tick, event.clone())?;
            assert_eq!(control.world(), changed.world());
        }
        control.tick()?;
        changed.tick()?;
        if [60, 61, 90, 120, 180, 240].contains(&tick) {
            checkpoints.push(json!({
                "control": summary(&control)?,
                "intervention": summary(&changed)?,
                "world_equal": control.world() == changed.world(),
            }));
        }
        if tick == 120 {
            control = Runtime::load_bytes(&control.save_bytes()?)?;
            changed = Runtime::load_bytes(&changed.save_bytes()?)?;
        }
    }
    let (mut control_replay, mut changed_replay) = (
        Runtime::load_bytes(&initial_save)?,
        Runtime::load_bytes(&initial_save)?,
    );
    for tick in 1..=240 {
        if tick == 61 {
            changed_replay.schedule(tick, event.clone())?;
        }
        control_replay.tick()?;
        changed_replay.tick()?;
    }
    Ok(json!({
        "kind": "living-world-implicit-ab",
        "initial_save": serde_json::from_slice::<Value>(&initial_save)?,
        "event": { "tick": 61, "kind": event },
        "checkpoints": checkpoints,
        "replay_equal": control == control_replay && changed == changed_replay,
        "final_worlds_equal": control.world() == changed.world(),
    }))
}

/// CPU tick, analytic snapshot, and one save/load observation for the mixed ecology.
pub fn measure_cpu() -> Result<Value, Box<dyn Error>> {
    super::living::measure_cpu_with(initial(SimulationStep::new(super::STEP.as_secs_f64())?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_intervention_changes_implicit_shape_and_replays() {
        let report = experiment().unwrap();
        assert_eq!(report["initial_save"]["format_version"], 8);
        assert_eq!(report["checkpoints"][1]["world_equal"], true);
        assert_eq!(report["replay_equal"], true);
        assert_eq!(report["final_worlds_equal"], false);
        let last = &report["checkpoints"][6];
        assert_ne!(
            last["control"]["implicit"]["scale"],
            last["intervention"]["implicit"]["scale"]
        );
        assert_ne!(
            last["control"]["analytical_nodes"],
            last["intervention"]["analytical_nodes"]
        );
        assert_eq!(
            last["control"]["implicit"]["definition_revision"],
            last["intervention"]["implicit"]["definition_revision"]
        );
    }

    #[test]
    fn growth_saturates_without_changing_identity_or_resource_bounds() {
        let mut runtime = initial(SimulationStep::new(crate::STEP.as_secs_f64()).unwrap()).unwrap();
        let id = field_id(&runtime);
        for _ in 0..2_000 {
            runtime.tick().unwrap();
        }
        let field = runtime.world().field_instance(id).unwrap();
        assert_eq!(
            field.transform().scale(),
            field.growth().unwrap().max_scale()
        );
        assert_eq!(field.growth().unwrap().last_allocated(), 0.0);
        assert!(runtime.world().sources().iter().all(|source| {
            let reservoir = source.reservoir().unwrap();
            reservoir.stored() >= 0.0 && reservoir.stored() <= reservoir.capacity()
        }));
        assert_eq!(
            Runtime::load_bytes(&runtime.save_bytes().unwrap()).unwrap(),
            runtime
        );
    }

    #[test]
    #[ignore = "requires native GPU; compare real mixed images and compiler reuse"]
    fn living_implicit_gpu_images_follow_authoritative_growth() {
        use analytic_renderer::{
            Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
            MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
        };
        use std::{path::Path, thread, time::Duration};

        let initial = initial(SimulationStep::new(crate::STEP.as_secs_f64()).unwrap()).unwrap();
        let (mut control, mut changed) = (initial.clone(), initial);
        let mut checkpoints = vec![(0, control.clone(), changed.clone())];
        for tick in 1..=240 {
            if tick == 61 {
                changed
                    .schedule(tick, crate::living::intervention(&changed))
                    .unwrap();
            }
            control.tick().unwrap();
            changed.tick().unwrap();
            if [60, 120, 240].contains(&tick) {
                checkpoints.push((tick, control.clone(), changed.clone()));
            }
        }
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("native GPU required");
        eprintln!("living2_gpu adapter={:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let renderer =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let camera = Camera::look_at(
            Vec3::new(0.0, 2.7, -8.0).unwrap(),
            Vec3::new(0.0, 1.1, 0.0).unwrap(),
            Vec3::new(0.0, 1.0, 0.0).unwrap(),
            0.95,
        )
        .unwrap();
        let size = [640, 360];
        let draw = |runtime: &Runtime, execution| {
            let image = renderer
                .draw_mixed_with_execution(
                    &device,
                    &queue,
                    runtime.world(),
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
                        materials: None,
                        simulation_seconds: 0.0,
                        background: [0.05, 0.075, 0.115],
                        light_direction: [0.5, 0.8, -0.6],
                    },
                    GpuFieldRenderMode {
                        execution,
                        spatial_guard: true,
                    },
                )
                .unwrap();
            crate::app_tests::read_rgb(&device, &queue, &image, size)
        };
        draw(&checkpoints[0].1, GpuFieldExecution::Structural);
        for _ in 0..500 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Structural {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            draw(&checkpoints[0].1, GpuFieldExecution::Structural);
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Structural
        );
        let requests = renderer.compiler_stats().unwrap().requests;
        let node_uploads = renderer.field_cache_stats().unwrap().node_uploads;
        let capture_dir = std::env::var_os("GENESIS_LIVING_CAPTURE_DIR");
        if let Some(dir) = &capture_dir {
            std::fs::create_dir_all(dir).unwrap();
        }
        let mut pairs = Vec::new();
        for (tick, control, changed) in &checkpoints {
            let mut images = Vec::new();
            for (side, runtime) in [("control", control), ("intervention", changed)] {
                let interpreted = draw(runtime, GpuFieldExecution::Interpreter);
                let structural = draw(runtime, GpuFieldExecution::Structural);
                assert_eq!(
                    renderer.compiler_stats().unwrap().active,
                    GpuFieldPath::Structural
                );
                let max_diff = interpreted
                    .iter()
                    .zip(&structural)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(max_diff <= 1, "tick {tick} {side} max RGB drift {max_diff}");
                assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
                assert_eq!(
                    renderer.field_cache_stats().unwrap().node_uploads,
                    node_uploads
                );
                if let Some(dir) = &capture_dir {
                    crate::app_tests::write_bmp(
                        &Path::new(dir).join(format!("living2-t{tick}-{side}.bmp")),
                        &structural,
                        size,
                    );
                }
                images.push(structural);
            }
            pairs.push(images);
        }
        assert_eq!(pairs[0][0], pairs[0][1]);
        assert_eq!(pairs[1][0], pairs[1][1]);
        assert_ne!(pairs[2][0], pairs[2][1]);
        assert_ne!(pairs[3][0], pairs[3][1]);
    }

    #[test]
    #[ignore = "release-profile GPU timing of the changing mixed living world"]
    fn living_implicit_gpu_resolution_measurement() {
        use analytic_renderer::{
            Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
            GpuTimer, MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
        };
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let mut runtime = initial(SimulationStep::new(crate::STEP.as_secs_f64()).unwrap()).unwrap();
        for tick in 1..=240 {
            if tick == 61 {
                runtime
                    .schedule(tick, crate::living::intervention(&runtime))
                    .unwrap();
            }
            runtime.tick().unwrap();
        }
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("native GPU required");
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: features,
            ..Default::default()
        }))
        .unwrap();
        let renderer =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let timer = GpuTimer::new(&device);
        let camera = Camera::look_at(
            Vec3::new(0.0, 2.7, -8.0).unwrap(),
            Vec3::new(0.0, 1.1, 0.0).unwrap(),
            Vec3::new(0.0, 1.0, 0.0).unwrap(),
            0.95,
        )
        .unwrap();
        let draw = |size, execution, timer: Option<&GpuTimer>| {
            renderer
                .draw_mixed_with_execution(
                    &device,
                    &queue,
                    runtime.world(),
                    camera,
                    MixedDrawOptions {
                        draw: DrawOptions {
                            size,
                            normal_debug: false,
                            surface: None,
                            timer,
                        },
                        policy: GraphTracePolicy::default(),
                        show_analytic: true,
                        show_fields: true,
                        diagnostic: true,
                        materials: None,
                        simulation_seconds: 0.0,
                        background: [0.05, 0.075, 0.115],
                        light_direction: [0.5, 0.8, -0.6],
                    },
                    GpuFieldRenderMode {
                        execution,
                        spatial_guard: true,
                    },
                )
                .unwrap();
        };
        draw([640, 360], GpuFieldExecution::Structural, None);
        for _ in 0..500 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Structural {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            draw([640, 360], GpuFieldExecution::Structural, None);
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Structural
        );
        let requests = renderer.compiler_stats().unwrap().requests;
        eprintln!(
            "living2_benchmark_adapter={:?} timestamp={} profile={}",
            adapter.get_info(),
            timer.is_some(),
            if cfg!(debug_assertions) {
                "dev"
            } else {
                "release"
            }
        );
        for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
            for execution in [
                GpuFieldExecution::Interpreter,
                GpuFieldExecution::Structural,
            ] {
                let mut enqueue = Vec::new();
                let mut gpu = Vec::new();
                let mut scene = Vec::new();
                let mut field_binding = Vec::new();
                let mut submission = Vec::new();
                for index in 0..9 {
                    let started = Instant::now();
                    draw(size, execution, timer.as_ref());
                    let queued_ms = started.elapsed().as_secs_f64() * 1000.0;
                    let stages = renderer.last_draw_stats().unwrap();
                    let gpu_ms = if let Some(timer) = &timer {
                        Some(timer.read_ms(&device, &queue).unwrap())
                    } else {
                        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                        None
                    };
                    if index >= 2 {
                        enqueue.push(queued_ms);
                        scene.push(stages.scene_ms);
                        field_binding.push(stages.field_binding_ms);
                        submission.push(stages.encode_submit_ms);
                        if let Some(ms) = gpu_ms {
                            gpu.push(ms);
                        }
                    }
                }
                assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
                let compiler = renderer.compiler_stats().unwrap();
                let field_cache = renderer.field_cache_stats().unwrap();
                eprintln!(
                    "{}",
                    json!({
                        "kind": "living2-offscreen",
                        "size": size,
                        "execution": format!("{execution:?}"),
                        "active": format!("{:?}", compiler.active),
                        "requests": compiler.requests,
                        "cache_hits": compiler.cache_hits,
                        "node_uploads": field_cache.node_uploads,
                        "enqueue_ms_p50_p95": crate::median_p95(&enqueue),
                        "scene_ms_p50_p95": crate::median_p95(&scene),
                        "field_binding_ms_p50_p95": crate::median_p95(&field_binding),
                        "submission_ms_p50_p95": crate::median_p95(&submission),
                        "gpu_ms_p50_p95": if gpu.is_empty() { None } else { Some(crate::median_p95(&gpu)) },
                        "samples": enqueue.len(),
                    })
                );
            }
        }
    }
}
