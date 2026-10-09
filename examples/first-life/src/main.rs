//! Reproducible headless growth scenarios and optional local timing diagnostics.

use analytic_renderer::Scene;
use spatial_math::Vec3;
use std::{error::Error, time::Instant};
use world_runtime::{PersistenceError, Runtime};
use world_simulation::{EnvironmentEventKind, SimulationStep};
use world_state::{DeterministicSeed, GrowthNode, GrowthParameters, WorldState};

mod ecology;

const TICKS: u64 = 120;

fn point(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite scenario point")
}

fn initial(name: &str) -> Result<Runtime, Box<dyn Error>> {
    let mut world = WorldState::new(DeterministicSeed(7));
    world.spawn_organism(Vec3::ZERO, GrowthParameters::new(0.14, 0.32, 0.12, 1.0)?)?;
    let strength = if name == "limited" { 0.05 } else { 1.0 };
    let source = world.spawn_source(point(0.0, 2.0, 0.0), 4.0, strength)?;
    if name.starts_with("contact") {
        world.spawn_body(point(0.12, 0.32, -1.0), 0.1)?;
    }
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
        "contact",
        "contact-pruned",
        "contact-replay",
    ]
    .contains(&name)
    {
        return Err(
            "scenario must be baseline, changed, limited, pruning, pruning-replay, contact, contact-pruned, or contact-replay".into(),
        );
    }
    let mut runtime = initial(name)?;
    let mut first_contact_tick = None;
    let mut first_contact = None;
    let started = Instant::now();
    for _ in 0..TICKS {
        if name.starts_with("pruning") && runtime.time().ticks() == 59 {
            let organism = runtime.world().organisms()[0].id();
            runtime.schedule(60, EnvironmentEventKind::PruneBranch { organism, child: 1 })?;
        }
        if name.starts_with("contact") && runtime.time().ticks() == 59 {
            if name != "contact" {
                let organism = runtime.world().organisms()[0].id();
                runtime.schedule(60, EnvironmentEventKind::PruneBranch { organism, child: 1 })?;
                let source = runtime.world().sources()[0].id();
                runtime.schedule(
                    60,
                    EnvironmentEventKind::SetSourceActive {
                        id: source,
                        active: false,
                    },
                )?;
            }
            let id = runtime.world().body().expect("contact body").id();
            runtime.schedule(
                60,
                EnvironmentEventKind::SetBodyVelocity {
                    id,
                    velocity: point(0.0, 0.0, 4.0),
                },
            )?;
        }
        if name.starts_with("contact") && runtime.time().ticks() == 89 {
            let id = runtime.world().body().expect("contact body").id();
            runtime.schedule(
                90,
                EnvironmentEventKind::SetBodyVelocity {
                    id,
                    velocity: Vec3::ZERO,
                },
            )?;
        }
        runtime.tick()?;
        if first_contact.is_none()
            && let Some(hit) = runtime.world().body().and_then(|body| body.contact())
        {
            first_contact_tick = Some(runtime.time().ticks());
            first_contact = Some(hit.collider);
        }
        if (name == "pruning-replay" || name == "contact-replay") && runtime.time().ticks() == 80 {
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
    let pruned =
        name.starts_with("pruning") || name == "contact-pruned" || name == "contact-replay";
    let event_outcome = if pruned {
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
        "prune_target": if pruned { Some(serde_json::json!({"tick":60,"child":1})) } else { None },
        "pruned_child_active": organism.node(1).is_some(),
        "next_node_id": organism.next_node_id(),
        "body_id": runtime.world().body().map(|body| body.id().value()),
        "body_position": runtime.world().body().map(|body| [body.position().x(), body.position().y(), body.position().z()]),
        "body_contact": runtime.world().body().and_then(|body| body.contact()).map(|hit| hit.collider),
        "first_contact_tick": first_contact_tick,
        "first_contact": first_contact,
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
    if args.first().is_some_and(|arg| arg == "ecology") {
        if args.len() == 2 && args[1] == "measure" {
            println!("{}", ecology::measure()?);
            return Ok(());
        }
        if args.len() != 2 {
            return Err("usage: first-life ecology <scenario>".into());
        }
        let (_, summary) = ecology::run(&args[1])?;
        println!("{summary}");
        return Ok(());
    }
    let name = args.first().map(String::as_str).unwrap_or("baseline");
    if args.len() > 2 || args.get(1).is_some_and(|arg| arg != "--measure") {
        return Err(
            "usage: first-life [baseline|changed|limited|pruning|pruning-replay|contact|contact-pruned|contact-replay] [--measure]"
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
    fn contact_is_blocked_until_pruning_clears_the_path_and_replay_matches() {
        let (blocked, before) = run("contact", false).unwrap();
        let (cleared, after) = run("contact-pruned", false).unwrap();
        let (replay, repeated) = run("contact-replay", false).unwrap();
        assert_eq!(cleared, replay);
        assert_eq!(
            after["state_fingerprint_fnv1a64"],
            repeated["state_fingerprint_fnv1a64"]
        );
        assert_eq!(before["first_contact_tick"], 71);
        assert_eq!(before["first_contact"]["Node"]["node"], 1);
        assert_eq!(after["first_contact"], serde_json::Value::Null);
        assert!(blocked.world().body().unwrap().position().z() < 0.0);
        assert!(cleared.world().body().unwrap().position().z() > 0.9);
    }

    #[test]
    fn finite_ecology_pick_prune_contact_snapshot_and_save_replay() {
        use analytic_renderer::{PickOutcome, Ray, SemanticTarget};
        use world_simulation::contact::{ContactScene, SweepOutcome};

        let mut world = WorldState::new(DeterministicSeed(81));
        let parameters = GrowthParameters::new(0.14, 0.32, 0.2, 1.0).unwrap();
        let first = world
            .spawn_organism(point(-0.6, 0.0, 0.0), parameters)
            .unwrap();
        world
            .spawn_organism(point(0.6, 0.0, 0.0), parameters)
            .unwrap();
        let source = world
            .spawn_finite_source(point(0.0, 2.0, 0.0), 4.0, 10.0, 0.0, 0.2, 0.2)
            .unwrap();
        let start = point(-0.6, 0.32, -1.0);
        let body = world.spawn_body(start, 0.1).unwrap();
        let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        while runtime.world().organisms()[0].node(1).is_none() {
            assert!(runtime.time().ticks() < 20);
            runtime.tick().unwrap();
        }
        let first_tree = &runtime.world().organisms()[0];
        let child = first_tree.node(1).unwrap();
        let midpoint = first_tree
            .root()
            .checked_add(child.position())
            .unwrap()
            .checked_scale(0.5)
            .unwrap();
        let scene = Scene::from_world(runtime.world()).unwrap();
        let ray = Ray::new(
            point(midpoint.x(), midpoint.y(), 3.0),
            point(0.0, 0.0, -1.0),
            0.0,
            10.0,
        )
        .unwrap();
        let PickOutcome::Hit(picked) = scene.pick_ray(ray).unwrap() else {
            panic!("expected semantic connection selection")
        };
        assert_eq!(
            picked.target,
            SemanticTarget::Connection {
                organism: first,
                child: 1
            }
        );
        let movement = point(0.0, 0.0, 2.0);
        let before = ContactScene::from_world(runtime.world()).unwrap();
        assert!(matches!(
            before.sweep(start, movement, 0.1).unwrap(),
            SweepOutcome::Hit(_)
        ));
        assert_eq!(
            before.sweep(start, movement, 0.1),
            before.sweep_direct(start, movement, 0.1)
        );
        let tick = runtime.time().ticks() + 1;
        let mut allocation_probe = runtime.clone();
        allocation_probe
            .schedule(
                tick,
                EnvironmentEventKind::PruneBranch {
                    organism: first,
                    child: 1,
                },
            )
            .unwrap();
        allocation_probe.tick().unwrap();
        let mut unpruned = runtime.clone();
        unpruned.tick().unwrap();
        assert_ne!(
            allocation_probe.world().organisms()[0].budget(),
            unpruned.world().organisms()[0].budget(),
            "pruning must change the first organism's resource share"
        );
        assert_ne!(
            allocation_probe.world().organisms()[1].budget(),
            unpruned.world().organisms()[1].budget(),
            "shared stock must redistribute to the remaining organism"
        );
        runtime
            .schedule(
                tick,
                EnvironmentEventKind::PruneBranch {
                    organism: first,
                    child: 1,
                },
            )
            .unwrap();
        runtime
            .schedule(
                tick,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false,
                },
            )
            .unwrap();
        runtime
            .schedule(
                tick,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: point(0.0, 0.0, 20.0),
                },
            )
            .unwrap();
        runtime.tick().unwrap();
        assert!(runtime.world().organisms()[0].node(1).is_none());
        assert!(runtime.world().body().unwrap().position().z() > 0.9);
        assert_eq!(
            runtime.world().sources()[0]
                .reservoir()
                .unwrap()
                .last_allocated(),
            0.0
        );
        let after = ContactScene::from_world(runtime.world()).unwrap();
        assert_eq!(
            after.sweep(start, movement, 0.1).unwrap(),
            SweepOutcome::Miss
        );
        assert_eq!(
            after.sweep(start, movement, 0.1),
            after.sweep_direct(start, movement, 0.1)
        );
        assert!(
            Scene::from_world(runtime.world())
                .unwrap()
                .primitives()
                .iter()
                .all(|primitive| primitive.target() != Some(picked.target))
        );
        let other_nodes = runtime.world().organisms()[1].nodes().len();
        let tick = runtime.time().ticks() + 1;
        runtime
            .schedule(
                tick,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: Vec3::ZERO,
                },
            )
            .unwrap();
        runtime
            .schedule(
                tick,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: true,
                },
            )
            .unwrap();
        for _ in 0..10 {
            runtime.tick().unwrap();
            runtime.world().validate().unwrap();
            Scene::from_world(runtime.world()).unwrap();
        }
        let mut resumed = Runtime::load_bytes(&runtime.save_bytes().unwrap()).unwrap();
        for _ in 0..20 {
            runtime.tick().unwrap();
            resumed.tick().unwrap();
            Scene::from_world(runtime.world()).unwrap();
            Scene::from_world(resumed.world()).unwrap();
        }
        assert_eq!(runtime, resumed);
        assert_eq!(runtime.save_bytes().unwrap(), resumed.save_bytes().unwrap());
        assert!(runtime.world().organisms()[1].nodes().len() > other_nodes);
    }

    #[test]
    fn multi_seed_ecology_contact_and_snapshot_stress_replays() {
        use world_simulation::contact::ContactScene;

        fn drive(runtime: &mut Runtime) {
            let next = runtime.time().ticks() + 1;
            let seed = runtime.world().seed().0;
            let source = runtime.world().sources()[0].id();
            if next.is_multiple_of(40) {
                runtime
                    .schedule(
                        next,
                        EnvironmentEventKind::MoveSource {
                            id: source,
                            position: point(
                                if (next / 40 + seed).is_multiple_of(2) {
                                    -1.0
                                } else {
                                    1.0
                                },
                                2.0,
                                0.0,
                            ),
                        },
                    )
                    .unwrap();
            }
            if next.is_multiple_of(60) {
                let organism = &runtime.world().organisms()[(seed as usize) % 4];
                if let Some(child) = organism.nodes().iter().find(|node| node.parent().is_some()) {
                    runtime
                        .schedule(
                            next,
                            EnvironmentEventKind::PruneBranch {
                                organism: organism.id(),
                                child: child.id(),
                            },
                        )
                        .unwrap();
                }
            }
            if next.is_multiple_of(30) {
                runtime
                    .schedule(
                        next,
                        EnvironmentEventKind::SetBodyVelocity {
                            id: runtime.world().body().unwrap().id(),
                            velocity: point(
                                0.0,
                                0.0,
                                if (next / 30 + seed).is_multiple_of(2) {
                                    -0.5
                                } else {
                                    0.5
                                },
                            ),
                        },
                    )
                    .unwrap();
            }
            let previous_ids: Vec<_> = runtime
                .world()
                .organisms()
                .iter()
                .map(|tree| tree.next_node_id())
                .collect();
            runtime.tick().unwrap();
            runtime.world().validate().unwrap();
            assert!(runtime.pending_events().is_empty());
            for (tree, previous) in runtime.world().organisms().iter().zip(previous_ids) {
                assert!(tree.next_node_id() >= previous);
            }
            let stock = runtime.world().sources()[0].reservoir().unwrap();
            let balance =
                stock.initial_stored() + stock.total_replenished() - stock.total_allocated();
            assert!(
                (balance - stock.stored()).abs()
                    <= 512.0
                        * f64::EPSILON
                        * (1.0 + stock.total_replenished() + stock.total_allocated())
            );
            let scene = ContactScene::from_world(runtime.world()).unwrap();
            let start = point(0.0, 0.32, -1.0);
            let displacement = point(0.0, 0.0, 2.0);
            assert_eq!(
                scene.sweep(start, displacement, 0.1),
                scene.sweep_direct(start, displacement, 0.1)
            );
            assert!(
                Scene::from_world(runtime.world())
                    .unwrap()
                    .primitives()
                    .len()
                    <= 512
            );
            assert!(runtime.save_bytes().unwrap().len() < world_runtime::MAX_SAVE_BYTES);
        }
        for seed in [13, 38, 80] {
            let mut world = WorldState::new(DeterministicSeed(seed));
            let parameters = GrowthParameters::new(0.1, 0.3, 0.2, 1.0).unwrap();
            for x in [-1.5, -0.5, 0.5, 1.5] {
                world
                    .spawn_organism(point(x, 0.0, 0.0), parameters)
                    .unwrap();
            }
            world
                .spawn_finite_source(point(0.0, 2.0, 0.0), 4.0, 10.0, 0.0, 0.2, 0.2)
                .unwrap();
            world.spawn_body(point(0.0, 0.32, -1.0), 0.1).unwrap();
            let mut continuous = Runtime::new(world, SimulationStep::new(0.1).unwrap());
            for _ in 0..120 {
                drive(&mut continuous);
            }
            let mut resumed = Runtime::load_bytes(&continuous.save_bytes().unwrap()).unwrap();
            for _ in 120..240 {
                drive(&mut continuous);
                drive(&mut resumed);
            }
            assert_eq!(continuous, resumed, "seed {seed}");
            assert_eq!(
                continuous.save_bytes().unwrap(),
                resumed.save_bytes().unwrap()
            );
        }
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
