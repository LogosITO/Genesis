//! Bounded JSON-authored mathematical growth using world-owned parameter channels.

use serde::Deserialize;
use serde_json::{Value, json};
use spatial_math::{Transform, Vec3};
use std::{
    error::Error,
    io::Read,
    path::{Path, PathBuf},
};
use world_runtime::Runtime;
use world_simulation::{EnvironmentEvent, SimulationStep};
use world_state::ParameterRule;

const MAX_SCENE_BYTES: usize = 64 * 1024;
const MAX_GRAPH_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesScene {
    format_version: u32,
    base_scene: String,
    fields: Vec<FieldSpec>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldSpec {
    definition: String,
    position: [f64; 3],
    scale: f64,
    rules: Vec<ParameterRule>,
}

fn sibling(directory: &Path, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || Path::new(name).file_name().and_then(|part| part.to_str()) != Some(name)
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err("scene references must be local file names".into());
    }
    let path = directory.join(name).canonicalize()?;
    if !path.starts_with(directory) || !path.is_file() {
        return Err("scene reference leaves its directory".into());
    }
    Ok(path)
}

fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("authored file exceeds its byte limit".into());
    }
    Ok(bytes)
}

fn parse(
    bytes: &[u8],
    directory: &Path,
    step: SimulationStep,
    feedback: Option<bool>,
) -> Result<Runtime, Box<dyn Error>> {
    if bytes.len() > MAX_SCENE_BYTES {
        return Err("rules scene exceeds 64 KiB".into());
    }
    let scene: RulesScene = serde_json::from_slice(bytes)?;
    if scene.format_version != 1 || scene.fields.len() != 2 {
        return Err("rules scene v1 requires exactly two mathematical fields".into());
    }
    let base = bounded(&sibling(directory, &scene.base_scene)?, MAX_SCENE_BYTES)?;
    let mut world = super::living::parse_scene(&base, step)?.world().clone();
    for field in scene.fields {
        let source = bounded(&sibling(directory, &field.definition)?, MAX_GRAPH_BYTES)?;
        let position = Vec3::new(field.position[0], field.position[1], field.position[2])?;
        let transform = Transform::new(position, field.scale)?;
        let id = world.spawn_field(&source, transform)?;
        let mut rules = field.rules;
        if let Some(feedback) = feedback {
            for rule in &mut rules {
                rule.feedback = feedback;
            }
        }
        world.enable_field_parameters(id, rules)?;
    }
    world.validate()?;
    Ok(Runtime::new(world, step))
}

/// Loads a complete local rules scene; sibling references cannot escape its directory.
pub fn read_scene(path: &Path, step: SimulationStep) -> Result<Runtime, Box<dyn Error>> {
    let path = path.canonicalize()?;
    let directory = path
        .parent()
        .ok_or("scene has no directory")?
        .canonicalize()?;
    parse(&bounded(&path, MAX_SCENE_BYTES)?, &directory, step, None)
}

/// Checked-in developer example; the `feedback` variant is for controlled ablation.
pub fn initial(step: SimulationStep, feedback: bool) -> Result<Runtime, Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/living-world-4.json");
    let path = path.canonicalize()?;
    let directory = path
        .parent()
        .ok_or("scene has no directory")?
        .canonicalize()?;
    parse(
        &bounded(&path, MAX_SCENE_BYTES)?,
        &directory,
        step,
        Some(feedback),
    )
}

fn record(runtime: &Runtime) -> Result<Value, Box<dyn Error>> {
    let fields = runtime.world().field_instances().iter().map(|field| -> Result<Value, Box<dyn Error>> {
        let state = field.parameters().ok_or("missing parameter state")?;
        Ok(json!({
            "id": field.id().value(), "revision": field.revision(), "generation": field.generation(),
            "parameter_revision": state.revision(),
            "values": state.channels().iter().map(|channel| channel.current()).collect::<Vec<_>>(),
            "allocated": state.channels().iter().map(|channel| channel.total_allocated()).collect::<Vec<_>>(),
            "sites": (0..state.channels().len()).map(|index| runtime.world().field_parameter_site(field.id(), index)).collect::<Result<Vec<_>, _>>()?,
            "bounds": runtime.world().field_bounds(field.id())?.min,
        }))
    }).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "tick": runtime.time().ticks(), "digest_fnv1a64": super::living::digest(runtime)?,
        "nodes": runtime.world().organisms().iter().map(|tree| tree.nodes().len()).collect::<Vec<_>>(),
        "stocks": runtime.world().sources().iter().map(|source| source.reservoir().expect("finite").stored()).collect::<Vec<_>>(),
        "source_allocated": runtime.world().sources().iter().map(|source| source.reservoir().expect("finite").total_allocated()).collect::<Vec<_>>(),
        "fields": fields,
    }))
}

/// Reproducible A/B experiment and same-law no-feedback ablation.
pub fn experiment() -> Result<Value, Box<dyn Error>> {
    let step = SimulationStep::new(super::STEP.as_secs_f64())?;
    let initial_runtime = initial(step, true)?;
    let initial_save = initial_runtime.save_bytes()?;
    let mut control = initial_runtime.clone();
    let mut changed = initial_runtime.clone();
    let event = super::living::intervention(&initial_runtime);
    let mut checkpoints =
        vec![json!({"control":record(&control)?, "intervention":record(&changed)?, "equal":true})];
    for tick in 1..=240 {
        if tick == 61 {
            changed.schedule(tick, event.clone())?;
        }
        control.tick()?;
        changed.tick()?;
        if [60, 61, 120, 240].contains(&tick) {
            checkpoints.push(json!({"control":record(&control)?, "intervention":record(&changed)?, "equal":control.world() == changed.world()}));
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
    let history = super::living_morph::bundle(
        &initial_save,
        &[EnvironmentEvent {
            tick: 61,
            order: 0,
            kind: event.clone(),
        }],
        &changed,
    )?;
    super::living_morph::replay_bundle(&history)?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenes")
        .canonicalize()?;
    let authored: Value = serde_json::from_slice(include_bytes!("../scenes/living-world-4.json"))?;
    let mut matrix = Vec::new();
    for (name, pointer, value) in [
        ("shifted_box", "/fields/1/position/0", json!(2.3)),
        ("lower_lobe_uptake", "/fields/0/rules/0/uptake", json!(0.8)),
    ] {
        let mut variant = authored.clone();
        *variant.pointer_mut(pointer).ok_or("matrix pointer")? = value;
        let bytes = serde_json::to_vec(&variant)?;
        let initial = parse(&bytes, &directory, step, Some(true))?;
        let initial_save = initial.save_bytes()?;
        let event = super::living::intervention(&initial);
        let mut control = initial.clone();
        let mut changed = initial;
        let mut no_feedback = parse(&bytes, &directory, step, Some(false))?;
        for tick in 1..=240 {
            if tick == 61 {
                changed.schedule(tick, event.clone())?;
                no_feedback.schedule(tick, event.clone())?;
            }
            control.tick()?;
            changed.tick()?;
            no_feedback.tick()?;
        }
        let mut replay = Runtime::load_bytes(&initial_save)?;
        for tick in 1..=240 {
            if tick == 61 {
                replay.schedule(tick, event.clone())?;
            }
            replay.tick()?;
        }
        matrix.push(json!({
            "case":name,
            "no_intervention":record(&control)?,
            "intervention":record(&changed)?,
            "feedback_disabled_intervention":record(&no_feedback)?,
            "replay_equal":replay == changed,
            "intervention_differs":control.world() != changed.world(),
            "feedback_differs":no_feedback.world() != changed.world(),
        }));
    }
    Ok(json!({
        "kind":"living-world-rules-ab", "initial_save":serde_json::from_slice::<Value>(&initial_save)?,
        "event":{"tick":61,"kind":event}, "checkpoints":checkpoints,
        "history_bundle":serde_json::from_slice::<Value>(&history)?,
        "replay_equal":replay == changed, "ablation":record(&ablation)?,
        "ablation_differs":ablation.world() != changed.world(),
        "matrix":matrix,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_two_topologies_replay_and_reject_invalid_rules() {
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let world = initial(step, true).unwrap();
        assert_eq!(world.world().field_instances().len(), 2);
        let result = experiment().unwrap();
        assert_eq!(result["initial_save"]["format_version"], 10);
        assert_eq!(result["checkpoints"][1]["equal"], true);
        assert_eq!(result["replay_equal"], true);
        assert_eq!(result["history_bundle"]["schema_version"], 1);
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenes")
            .canonicalize()
            .unwrap();
        let mut source: Value =
            serde_json::from_slice(include_bytes!("../scenes/living-world-4.json")).unwrap();
        source["fields"][0]["definition"] = "../morph-lobes.json".into();
        assert!(
            parse(
                &serde_json::to_vec(&source).unwrap(),
                &directory,
                step,
                None
            )
            .is_err()
        );
        source["fields"][0]["definition"] = "morph-lobes.json".into();
        source["fields"][0]["rules"][0]["binding"]["node"] = "missing".into();
        assert!(
            parse(
                &serde_json::to_vec(&source).unwrap(),
                &directory,
                step,
                None
            )
            .is_err()
        );
        source["fields"][0]["rules"][0]["binding"]["node"] = "left_ball".into();
        for (path, value) in [
            ("/format_version", json!(2)),
            ("/fields/0/rules/0/uptake", json!(-1.0)),
            ("/fields/0/rules/0/maximum", json!(0.0)),
            ("/fields/1/definition", json!("missing.json")),
            ("/base_scene", json!("missing.json")),
        ] {
            let mut invalid = source.clone();
            *invalid.pointer_mut(path).unwrap() = value;
            assert!(
                parse(
                    &serde_json::to_vec(&invalid).unwrap(),
                    &directory,
                    step,
                    None
                )
                .is_err(),
                "{path}"
            );
        }
        let mut duplicate = source.clone();
        let rule = duplicate["fields"][0]["rules"][0].clone();
        duplicate["fields"][0]["rules"]
            .as_array_mut()
            .unwrap()
            .push(rule);
        assert!(
            parse(
                &serde_json::to_vec(&duplicate).unwrap(),
                &directory,
                step,
                None
            )
            .is_err()
        );
        let mut unknown = source.clone();
        unknown["fields"][0]["rules"][0]["growth_law"] = json!("arbitrary");
        assert!(
            parse(
                &serde_json::to_vec(&unknown).unwrap(),
                &directory,
                step,
                None
            )
            .is_err()
        );
        assert!(parse(b"{", &directory, step, None).is_err());
        assert!(parse(&vec![b' '; MAX_SCENE_BYTES + 1], &directory, step, None).is_err());
        assert!(parse(br#"{"uptake":1e999}"#, &directory, step, None).is_err());
        assert_eq!(initial(step, true).unwrap(), world);
    }

    #[test]
    fn parameterized_ecology_stays_valid_through_saturation_and_reload() {
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let mut runtime = initial(step, true).unwrap();
        for tick in 1..=2_000 {
            runtime.tick().unwrap();
            if tick % 500 == 0 {
                runtime.world().validate().unwrap();
                runtime = Runtime::load_bytes(&runtime.save_bytes().unwrap()).unwrap();
            }
        }
        assert_eq!(runtime.time().ticks(), 2_000);
        for field in runtime.world().field_instances() {
            for channel in field.parameters().unwrap().channels() {
                assert!(channel.current() <= channel.rule().maximum);
                assert!(channel.total_allocated() >= 0.0);
            }
        }
    }

    #[test]
    #[ignore = "requires native GPU; mixed identity/depth readback and two-topology compiler parity"]
    fn two_topologies_native_gpu_identity_and_images() {
        use analytic_renderer::{
            Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
            MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
        };
        use std::{thread, time::Duration};
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let mut control = initial(step, true).unwrap();
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
            if [60, 61, 120, 240].contains(&tick) {
                states.push((tick, control.clone(), changed.clone()));
            }
        }
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("native GPU required");
        eprintln!("living_world_4_adapter={:?}", adapter.get_info());
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
        let diagnose = |runtime: &Runtime, execution| {
            renderer
                .diagnose_mixed_camera(
                    &device,
                    &queue,
                    runtime.world(),
                    camera,
                    MixedDrawOptions {
                        draw: DrawOptions {
                            size: [64, 64],
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
                .unwrap()
        };
        let draw = |runtime: &Runtime| {
            let size = [640, 360];
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
                        execution: GpuFieldExecution::Combined,
                        spatial_guard: true,
                    },
                )
                .unwrap();
            crate::app_tests::read_rgb(&device, &queue, &texture, size)
        };
        for _ in 0..500 {
            diagnose(&control, GpuFieldExecution::Combined);
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Combined {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Combined
        );
        let requests = renderer.compiler_stats().unwrap().requests;
        let uploads = renderer.field_cache_stats().unwrap().node_uploads;
        let capture_dir = std::env::var_os("GENESIS_LW4_CAPTURE_DIR");
        if let Some(dir) = &capture_dir {
            std::fs::create_dir_all(dir).unwrap();
        }
        let mut compared = 0usize;
        for (tick, a, b) in &states {
            let mut images = Vec::new();
            for (side, runtime) in [("control", a), ("intervention", b)] {
                let reference = diagnose(runtime, GpuFieldExecution::Interpreter);
                let compiled = diagnose(runtime, GpuFieldExecution::Combined);
                assert_eq!(
                    renderer.compiler_stats().unwrap().active,
                    GpuFieldPath::Combined
                );
                assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
                for (index, (left, right)) in reference.iter().zip(&compiled).enumerate() {
                    assert_eq!(left.path, GpuFieldPath::Interpreter);
                    assert_eq!(right.path, GpuFieldPath::Combined);
                    assert_eq!(
                        (
                            left.hit,
                            left.field,
                            left.uncertain,
                            left.exhausted,
                            left.ambiguous,
                            left.bounds_culled,
                            left.identity,
                            left.definition,
                            left.material
                        ),
                        (
                            right.hit,
                            right.field,
                            right.uncertain,
                            right.exhausted,
                            right.ambiguous,
                            right.bounds_culled,
                            right.identity,
                            right.definition,
                            right.material
                        ),
                        "tick {tick} {side} pixel {index}: {left:?} versus {right:?}"
                    );
                    if let (Some(x), Some(y)) = (left.distance, right.distance) {
                        assert!(
                            (x - y).abs() <= 0.005,
                            "tick {tick} {side} pixel {index}: {x} versus {y}"
                        );
                    }
                    if let (Some(a), Some(b)) = (left.normal, right.normal) {
                        assert!(
                            a.into_iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.02),
                            "tick {tick} {side} pixel {index}: {left:?} versus {right:?}"
                        );
                    }
                    compared += 1;
                }
                let image = draw(runtime);
                if let Some(dir) = &capture_dir {
                    crate::app_tests::write_bmp(
                        &Path::new(dir).join(format!("living-world-4-t{tick}-{side}.bmp")),
                        &image,
                        [640, 360],
                    );
                }
                images.push(image);
            }
            if *tick <= 60 {
                assert_eq!(images[0], images[1]);
            } else {
                assert_ne!(images[0], images[1]);
            }
        }
        assert!(renderer.field_cache_stats().unwrap().node_uploads > uploads);
        eprintln!("living_world_4_compared_pixels={compared} compiler_requests={requests}");
    }

    #[test]
    #[ignore = "release-profile native GPU timing of a changing two-definition world"]
    fn two_topologies_native_gpu_resolution_measurement() {
        use analytic_renderer::{
            Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
            GpuTimer, MixedDrawOptions, field_graph_gpu::GraphTracePolicy,
        };
        use std::{
            thread,
            time::{Duration, Instant},
        };
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let mut baseline = initial(step, true).unwrap();
        for tick in 1..=120 {
            if tick == 61 {
                baseline
                    .schedule(tick, crate::living::intervention(&baseline))
                    .unwrap();
            }
            baseline.tick().unwrap();
        }
        let adapter = pollster::block_on(
            wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
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
        let draw = |world: &Runtime, size, execution, timer: Option<&GpuTimer>| {
            renderer
                .draw_mixed_with_execution(
                    &device,
                    &queue,
                    world.world(),
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
        for _ in 0..500 {
            draw(&baseline, [640, 360], GpuFieldExecution::Combined, None);
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Combined {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Combined
        );
        let requests = renderer.compiler_stats().unwrap().requests;
        eprintln!(
            "living_world_4_benchmark_adapter={:?} timestamp={} profile={}",
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
                let mut world = baseline.clone();
                let mut tick_ms = Vec::new();
                let mut gpu_ms = Vec::new();
                let mut frame_ms = Vec::new();
                let mut binding_ms = Vec::new();
                for sample in 0..9 {
                    let started = Instant::now();
                    world.tick().unwrap();
                    let simulation_ms = started.elapsed().as_secs_f64() * 1000.0;
                    let started = Instant::now();
                    draw(&world, size, execution, timer.as_ref());
                    let binding = renderer.last_draw_stats().unwrap().field_binding_ms;
                    if let Some(timer) = &timer {
                        gpu_ms.push(timer.read_ms(&device, &queue).unwrap());
                    } else {
                        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                    }
                    let full_ms = started.elapsed().as_secs_f64() * 1000.0;
                    if sample >= 2 {
                        tick_ms.push(simulation_ms);
                        frame_ms.push(full_ms);
                        binding_ms.push(binding);
                    }
                }
                let gpu = if gpu_ms.is_empty() {
                    None
                } else {
                    Some(crate::median_p95(&gpu_ms[2..]))
                };
                let cache = renderer.field_cache_stats().unwrap();
                assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
                eprintln!(
                    "{}",
                    json!({
                        "kind":"living-world-4-dynamic-offscreen", "size":size,
                        "execution":format!("{execution:?}"), "samples":7,
                        "gpu_ms_p50_p95":gpu,
                        "offscreen_frame_ms_p50_p95":crate::median_p95(&frame_ms),
                        "simulation_tick_ms_p50_p95":crate::median_p95(&tick_ms),
                        "field_binding_ms_p50_p95":crate::median_p95(&binding_ms),
                        "node_uploads":cache.node_uploads,
                        "node_bytes":cache.node_bytes,
                        "compiler_requests":requests,
                    })
                );
            }
        }
    }
}
