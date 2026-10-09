//! Reproducible headless growth scenarios and optional local timing diagnostics.

use analytic_renderer::Scene;
use spatial_math::Vec3;
use std::{error::Error, time::Instant};
use world_runtime::{PersistenceError, Runtime};
use world_simulation::{EnvironmentEventKind, SimulationStep};
use world_state::{DeterministicSeed, GrowthNode, GrowthParameters, WorldState};

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
    if ![
        "baseline",
        "changed",
        "limited",
        "pruning",
        "pruning-replay",
    ]
    .contains(&name)
    {
        return Err(
            "scenario must be baseline, changed, limited, pruning, or pruning-replay".into(),
        );
    }
    let mut runtime = initial(name)?;
    let started = Instant::now();
    for _ in 0..TICKS {
        if name.starts_with("pruning") && runtime.time().ticks() == 59 {
            let organism = runtime.world().organisms()[0].id();
            runtime.schedule(60, EnvironmentEventKind::PruneBranch { organism, child: 1 })?;
        }
        runtime.tick()?;
        if name == "pruning-replay" && runtime.time().ticks() == 80 {
            runtime = Runtime::load_bytes(&runtime.save_bytes()?)?;
        }
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
    let event_outcome = if name.starts_with("pruning") {
        if organism.node(1).is_some() {
            return Err("prune event left target active".into());
        }
        "applied"
    } else {
        "none"
    };
    let last_node = organism.nodes().last().expect("root exists").position();
    let fingerprint = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    let summary = serde_json::json!({
        "scenario": name,
        "ticks": TICKS,
        "organism_id": organism.id().value(),
        "node_count": organism.nodes().len(),
        "active_count": organism.active_count(),
        "branch_count": organism.branch_count(),
        "event_outcome": event_outcome,
        "prune_target": if name.starts_with("pruning") { Some(serde_json::json!({"tick":60,"child":1})) } else { None },
        "pruned_child_active": organism.node(1).is_some(),
        "next_node_id": organism.next_node_id(),
        "last_allocated_node": [last_node.x(), last_node.y(), last_node.z()],
        "snapshot_primitives": scene.primitives().len(),
        "state_fingerprint_fnv1a64": format!("{fingerprint:016x}")
    });
    Ok((runtime, summary))
}

fn scale_sample(
    organisms: usize,
    target_nodes: usize,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut world = WorldState::new(DeterministicSeed(7));
    for index in 0..organisms {
        world.spawn_organism(
            point(index as f64 - 1.5, 0.0, 0.0),
            GrowthParameters::new(0.14, 0.32, 0.12, 1.0)?,
        )?;
    }
    world.spawn_source(point(0.0, 2.0, 0.0), 100.0, 10.0)?;
    let mut runtime = Runtime::new(world, SimulationStep::new(0.01)?);
    while runtime
        .world()
        .organisms()
        .iter()
        .any(|o| o.nodes().len() < target_nodes)
    {
        if runtime.time().ticks() >= 200 {
            return Err("scale scene did not reach requested node count".into());
        }
        runtime.tick()?;
    }
    let nodes: usize = runtime
        .world()
        .organisms()
        .iter()
        .map(|o| o.nodes().len())
        .sum();
    let mut step_us = Vec::new();
    let mut snapshot_us = Vec::new();
    let mut primitive_count = 0;
    for _ in 0..25 {
        let mut copy = runtime.clone();
        let started = Instant::now();
        copy.tick()?;
        step_us.push(started.elapsed().as_secs_f64() * 1e6);
        let started = Instant::now();
        primitive_count = Scene::from_world(runtime.world())?.primitives().len();
        snapshot_us.push(started.elapsed().as_secs_f64() * 1e6);
    }
    step_us.sort_by(f64::total_cmp);
    snapshot_us.sort_by(f64::total_cmp);
    let save_bytes = match runtime.save_bytes() {
        Ok(bytes) => Some(bytes.len()),
        Err(PersistenceError::TooLarge) => None,
        Err(error) => return Err(error.into()),
    };
    Ok(serde_json::json!({
        "organisms": organisms, "nodes": nodes, "ticks": runtime.time().ticks(),
        "primitives": primitive_count,
        "step_median_us": step_us[12], "snapshot_median_us": snapshot_us[12],
        "node_storage_lower_bound_bytes": nodes * std::mem::size_of::<GrowthNode>(),
        "save_bytes": save_bytes,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "scale") {
        if args.len() != 2 || args[1] != "--measure" {
            return Err("usage: first-life scale --measure".into());
        }
        let mut results = Vec::new();
        for count in [1, 8, 16, 32, 48] {
            results.push(scale_sample(1, count)?);
        }
        results.push(scale_sample(4, 48)?);
        println!("{}", serde_json::Value::Array(results));
        return Ok(());
    }
    let name = args.first().map(String::as_str).unwrap_or("baseline");
    if args.len() > 2 || args.get(1).is_some_and(|arg| arg != "--measure") {
        return Err(
            "usage: first-life [baseline|changed|limited|pruning|pruning-replay] [--measure]"
                .into(),
        );
    }
    let (_, summary) = run(name, args.len() == 2)?;
    println!("{summary}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "local CPU timing experiment; run serially in release"]
    fn interaction_cpu_benchmark() {
        use analytic_renderer::{Bvh, Ray};
        fn stats(mut values: Vec<f64>) -> (f64, f64) {
            values.sort_by(f64::total_cmp);
            (
                values[values.len() / 2],
                values[(values.len() * 95).div_ceil(100) - 1],
            )
        }
        fn measure(label: &str, runtime: Runtime) {
            let organism = runtime.world().organisms()[0].id();
            let child = runtime.world().organisms()[0].node(1).unwrap().position();
            let ray = Ray::new(
                point(child.x(), child.y(), child.z() - 5.0),
                point(0.0, 0.0, 1.0),
                0.0,
                100.0,
            )
            .unwrap();
            let scene = Scene::from_world(runtime.world()).unwrap();
            let mut picking = Vec::new();
            let mut validation = Vec::new();
            let mut pruning = Vec::new();
            let mut snapshot = Vec::new();
            let mut bvh = Vec::new();
            for _ in 0..25 {
                let started = Instant::now();
                std::hint::black_box(scene.pick_ray(ray).unwrap());
                picking.push(started.elapsed().as_secs_f64() * 1e3);
                let mut proposed = runtime.clone();
                let started = Instant::now();
                proposed
                    .schedule(
                        runtime.time().ticks() + 1,
                        EnvironmentEventKind::PruneBranch { organism, child: 1 },
                    )
                    .unwrap();
                validation.push(started.elapsed().as_secs_f64() * 1e3);
                let mut world = runtime.world().clone();
                let started = Instant::now();
                world.prune_branch(organism, 1).unwrap();
                pruning.push(started.elapsed().as_secs_f64() * 1e3);
                let started = Instant::now();
                let changed = Scene::from_world(&world).unwrap();
                snapshot.push(started.elapsed().as_secs_f64() * 1e3);
                let started = Instant::now();
                std::hint::black_box(Bvh::build(&changed).unwrap());
                bvh.push(started.elapsed().as_secs_f64() * 1e3);
            }
            eprintln!(
                "interaction_cpu scene={label} primitives={} pick_ms={:?} schedule_ms={:?} prune_ms={:?} snapshot_ms={:?} bvh_ms={:?}",
                scene.primitives().len(),
                stats(picking),
                stats(validation),
                stats(pruning),
                stats(snapshot),
                stats(bvh)
            );
        }
        let mut small = initial("baseline").unwrap();
        for _ in 0..60 {
            small.tick().unwrap();
        }
        measure("small", small);
        let mut world = WorldState::new(DeterministicSeed(7));
        for index in 0..4 {
            world
                .spawn_organism(
                    point(index as f64 - 1.5, 0.0, 0.0),
                    GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
                )
                .unwrap();
        }
        world
            .spawn_source(point(0.0, 2.0, 0.0), 100.0, 10.0)
            .unwrap();
        let mut large = Runtime::new(world, SimulationStep::new(0.01).unwrap());
        while large
            .world()
            .organisms()
            .iter()
            .any(|tree| tree.nodes().len() < 48)
        {
            large.tick().unwrap();
        }
        assert_eq!(
            Scene::from_world(large.world()).unwrap().primitives().len(),
            381
        );
        measure("real-381", large);
    }

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
        assert_ne!(
            baseline["last_allocated_node"],
            changed_summary["last_allocated_node"]
        );
        assert_eq!(limited_summary["node_count"], 1);
        assert!(baseline["node_count"].as_u64().unwrap() > 1);
    }

    #[test]
    fn pruning_and_save_replay_change_authoritative_topology() {
        let (baseline, base_summary) = run("baseline", false).unwrap();
        let (pruned, prune_summary) = run("pruning", false).unwrap();
        let (replayed, replay_summary) = run("pruning-replay", false).unwrap();
        assert_eq!(pruned, replayed);
        assert_ne!(baseline.world(), pruned.world());
        assert_ne!(
            base_summary["state_fingerprint_fnv1a64"],
            prune_summary["state_fingerprint_fnv1a64"]
        );
        assert_eq!(
            prune_summary["state_fingerprint_fnv1a64"],
            replay_summary["state_fingerprint_fnv1a64"]
        );
        assert_eq!(prune_summary["pruned_child_active"], false);
        assert_eq!(prune_summary["ticks"], 120);
        assert!(
            pruned.world().organisms()[0].next_node_id()
                > pruned.world().organisms()[0].nodes().len() as u32
        );
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
                    frequent.world().organisms()[0].nodes().len() * 2
                );
            }
            frequent.tick().unwrap();
        }
        assert_eq!(sparse, frequent);
    }
}
