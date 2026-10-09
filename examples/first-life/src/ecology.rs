//! Machine-readable finite-resource experiments using the authoritative runtime.

use analytic_renderer::Scene;
use serde_json::{Value, json};
use spatial_math::Vec3;
use std::{error::Error, time::Instant};
use world_runtime::Runtime;
use world_simulation::{EnvironmentEventKind, SimulationStep, contact::ContactScene};
use world_state::{DeterministicSeed, GrowthNode, GrowthParameters, WorldState};

const TICKS: u64 = 40;

fn point(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite experiment coordinate")
}

fn initial(name: &str) -> Result<Runtime, Box<dyn Error>> {
    let mut world = WorldState::new(DeterministicSeed(31));
    let count = usize::from(name != "isolated") + 1;
    for index in 0..count {
        let x = if name == "separated" {
            if index == 0 { -2.0 } else { 2.0 }
        } else if index == 0 {
            -0.6
        } else {
            0.6
        };
        world.spawn_organism(
            point(x, 0.0, 0.0),
            GrowthParameters::new(0.14, 0.32, 0.5, 1.0)?,
        )?;
    }
    if name == "separated" {
        for x in [-2.0, 2.0] {
            world.spawn_finite_source(point(x, 2.0, 0.0), 3.0, 10.0, 0.0, 0.2, 0.2)?;
        }
    } else {
        world.spawn_finite_source(point(0.0, 2.0, 0.0), 4.0, 10.0, 0.0, 0.2, 0.2)?;
    }
    Ok(Runtime::new(world, SimulationStep::new(0.1)?))
}

pub fn run(name: &str) -> Result<(Runtime, Value), Box<dyn Error>> {
    if ![
        "isolated",
        "competition",
        "separated",
        "environment-change",
        "pruning",
        "replay",
    ]
    .contains(&name)
    {
        return Err("unknown ecology scenario".into());
    }
    let mut runtime = initial(name)?;
    for _ in 0..TICKS {
        if runtime.time().ticks() == 19 {
            let source = runtime.world().sources()[0].id();
            if name == "environment-change" {
                runtime.schedule(
                    20,
                    EnvironmentEventKind::SetSourceActive {
                        id: source,
                        active: false,
                    },
                )?;
            }
            if name == "pruning" {
                let organism = runtime.world().organisms()[0].id();
                runtime.schedule(20, EnvironmentEventKind::PruneBranch { organism, child: 1 })?;
            }
        }
        runtime.tick()?;
        if name == "replay" && runtime.time().ticks() == 20 {
            runtime = Runtime::load_bytes(&runtime.save_bytes()?)?;
        }
    }
    let scene = Scene::from_world(runtime.world())?;
    let saved = runtime.save_bytes()?;
    assert_eq!(Runtime::load_bytes(&saved)?, runtime);
    let fingerprint = saved.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    let organisms: Vec<_> = runtime
        .world()
        .organisms()
        .iter()
        .map(|organism| {
            json!({
                    "id": organism.id().value(),
                    "nodes": organism.nodes().len(),
                    "branches": organism.branch_count(),
            "active_tips": organism.active_count(),
            "lifecycle": organism.lifecycle_state(),
                    "budget": organism.budget(),
                    "next_node_id": organism.next_node_id(),
                })
        })
        .collect();
    let sources: Vec<_> = runtime
        .world()
        .sources()
        .iter()
        .map(|source| {
            let stock = source.reservoir().expect("finite ecology source");
            json!({
                "id": source.id().value(), "active": source.active(),
            "stored": stock.stored(), "capacity": stock.capacity(),
            "initial_stored": stock.initial_stored(),
            "last_replenished": stock.last_replenished(),
            "total_replenished": stock.total_replenished(),
                "last_allocated": stock.last_allocated(),
                "total_allocated": stock.total_allocated(),
            })
        })
        .collect();
    let summary = json!({
        "scenario": name, "tick": runtime.time().ticks(),
        "organisms": organisms, "sources": sources,
        "total_consumed": runtime.world().sources().iter()
            .map(|source| source.reservoir().unwrap().total_allocated()).sum::<f64>(),
        "snapshot_primitives": scene.primitives().len(),
        "save_bytes": saved.len(),
        "state_fingerprint_fnv1a64": format!("{fingerprint:016x}"),
    });
    Ok((runtime, summary))
}

/// Serial local timing of reproducible stable and growing ecological ticks.
pub fn measure() -> Result<Value, Box<dyn Error>> {
    let mut rows = Vec::new();
    for count in [1usize, 2, 4] {
        for growing in [false, true] {
            let mut world = WorldState::new(DeterministicSeed(31));
            for index in 0..count {
                world.spawn_organism(
                    point(index as f64 * 1.2 - (count - 1) as f64 * 0.6, 0.0, 0.0),
                    GrowthParameters::new(0.14, 0.32, 0.5, 1.0)?,
                )?;
            }
            let source =
                world.spawn_finite_source(point(0.0, 2.0, 0.0), 4.0, 10.0, 0.0, 0.2, 0.2)?;
            if !growing {
                world.source_mut(source)?.set_active(false);
            }
            let mut baseline = Runtime::new(world, SimulationStep::new(0.1)?);
            if growing {
                for _ in 0..100 {
                    let mut next = baseline.clone();
                    next.tick()?;
                    let old_nodes: usize = baseline
                        .world()
                        .organisms()
                        .iter()
                        .map(|o| o.nodes().len())
                        .sum();
                    let new_nodes: usize = next
                        .world()
                        .organisms()
                        .iter()
                        .map(|o| o.nodes().len())
                        .sum();
                    if new_nodes > old_nodes {
                        break;
                    }
                    baseline = next;
                }
            }
            let mut tick_ms = Vec::new();
            let mut snapshot_ms = Vec::new();
            let mut collider_ms = Vec::new();
            let mut result_nodes = 0;
            for _ in 0..50 {
                let mut copy = baseline.clone();
                let started = Instant::now();
                copy.tick()?;
                tick_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                result_nodes = copy
                    .world()
                    .organisms()
                    .iter()
                    .map(|o| o.nodes().len())
                    .sum();
                let started = Instant::now();
                let scene = Scene::from_world(copy.world())?;
                std::hint::black_box(scene.primitives().len());
                snapshot_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                let contact = ContactScene::from_world(copy.world())?;
                std::hint::black_box(contact.node_count());
                collider_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let pair = |values: &mut Vec<f64>| {
                values.sort_by(f64::total_cmp);
                [values[24], values[47]]
            };
            rows.push(json!({
                "organisms": count, "growing_tick": growing, "baseline_tick": baseline.time().ticks(),
                "result_nodes": result_nodes,
                "full_cpu_tick_ms_median_p95": pair(&mut tick_ms),
                "snapshot_ms_median_p95": pair(&mut snapshot_ms),
                "contact_bvh_build_ms_median_p95": pair(&mut collider_ms),
                "node_storage_lower_bound_bytes": result_nodes * std::mem::size_of::<GrowthNode>(),
                "save_bytes": baseline.save_bytes()?.len(),
            }));
        }
    }
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_competition_changes_authoritative_growth_and_replays() {
        let (single, isolated) = run("isolated").unwrap();
        let (shared, competition) = run("competition").unwrap();
        let (separate, separated) = run("separated").unwrap();
        let (replayed, replay) = run("replay").unwrap();
        assert_eq!(shared, replayed);
        assert_eq!(
            competition["state_fingerprint_fnv1a64"],
            replay["state_fingerprint_fnv1a64"]
        );
        assert!(
            single.world().organisms()[0].nodes().len()
                > shared.world().organisms()[0].nodes().len()
        );
        assert!(
            separate.world().organisms()[0].nodes().len()
                > shared.world().organisms()[0].nodes().len()
        );
        assert_eq!(isolated["tick"], TICKS);
        assert_eq!(separated["organisms"].as_array().unwrap().len(), 2);
        let (_, changed) = run("environment-change").unwrap();
        let (_, pruned) = run("pruning").unwrap();
        assert_ne!(
            changed["state_fingerprint_fnv1a64"],
            competition["state_fingerprint_fnv1a64"]
        );
        assert_ne!(
            pruned["state_fingerprint_fnv1a64"],
            competition["state_fingerprint_fnv1a64"]
        );
        assert!(
            pruned["organisms"][0]["next_node_id"].as_u64().unwrap()
                > pruned["organisms"][0]["nodes"].as_u64().unwrap()
        );
    }
}
