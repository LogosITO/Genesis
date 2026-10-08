//! Reproducible headless growth scenarios and optional local timing diagnostics.

use analytic_renderer::Scene;
use spatial_math::Vec3;
use std::{error::Error, time::Instant};
use world_runtime::Runtime;
use world_simulation::{EnvironmentEventKind, SimulationStep};
use world_state::{DeterministicSeed, GrowthParameters, WorldState};

const TICKS: u64 = 120;

fn point(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite scenario point")
}

fn initial(name: &str) -> Result<Runtime, Box<dyn Error>> {
    let mut world = WorldState::new(DeterministicSeed(7));
    world.spawn_organism(Vec3::ZERO, GrowthParameters::new(0.14, 0.32, 0.12, 1.0)?)?;
    let strength = if name == "limited" { 0.05 } else { 1.0 };
    let source = world.spawn_source(point(0.0, 2.0, 0.0), 4.0, strength)?;
    let mut runtime = Runtime::new(world, SimulationStep::new(1.0 / 60.0)?);
    if name == "changed" {
        runtime.schedule(
            40,
            EnvironmentEventKind::MoveSource {
                id: source,
                position: point(2.0, 1.0, 0.0),
            },
        )?;
    }
    Ok(runtime)
}

fn run(name: &str, measure: bool) -> Result<(Runtime, serde_json::Value), Box<dyn Error>> {
    if !["baseline", "changed", "limited"].contains(&name) {
        return Err("scenario must be baseline, changed, or limited".into());
    }
    let mut runtime = initial(name)?;
    let started = Instant::now();
    for _ in 0..TICKS {
        runtime.tick()?;
    }
    let simulation_time = started.elapsed();
    let started = Instant::now();
    let scene = Scene::from_world(runtime.world())?;
    let snapshot_time = started.elapsed();
    let started = Instant::now();
    let bytes = runtime.save_bytes()?;
    let save_time = started.elapsed();
    let started = Instant::now();
    let loaded = Runtime::load_bytes(&bytes)?;
    let load_time = started.elapsed();
    assert_eq!(loaded, runtime);
    if measure {
        eprintln!(
            "scenario={name} ticks={TICKS} nodes={} cpu_step_mean_us={:.3} snapshot_us={:.3} save_us={:.3} load_us={:.3} save_bytes={} (single uncalibrated run)",
            runtime.world().organisms()[0].nodes().len(),
            simulation_time.as_secs_f64() * 1e6 / TICKS as f64,
            snapshot_time.as_secs_f64() * 1e6,
            save_time.as_secs_f64() * 1e6,
            load_time.as_secs_f64() * 1e6,
            bytes.len()
        );
    }
    let organism = &runtime.world().organisms()[0];
    let tip = organism.nodes().last().expect("root exists").position();
    let fingerprint = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    let summary = serde_json::json!({
        "scenario": name,
        "ticks": TICKS,
        "organism_id": organism.id().value(),
        "node_count": organism.nodes().len(),
        "tip": [tip.x(), tip.y(), tip.z()],
        "snapshot_primitives": scene.primitives().len(),
        "state_fingerprint_fnv1a64": format!("{fingerprint:016x}")
    });
    Ok((runtime, summary))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let name = args.first().map(String::as_str).unwrap_or("baseline");
    if args.len() > 2 || args.get(1).is_some_and(|arg| arg != "--measure") {
        return Err("usage: first-life [baseline|changed|limited] [--measure]".into());
    }
    let (_, summary) = run(name, args.len() == 2)?;
    println!("{summary}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenarios_and_replay_are_distinct_and_exact() {
        let (base, baseline) = run("baseline", false).unwrap();
        let (again, repeated) = run("baseline", false).unwrap();
        assert_eq!(base, again);
        assert_eq!(baseline, repeated);
        let (changed, changed_summary) = run("changed", false).unwrap();
        let (limited, limited_summary) = run("limited", false).unwrap();
        assert_ne!(base, changed);
        assert_ne!(base, limited);
        assert_ne!(baseline["tip"], changed_summary["tip"]);
        assert_eq!(limited_summary["node_count"], 1);
        assert!(baseline["node_count"].as_u64().unwrap() > 1);
    }

    #[test]
    fn save_load_continuation_matches_uninterrupted() {
        let mut continuous = initial("changed").unwrap();
        let mut split = continuous.clone();
        for _ in 0..TICKS {
            continuous.tick().unwrap();
        }
        for _ in 0..35 {
            split.tick().unwrap();
        }
        let mut restored = Runtime::load_bytes(&split.save_bytes().unwrap()).unwrap();
        for _ in 35..TICKS {
            restored.tick().unwrap();
        }
        assert_eq!(continuous, restored);
    }

    #[test]
    fn render_snapshot_frequency_does_not_change_fixed_ticks() {
        let mut sparse = initial("changed").unwrap();
        let mut frequent = sparse.clone();
        for _ in 0..60 {
            sparse.tick().unwrap();
            for _ in 0..5 {
                let scene = Scene::from_world(frequent.world()).unwrap();
                assert_eq!(
                    scene.primitives().len(),
                    frequent.world().organisms()[0].nodes().len() + 1
                );
            }
            frequent.tick().unwrap();
        }
        assert_eq!(sparse, frequent);
    }
}
