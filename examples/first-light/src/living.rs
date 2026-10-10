//! A bounded two-strategy ecosystem using the existing authoritative runtime.

use analytic_renderer::Scene;
use serde::Deserialize;
use serde_json::{Value, json};
use spatial_math::Vec3;
use std::{error::Error, io::Read, path::Path, time::Instant};
use world_runtime::Runtime;
use world_simulation::{EnvironmentEventKind, SimulationStep};
use world_state::{DeterministicSeed, GrowthParameters, WorldState};

fn point(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("fixed finite coordinate")
}

const MAX_SCENE_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LivingScene {
    format_version: u32,
    seed: u64,
    organisms: Vec<OrganismSpec>,
    sources: Vec<SourceSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganismSpec {
    root: [f64; 3],
    node_radius: f64,
    segment_length: f64,
    threshold: f64,
    uptake: f64,
    max_children: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSpec {
    position: [f64; 3],
    radius: f64,
    strength: f64,
    stored: f64,
    capacity: f64,
    replenish_per_tick: f64,
}

pub fn parse_scene(bytes: &[u8], step: SimulationStep) -> Result<Runtime, Box<dyn Error>> {
    if bytes.len() > MAX_SCENE_BYTES {
        return Err("living scene exceeds 64 KiB".into());
    }
    let scene: LivingScene = serde_json::from_slice(bytes)?;
    if scene.format_version != 1 || scene.organisms.len() != 2 || scene.sources.len() != 2 {
        return Err("living scene v1 requires exactly two organisms and two finite sources".into());
    }
    let mut world = WorldState::new(DeterministicSeed(scene.seed));
    for organism in scene.organisms {
        world.spawn_organism(
            Vec3::new(organism.root[0], organism.root[1], organism.root[2])?,
            GrowthParameters::new(
                organism.node_radius,
                organism.segment_length,
                organism.threshold,
                organism.uptake,
            )?
            .with_max_children(organism.max_children)?,
        )?;
    }
    if world.organisms()[0].parameters() == world.organisms()[1].parameters() {
        return Err("living scene requires two distinct developmental strategies".into());
    }
    for source in scene.sources {
        world.spawn_finite_source(
            Vec3::new(source.position[0], source.position[1], source.position[2])?,
            source.radius,
            source.strength,
            source.stored,
            source.capacity,
            source.replenish_per_tick,
        )?;
    }
    world.validate()?;
    Ok(Runtime::new(world, step))
}

pub fn read_scene(path: &Path, step: SimulationStep) -> Result<Runtime, Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_SCENE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    parse_scene(&bytes, step)
}

/// Same initial state for the native view and the A/B experiment.
pub fn initial(step: SimulationStep) -> Result<Runtime, Box<dyn Error>> {
    parse_scene(include_bytes!("../scenes/living-world.json"), step)
}

pub fn intervention(runtime: &Runtime) -> EnvironmentEventKind {
    EnvironmentEventKind::MoveSource {
        id: runtime.world().sources()[0].id(),
        position: point(4.0, 1.5, 0.0),
    }
}

pub(crate) fn digest(runtime: &Runtime) -> Result<String, Box<dyn Error>> {
    let hash = runtime
        .save_bytes()?
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    Ok(format!("{hash:016x}"))
}

fn summary(runtime: &Runtime) -> Result<Value, Box<dyn Error>> {
    Ok(json!({
        "tick": runtime.time().ticks(),
        "digest_fnv1a64": digest(runtime)?,
        "organisms": runtime.world().organisms().iter().map(|organism| json!({
            "id": organism.id().value(),
            "nodes": organism.nodes().len(),
            "branches": organism.branch_count(),
            "budget": organism.budget(),
            "next_node_id": organism.next_node_id(),
        })).collect::<Vec<_>>(),
        "sources": runtime.world().sources().iter().map(|source| {
            let stock = source.reservoir().expect("finite source");
            json!({
                "id": source.id().value(),
                "position": [source.position().x(), source.position().y(), source.position().z()],
                "stored": stock.stored(),
                "allocated": stock.total_allocated(),
                "replenished": stock.total_replenished(),
            })
        }).collect::<Vec<_>>(),
    }))
}

/// Compares two worlds under identical laws, with one source move in world B.
pub fn experiment() -> Result<Value, Box<dyn Error>> {
    let initial = initial(SimulationStep::new(super::STEP.as_secs_f64())?)?;
    let (mut control, mut changed) = (initial.clone(), initial.clone());
    let mut checkpoints =
        vec![json!({"control": summary(&control)?, "intervention": summary(&changed)?})];
    let mut event = None;
    for tick in 1..=240 {
        if tick == 61 {
            let kind = intervention(&changed);
            changed.schedule(tick, kind.clone())?;
            assert_eq!(control.world(), changed.world());
            event = Some(json!({"tick": tick, "kind": kind, "pre_tick_world_equal": true}));
        }
        control
            .tick()
            .map_err(|error| format!("control tick {tick}: {error}"))?;
        changed
            .tick()
            .map_err(|error| format!("intervention tick {tick}: {error}"))?;
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
    let (mut control_replay, mut changed_replay) = (initial.clone(), initial);
    changed_replay.schedule(61, intervention(&changed_replay))?;
    for _ in 0..240 {
        control_replay.tick()?;
        changed_replay.tick()?;
    }
    let replay_equal = control == control_replay && changed == changed_replay;
    Ok(json!({
        "kind": "living-world-ab",
        "event": event,
        "checkpoints": checkpoints,
        "replay_equal": replay_equal,
        "same_initial_state": checkpoints[0]["control"] == checkpoints[0]["intervention"],
        "final_worlds_equal": control.world() == changed.world(),
        "control_save_bytes": control.save_bytes()?.len(),
        "intervention_save_bytes": changed.save_bytes()?.len(),
    }))
}

/// Serial CPU measurements of the actual changing A/B worlds; no window or GPU time included.
pub fn measure_cpu() -> Result<Value, Box<dyn Error>> {
    measure_cpu_with(initial(SimulationStep::new(super::STEP.as_secs_f64())?)?)
}

pub(crate) fn measure_cpu_with(initial: Runtime) -> Result<Value, Box<dyn Error>> {
    let mut rows = Vec::new();
    for intervene in [false, true] {
        let mut runtime = initial.clone();
        let mut tick_ms = Vec::new();
        let mut snapshot_ms = Vec::new();
        let mut primitives = 0;
        let mut save_load_ms = None;
        for tick in 1..=240 {
            if tick == 61 && intervene {
                runtime.schedule(tick, intervention(&runtime))?;
            }
            let started = Instant::now();
            runtime.tick()?;
            tick_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            primitives = Scene::from_world(runtime.world())?.primitives().len();
            snapshot_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            if tick == 120 {
                let started = Instant::now();
                let saved = runtime.save_bytes()?;
                let loaded = Runtime::load_bytes(&saved)?;
                assert_eq!(runtime, loaded);
                save_load_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                runtime = loaded;
            }
        }
        let quantiles = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            [values[values.len() / 2], values[values.len() * 95 / 100]]
        };
        rows.push(json!({
            "intervention": intervene,
            "ticks": 240,
            "tick_ms_p50_p95": quantiles(&mut tick_ms),
            "snapshot_ms_p50_p95": quantiles(&mut snapshot_ms),
            "save_load_at_120_ms": save_load_ms,
            "final_primitives": primitives,
            "final_digest_fnv1a64": digest(&runtime)?,
        }));
    }
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_validated_intervention_changes_later_growth_and_replays() {
        let result = experiment().unwrap();
        assert_eq!(result["same_initial_state"], true);
        assert_eq!(result["replay_equal"], true);
        assert_eq!(result["checkpoints"][1]["world_equal"], true);
        assert_eq!(result["event"]["pre_tick_world_equal"], true);
        assert_eq!(result["final_worlds_equal"], false);
        assert_ne!(
            result["checkpoints"][6]["control"]["organisms"],
            result["checkpoints"][6]["intervention"]["organisms"],
            "the intervention must change later organism state, not only source position"
        );
        let control_nodes = result["checkpoints"][6]["control"]["organisms"][1]["nodes"]
            .as_u64()
            .unwrap();
        let changed_nodes = result["checkpoints"][6]["intervention"]["organisms"][1]["nodes"]
            .as_u64()
            .unwrap();
        assert!(control_nodes > changed_nodes + 10);
    }

    #[test]
    fn rejected_intervention_preserves_authoritative_state() {
        let mut runtime = initial(SimulationStep::new(crate::STEP.as_secs_f64()).unwrap()).unwrap();
        let before = runtime.clone();
        let id = runtime.world().sources()[0].id();
        assert!(
            runtime
                .schedule(
                    1,
                    EnvironmentEventKind::MoveSource {
                        id,
                        position: point(1001.0, 0.0, 0.0),
                    }
                )
                .is_err()
        );
        assert_eq!(runtime, before);
    }

    #[test]
    fn bounded_authored_scene_rejects_invalid_rules() {
        let step = SimulationStep::new(crate::STEP.as_secs_f64()).unwrap();
        let source = include_bytes!("../scenes/living-world.json");
        assert_eq!(parse_scene(source, step).unwrap(), initial(step).unwrap());
        let mut invalid: Value = serde_json::from_slice(source).unwrap();
        invalid["organisms"][0]["max_children"] = json!(3);
        assert!(parse_scene(&serde_json::to_vec(&invalid).unwrap(), step).is_err());
        invalid["organisms"][0]["max_children"] = json!(1);
        invalid["sources"][0]["stored"] = json!(-1.0);
        assert!(parse_scene(&serde_json::to_vec(&invalid).unwrap(), step).is_err());
        invalid["sources"][0]["stored"] = json!(0.2);
        invalid["format_version"] = json!(2);
        assert!(parse_scene(&serde_json::to_vec(&invalid).unwrap(), step).is_err());
        assert!(parse_scene(&vec![b' '; MAX_SCENE_BYTES + 1], step).is_err());
    }
}
