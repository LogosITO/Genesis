//! Demo-local level and objective rules for The Passage.

use analytic_field::Sphere;
use analytic_renderer::{Primitive, Scene, SemanticTarget};
use spatial_math::{Transform, Vec3};
use world_runtime::Runtime;
use world_simulation::SimulationStep;
use world_state::{DeterministicSeed, EntityId, GrowthParameters, WorldState};

pub const GOAL_Z: f64 = 1.5;
const BODY_Y: f64 = 0.35;

fn point(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite level point")
}

pub fn initial() -> Result<Runtime, Box<dyn std::error::Error>> {
    let mut world = WorldState::new(DeterministicSeed(29));
    // Analytic spheres form a narrow lane; the central organism's removable branch closes it.
    for z in [-1.2, -0.6, 0.0, 0.6, 1.2] {
        for x in [-0.55, 0.55] {
            world.spawn_sphere(
                Sphere::new(0.39)?,
                Transform::new(point(x, BODY_Y, z), 1.0)?,
                0.0,
            )?;
        }
    }
    let params = GrowthParameters::new(0.14, 0.32, 0.12, 1.0)?.with_max_children(1)?;
    world.spawn_organism(point(0.0, 0.0, 0.0), params)?;
    world.spawn_organism(point(1.5, 0.0, 0.2), params)?;
    world.spawn_finite_source(point(0.55, 1.1, 0.0), 3.0, 10.0, 0.0, 0.2, 0.2)?;
    world.spawn_body(point(0.0, BODY_Y, -1.55), 0.1)?;
    let mut runtime = Runtime::new(world, SimulationStep::new(1.0 / 60.0)?);
    for _ in 0..45 {
        runtime.tick()?;
    }
    Ok(runtime)
}

pub fn goal_reached(world: &WorldState) -> bool {
    world
        .organisms()
        .first()
        .is_some_and(|tree| tree.node(1).is_none())
        && world
            .sources()
            .first()
            .is_some_and(|source| source.position().x() > 2.0)
        && world.body().is_some_and(|body| {
            let p = body.position();
            p.x() * p.x() + (p.y() - BODY_Y).powi(2) + (p.z() - GOAL_Z).powi(2) <= 0.25 * 0.25
        })
}

pub fn decorate(scene: &mut Scene) -> Result<(), Box<dyn std::error::Error>> {
    // The goal is a visual marker; only the authoritative body centre determines victory.
    let id = u32::try_from(scene.primitives().len())?;
    scene.push(Primitive::sphere(
        id,
        Sphere::new(0.18)?,
        Transform::new(point(0.0, 0.55, GOAL_Z), 1.0)?,
        [0.91, 0.29, 0.85],
    )?)?;
    Ok(())
}

pub fn branch(target: Option<SemanticTarget>, world: &WorldState) -> Option<(EntityId, u32)> {
    let (organism, child) = match target? {
        SemanticTarget::Connection { organism, child }
        | SemanticTarget::GrowthNode {
            organism,
            node: child,
        } => (organism, child),
        _ => return None,
    };
    (child != 0
        && world
            .organisms()
            .iter()
            .any(|tree| tree.id() == organism && tree.node(child).is_some()))
    .then_some((organism, child))
}

pub fn measure_cpu() -> Result<(), Box<dyn std::error::Error>> {
    use std::{hint::black_box, time::Instant};
    use world_simulation::contact::ContactScene;
    let mut runtime = initial()?;
    let mut steps = Vec::new();
    for _ in 0..120 {
        let start = Instant::now();
        runtime.tick()?;
        steps.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let mut builds = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        black_box(ContactScene::from_world(runtime.world())?);
        builds.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let mut contacts = Vec::new();
    let scene = ContactScene::from_world(runtime.world())?;
    for _ in 0..1000 {
        let start = Instant::now();
        black_box(scene.sweep(point(0.0, 0.35, -0.3), point(0.0, 0.0, 0.6), 0.1)?);
        contacts.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let mut snapshots = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        black_box(Scene::from_world(runtime.world())?);
        snapshots.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let (step_p50, step_p95) = super::median_p95(&steps);
    let (build_p50, build_p95) = super::median_p95(&builds);
    let (contact_p50, contact_p95) = super::median_p95(&contacts);
    let (snapshot_p50, snapshot_p95) = super::median_p95(&snapshots);
    println!(
        "{{\"scenario\":\"the-passage\",\"profile\":\"release\",\"step_ms_p50\":{step_p50:.6},\"step_ms_p95\":{step_p95:.6},\"contact_bvh_build_ms_p50\":{build_p50:.6},\"contact_bvh_build_ms_p95\":{build_p95:.6},\"contact_query_ms_p50\":{contact_p50:.6},\"contact_query_ms_p95\":{contact_p95:.6},\"snapshot_ms_p50\":{snapshot_p50:.6},\"snapshot_ms_p95\":{snapshot_p95:.6}}}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use analytic_renderer::{PickOutcome, Ray};
    use world_simulation::EnvironmentEventKind;
    use world_simulation::contact::{ContactScene, SweepOutcome};

    fn velocity(runtime: &mut Runtime, z: f64) {
        let body = runtime.world().body().unwrap().id();
        runtime
            .schedule(
                runtime.time().ticks() + 1,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: point(0.0, 0.0, z),
                },
            )
            .unwrap();
    }

    #[test]
    fn passage_playthrough_replay_and_restart() {
        let mut game = initial().unwrap();
        let start = game.clone();
        let first = game.world().organisms()[0].id();
        let child = game.world().organisms()[0].nodes()[1].id();
        let node = game.world().organisms()[0].node(child).unwrap().position();
        let picked = Scene::from_world(game.world())
            .unwrap()
            .pick_ray(
                Ray::new(
                    point(node.x(), node.y(), node.z() - 0.6),
                    point(0.0, 0.0, 1.0),
                    0.0,
                    2.0,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            matches!(picked, PickOutcome::Hit(hit) if hit.target == SemanticTarget::GrowthNode { organism: first, node: child } || hit.target == SemanticTarget::Connection { organism: first, child })
        );
        assert_eq!(
            branch(
                Some(SemanticTarget::Connection {
                    organism: first,
                    child
                }),
                game.world()
            ),
            Some((first, child))
        );
        assert_eq!(
            branch(
                Some(SemanticTarget::GrowthNode {
                    organism: first,
                    node: 0
                }),
                game.world()
            ),
            None
        );
        assert!(
            game.schedule(
                game.time().ticks() + 1,
                EnvironmentEventKind::PruneBranch {
                    organism: first,
                    child: 0
                }
            )
            .is_err()
        );

        velocity(&mut game, 2.0);
        for _ in 0..100 {
            game.tick().unwrap();
        }
        let blocked = game.world().body().unwrap().position().z();
        assert!(blocked < 0.0, "path unexpectedly open: {blocked}");
        assert!(!goal_reached(game.world()));
        let before = ContactScene::from_world(game.world()).unwrap();
        assert!(matches!(
            before
                .sweep(point(0.0, 0.35, -0.3), point(0.0, 0.0, 0.6), 0.1)
                .unwrap(),
            SweepOutcome::Hit(_)
        ));
        let source = game.world().sources()[0].id();
        let outer_before = game.world().organisms()[1].nodes().len();
        game.schedule(
            game.time().ticks() + 1,
            EnvironmentEventKind::MoveSource {
                id: source,
                position: point(2.4, 3.7, 0.2),
            },
        )
        .unwrap();
        game.schedule(
            game.time().ticks() + 1,
            EnvironmentEventKind::PruneBranch {
                organism: first,
                child,
            },
        )
        .unwrap();
        let saved = game.save_bytes().unwrap();
        assert!(matches!(
            game.pending_events()[0].kind,
            EnvironmentEventKind::MoveSource { .. }
        ));
        assert!(matches!(
            game.pending_events()[1].kind,
            EnvironmentEventKind::PruneBranch { .. }
        ));
        let mut replay = Runtime::load_bytes(&saved).unwrap();
        let mut reached = false;
        for _ in 0..100 {
            game.tick().unwrap();
            replay.tick().unwrap();
            if goal_reached(game.world()) {
                reached = true;
                break;
            }
        }
        assert_eq!(game, replay);
        assert!(game.world().organisms()[0].node(child).is_none());
        let after = ContactScene::from_world(game.world()).unwrap();
        assert_eq!(
            after
                .sweep(point(0.0, 0.35, -0.3), point(0.0, 0.0, 0.6), 0.1)
                .unwrap(),
            SweepOutcome::Miss
        );
        assert!(
            game.world().organisms()[1].nodes().len() > outer_before,
            "resource move did not grow outer organism: {outer_before} -> {}",
            game.world().organisms()[1].nodes().len()
        );
        assert!(game.world().body().unwrap().position().z() > blocked);
        assert!(
            reached,
            "final position: {:?}",
            game.world().body().unwrap().position()
        );
        assert_eq!(initial().unwrap(), start);
        assert!(!goal_reached(start.world()));
        println!(
            "{{\"scenario\":\"the-passage\",\"result\":\"success\",\"tick\":{},\"z\":{}}}",
            game.time().ticks(),
            game.world().body().unwrap().position().z()
        );
    }
}
