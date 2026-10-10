//! Bounded two-lobe morphology with geometry-derived resource sampling sites.

use serde_json::{Value, json};
use spatial_math::{Transform, Vec3};
use std::error::Error;
use std::io::Read;
use world_runtime::Runtime;
use world_simulation::{EnvironmentEvent, SimulationStep};

const MAX_BUNDLE_BYTES: usize = 512 * 1024;
const MAX_BUNDLE_EVENTS: usize = 4_096;
const MAX_BUNDLE_TICKS: u64 = 10_000;

const MORPH: &[u8] = include_bytes!("../../field-graph/definitions/morph-lobes.json");
const TWIN: &[u8] = include_bytes!("../../field-graph/definitions/twin.json");

pub fn initial(step: SimulationStep, feedback: bool) -> Result<Runtime, Box<dyn Error>> {
    let base = super::living::initial(step)?;
    let mut world = base.world().clone();
    let id = world.spawn_field(MORPH, Transform::new(Vec3::new(-0.2, 0.75, -1.25)?, 0.48)?)?;
    world.enable_field_morphology(id, [1.4, 0.85], [0.32, 0.48], [1.15, 1.25], feedback)?;
    let second = world.spawn_field(TWIN, Transform::new(Vec3::new(1.7, 0.7, -1.3)?, 0.22)?)?;
    world.enable_field_growth(second, 0.7, 0.25, 0.65)?;
    Ok(Runtime::from_snapshot(world, base.time(), step)?)
}

fn record(runtime: &Runtime) -> Result<Value, Box<dyn Error>> {
    let field = runtime
        .world()
        .field_instances()
        .iter()
        .find(|field| field.morphology().is_some())
        .expect("morph scene");
    let morph = field.morphology().expect("morphology");
    let bounds = runtime.world().field_bounds(field.id())?;
    Ok(json!({
        "tick": runtime.time().ticks(),
        "digest_fnv1a64": super::living::digest(runtime)?,
        "analytic_nodes": runtime.world().organisms().iter().map(|tree| tree.nodes().len()).collect::<Vec<_>>(),
        "stocks": runtime.world().sources().iter().map(|source| source.reservoir().expect("finite").stored()).collect::<Vec<_>>(),
        "source_allocated": runtime.world().sources().iter().map(|source| source.reservoir().expect("finite").total_allocated()).collect::<Vec<_>>(),
        "morph": {
            "id": field.id().value(),
            "definition_revision": field.revision(),
            "generation": field.generation(),
            "radii": morph.radii(),
            "total_allocated": morph.total_allocated(),
            "last_allocated": morph.last_allocated(),
            "sites": [runtime.world().field_morph_site(field.id(), 0)?, runtime.world().field_morph_site(field.id(), 1)?],
            "bounds_min": bounds.min,
            "bounds_max": bounds.max,
        },
        "second_implicit": runtime.world().field_instances().iter().find(|item| item.growth().is_some()).map(|item| json!({
            "id": item.id().value(), "scale": item.transform().scale(), "allocated": item.growth().expect("growth").total_allocated()
        })),
    }))
}

/// Initial v9 snapshot plus one typed event, no-intervention control and feedback ablation.
pub fn experiment() -> Result<Value, Box<dyn Error>> {
    let step = SimulationStep::new(super::STEP.as_secs_f64())?;
    let initial_runtime = initial(step, true)?;
    let initial_save = initial_runtime.save_bytes()?;
    let mut control = initial_runtime.clone();
    let mut changed = initial_runtime.clone();
    let event = super::living::intervention(&changed);
    let mut checkpoints = vec![
        json!({"control": record(&control)?, "intervention": record(&changed)?, "equal": true}),
    ];
    for tick in 1..=240 {
        if tick == 61 {
            changed.schedule(tick, event.clone())?;
        }
        control.tick()?;
        changed.tick()?;
        if [60, 61, 90, 120, 180, 240].contains(&tick) {
            checkpoints.push(json!({"control": record(&control)?, "intervention": record(&changed)?, "equal": control.world() == changed.world()}));
        }
        if tick == 120 {
            control = Runtime::load_bytes(&control.save_bytes()?)?;
            changed = Runtime::load_bytes(&changed.save_bytes()?)?;
        }
    }
    let mut replay = Runtime::load_bytes(&initial_save)?;
    for tick in 1..=240 {
        if tick == 61 {
            replay.schedule(tick, event.clone())?;
        }
        replay.tick()?;
    }
    let mut ablation = initial(step, false)?;
    for tick in 1..=240 {
        if tick == 61 {
            ablation.schedule(tick, event.clone())?;
        }
        ablation.tick()?;
    }
    Ok(json!({
        "kind": "living-world-morphogenesis-ab",
        "initial_save": serde_json::from_slice::<Value>(&initial_save)?,
        "event": {"tick": 61, "kind": event},
        "checkpoints": checkpoints,
        "replay_equal": replay == changed,
        "no_intervention_equal": Runtime::load_bytes(&control.save_bytes()?)? == control,
        "ablation": record(&ablation)?,
        "ablation_differs": ablation.world() != changed.world(),
    }))
}

/// Serial CPU tick and analytical snapshot measurements for matched feedback variants.
pub fn measure_cpu() -> Result<Value, Box<dyn Error>> {
    let step = SimulationStep::new(super::STEP.as_secs_f64())?;
    Ok(json!({
        "feedback": super::living::measure_cpu_with(initial(step, true)?)?,
        "ablation": super::living::measure_cpu_with(initial(step, false)?)?,
    }))
}

/// Versioned initial snapshot plus applied events. A completed native save is a separate artifact.
pub fn bundle(
    initial: &[u8],
    events: &[EnvironmentEvent],
    final_state: &Runtime,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if events.len() > MAX_BUNDLE_EVENTS || final_state.time().ticks() > MAX_BUNDLE_TICKS {
        return Err("experiment bundle exceeds event or tick limit".into());
    }
    let initial_runtime = Runtime::load_bytes(initial)?;
    if initial_runtime.time().ticks() != 0 || !initial_runtime.pending_events().is_empty() {
        return Err("bundle requires an initial state without pending events".into());
    }
    if !final_state.pending_events().is_empty() {
        return Err("bundle requires a completed state without pending events".into());
    }
    let final_state = Runtime::from_snapshot(
        final_state.world().clone(),
        final_state.time(),
        final_state.step(),
    )?;
    let value = json!({
        "schema_version": 1,
        "kind": "living-world-morph-native-history",
        "initial_save": serde_json::from_slice::<Value>(initial)?,
        "events": events,
        "final_tick": final_state.time().ticks(),
        "final_save": serde_json::from_slice::<Value>(&final_state.save_bytes()?)?,
        "final_digest_fnv1a64": super::living::digest(&final_state)?,
    });
    let bytes = serde_json::to_vec_pretty(&value)?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err("experiment bundle exceeds byte limit".into());
    }
    replay_bundle(&bytes)?;
    Ok(bytes)
}

/// Replays a bounded native history from its exact-source initial snapshot.
pub fn replay_bundle(bytes: &[u8]) -> Result<Value, Box<dyn Error>> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err("experiment bundle exceeds byte limit".into());
    }
    let bundle: Value = serde_json::from_slice(bytes)?;
    if bundle["schema_version"] != 1 || bundle["kind"] != "living-world-morph-native-history" {
        return Err("unsupported experiment bundle".into());
    }
    let final_tick = bundle["final_tick"].as_u64().ok_or("missing final tick")?;
    if final_tick > MAX_BUNDLE_TICKS {
        return Err("experiment tick limit".into());
    }
    let initial = serde_json::to_vec(&bundle["initial_save"])?;
    let mut runtime = Runtime::load_bytes(&initial)?;
    if runtime.time().ticks() != 0
        || !runtime.pending_events().is_empty()
        || runtime
            .world()
            .field_instances()
            .iter()
            .all(|field| field.morphology().is_none() && field.parameters().is_none())
    {
        return Err("bundle initial state is not a fresh morph world".into());
    }
    let events: Vec<EnvironmentEvent> = serde_json::from_value(bundle["events"].clone())?;
    if events.len() > MAX_BUNDLE_EVENTS
        || events
            .iter()
            .any(|event| event.tick == 0 || event.tick > final_tick)
        || events
            .windows(2)
            .any(|pair| (pair[0].tick, pair[0].order) >= (pair[1].tick, pair[1].order))
    {
        return Err("invalid applied event order".into());
    }
    let mut events = events.into_iter().peekable();
    while runtime.time().ticks() < final_tick {
        let tick = runtime.time().ticks() + 1;
        while events.peek().is_some_and(|event| event.tick == tick) {
            let event = events.next().expect("peeked event");
            runtime.schedule(tick, event.kind)?;
        }
        runtime.tick()?;
    }
    let expected = Runtime::load_bytes(&serde_json::to_vec(&bundle["final_save"])?)?;
    if !expected.pending_events().is_empty() {
        return Err("bundle final state has pending events".into());
    }
    // Native ticks apply events directly; Runtime::schedule advances a separate queue-order counter.
    let runtime = Runtime::from_snapshot(runtime.world().clone(), runtime.time(), runtime.step())?;
    let expected =
        Runtime::from_snapshot(expected.world().clone(), expected.time(), expected.step())?;
    if runtime != expected {
        return Err("experiment replay state mismatch".into());
    }
    let actual = super::living::digest(&runtime)?;
    if bundle["final_digest_fnv1a64"] != actual {
        return Err("experiment replay digest mismatch".into());
    }
    Ok(
        json!({"kind":"living-world-morph-replay", "tick":final_tick, "digest_fnv1a64":actual, "equal":true}),
    )
}

pub fn read_bundle(path: &std::path::Path) -> Result<Value, Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_BUNDLE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    replay_bundle(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_feedback_changes_later_world_and_replays() {
        let result = experiment().unwrap();
        assert_eq!(result["initial_save"]["format_version"], 9);
        assert_eq!(result["checkpoints"][1]["equal"], true);
        assert_eq!(result["replay_equal"], true);
        assert_eq!(result["ablation_differs"], true);
        assert_ne!(
            result["checkpoints"][6]["intervention"]["morph"]["radii"],
            result["ablation"]["morph"]["radii"]
        );
    }

    #[test]
    fn native_bundle_replays_and_rejects_wrong_event_order() {
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let initial = initial(step, true).unwrap();
        let mut runtime = initial.clone();
        let event = EnvironmentEvent {
            tick: 61,
            order: 0,
            kind: crate::living::intervention(&runtime),
        };
        for tick in 1..=120 {
            if tick == 61 {
                runtime.schedule(tick, event.kind.clone()).unwrap();
            }
            runtime.tick().unwrap();
        }
        let bytes = bundle(&initial.save_bytes().unwrap(), &[event], &runtime).unwrap();
        assert_eq!(replay_bundle(&bytes).unwrap()["equal"], true);
        let mut bad: Value = serde_json::from_slice(&bytes).unwrap();
        bad["events"][0]["tick"] = 0.into();
        assert!(replay_bundle(&serde_json::to_vec(&bad).unwrap()).is_err());
        bad["events"][0]["tick"] = 61.into();
        let duplicate = bad["events"][0].clone();
        bad["events"].as_array_mut().unwrap().push(duplicate);
        assert!(replay_bundle(&serde_json::to_vec(&bad).unwrap()).is_err());
        bad["events"].as_array_mut().unwrap().pop();
        bad["final_digest_fnv1a64"] = "wrong".into();
        assert!(replay_bundle(&serde_json::to_vec(&bad).unwrap()).is_err());
    }

    #[test]
    fn native_bundle_validates_prune_against_world_at_event_tick() {
        use world_simulation::EnvironmentEventKind;
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let initial = initial(step, true).unwrap();
        let mut runtime = initial.clone();
        for _ in 0..60 {
            runtime.tick().unwrap();
        }
        let organism = runtime.world().organisms()[0].id();
        assert!(initial.world().organisms()[0].node(1).is_none());
        assert!(runtime.world().organisms()[0].node(1).is_some());
        let kind = EnvironmentEventKind::PruneBranch { organism, child: 1 };
        runtime.schedule(61, kind.clone()).unwrap();
        runtime.tick().unwrap();
        let event = EnvironmentEvent {
            tick: 61,
            order: 0,
            kind,
        };
        let bytes = bundle(&initial.save_bytes().unwrap(), &[event], &runtime).unwrap();
        assert_eq!(replay_bundle(&bytes).unwrap()["equal"], true);
    }

    #[test]
    #[ignore = "requires native GPU; real interpreter/Combined mixed readback and cache reuse"]
    fn morph_gpu_images_and_compiler_reuse() {
        use analytic_renderer::{
            Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
            MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
        };
        use std::{path::Path, thread, time::Duration};
        let mut control = initial(
            SimulationStep::new(crate::STEP.as_secs_f64()).unwrap(),
            true,
        )
        .unwrap();
        let mut changed = control.clone();
        let mut states = vec![(0, control.clone(), changed.clone())];
        for tick in 1..=240 {
            if tick == 61 {
                changed
                    .schedule(tick, crate::living::intervention(&changed))
                    .unwrap();
            }
            control.tick().unwrap();
            changed.tick().unwrap();
            if [60, 120, 240].contains(&tick) {
                states.push((tick, control.clone(), changed.clone()));
            }
        }
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("native GPU required");
        eprintln!("morph_gpu_adapter={:?}", adapter.get_info());
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
            let texture = renderer
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
            crate::app_tests::read_rgb(&device, &queue, &texture, size)
        };
        draw(&states[0].1, GpuFieldExecution::Combined);
        for _ in 0..500 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Combined {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            draw(&states[0].1, GpuFieldExecution::Combined);
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Combined
        );
        let requests = renderer.compiler_stats().unwrap().requests;
        let first_uploads = renderer.field_cache_stats().unwrap().node_uploads;
        let capture_dir = std::env::var_os("GENESIS_MORPH_CAPTURE_DIR");
        if let Some(dir) = &capture_dir {
            std::fs::create_dir_all(dir).unwrap();
        }
        for (tick, a, b) in &states {
            let mut pairs = Vec::new();
            for (side, runtime) in [("control", a), ("intervention", b)] {
                let interpreted = draw(runtime, GpuFieldExecution::Interpreter);
                let combined = draw(runtime, GpuFieldExecution::Combined);
                let max_diff = interpreted
                    .iter()
                    .zip(&combined)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(max_diff <= 1, "tick {tick} {side}: RGB drift {max_diff}");
                assert_eq!(
                    renderer.compiler_stats().unwrap().active,
                    GpuFieldPath::Combined
                );
                assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
                if let Some(dir) = &capture_dir {
                    crate::app_tests::write_bmp(
                        &Path::new(dir).join(format!("morph-t{tick}-{side}.bmp")),
                        &combined,
                        size,
                    );
                }
                pairs.push(combined);
            }
            if *tick <= 60 {
                assert_eq!(pairs[0], pairs[1]);
            } else {
                assert_ne!(pairs[0], pairs[1]);
            }
        }
        assert!(renderer.field_cache_stats().unwrap().node_uploads > first_uploads);
    }

    #[test]
    #[ignore = "release-profile native GPU timestamps for the evolving mixed morph world"]
    fn morph_gpu_resolution_measurement() {
        use analytic_renderer::{
            Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
            GpuTimer, MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
        };
        use std::{
            thread,
            time::{Duration, Instant},
        };
        let mut runtime = initial(
            SimulationStep::new(crate::STEP.as_secs_f64()).unwrap(),
            true,
        )
        .unwrap();
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
        draw([640, 360], GpuFieldExecution::Combined, None);
        for _ in 0..500 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Combined {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            draw([640, 360], GpuFieldExecution::Combined, None);
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Combined
        );
        let requests = renderer.compiler_stats().unwrap().requests;
        eprintln!(
            "morph_benchmark_adapter={:?} timestamp={} profile={}",
            adapter.get_info(),
            timer.is_some(),
            if cfg!(debug_assertions) {
                "dev"
            } else {
                "release"
            }
        );
        for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
            for execution in [GpuFieldExecution::Interpreter, GpuFieldExecution::Combined] {
                let mut enqueue = Vec::new();
                let mut gpu = Vec::new();
                let mut binding = Vec::new();
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
                        binding.push(stages.field_binding_ms);
                        submission.push(stages.encode_submit_ms);
                        if let Some(ms) = gpu_ms {
                            gpu.push(ms);
                        }
                    }
                }
                let compiler = renderer.compiler_stats().unwrap();
                let field_cache = renderer.field_cache_stats().unwrap();
                assert_eq!(compiler.requests, requests);
                eprintln!(
                    "{}",
                    json!({
                        "kind":"morph-offscreen", "size":size, "execution":format!("{execution:?}"),
                        "active":format!("{:?}",compiler.active), "requests":compiler.requests,
                        "cache_hits":compiler.cache_hits, "node_uploads":field_cache.node_uploads,
                        "enqueue_ms_p50_p95":crate::median_p95(&enqueue),
                        "field_binding_ms_p50_p95":crate::median_p95(&binding),
                        "submission_ms_p50_p95":crate::median_p95(&submission),
                        "gpu_ms_p50_p95": if gpu.is_empty() { None } else { Some(crate::median_p95(&gpu)) },
                        "samples":enqueue.len(),
                    })
                );
            }
        }
    }
}
