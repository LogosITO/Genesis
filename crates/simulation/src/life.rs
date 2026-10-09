//! Tick-indexed environmental events and atomic bounded growth.

use crate::contact::{ContactScene, SweepOutcome};
use crate::{SimulationError, SimulationStep, SimulationTime, advance};
use serde::{Deserialize, Serialize};
use spatial_math::Vec3;
use world_state::{EntityId, MAX_NODES, WorldState};

struct Allocation {
    ratios: Vec<Vec<f64>>,
    totals: Vec<f64>,
}

// Each source allocates once across organisms. Demands are each organism's largest eligible
// tip request from that source; node maturity is a signal, while the shared budget stores units.
fn allocate_finite(
    world: &mut WorldState,
    step: SimulationStep,
) -> Result<Option<Allocation>, SimulationError> {
    if !world
        .sources()
        .iter()
        .any(|source| source.reservoir().is_some())
    {
        return Ok(None);
    }
    if world
        .sources()
        .iter()
        .any(|source| source.reservoir().is_none())
    {
        return Err(SimulationError::World(
            world_state::WorldError::InvalidStructure,
        ));
    }
    let sources = world.sources().to_vec();
    let mut raw = vec![vec![0.0_f64; sources.len()]; world.organisms().len()];
    for (oi, organism) in world.organisms().iter().enumerate() {
        let nodes = organism.nodes();
        let mut children = vec![0u8; nodes.len()];
        for node in &nodes[1..] {
            let parent = nodes
                .iter()
                .position(|candidate| candidate.id() == node.parent().expect("validated tree"))
                .expect("validated parent");
            children[parent] += 1;
        }
        for (ni, node) in nodes.iter().enumerate() {
            if children[ni] >= organism.parameters().max_children() {
                continue;
            }
            for (si, source) in sources.iter().enumerate() {
                let request = source
                    .sample(node.position())
                    .map_err(SimulationError::World)?
                    * organism.parameters().uptake()
                    * step.seconds();
                if !request.is_finite() {
                    return Err(SimulationError::Math(spatial_math::MathError::NonFinite));
                }
                raw[oi][si] = raw[oi][si].max(request);
            }
        }
    }
    let mut demands = raw.clone();
    for (oi, organism) in world.organisms().iter().enumerate() {
        let sum: f64 = raw[oi].iter().sum();
        if !sum.is_finite() {
            return Err(SimulationError::Math(spatial_math::MathError::NonFinite));
        }
        let headroom = organism.parameters().threshold() * MAX_NODES as f64 - organism.budget();
        let scale = if sum > headroom { headroom / sum } else { 1.0 };
        for demand in &mut demands[oi] {
            *demand *= scale;
        }
    }
    let mut ratios = demands.clone();
    let mut totals = vec![0.0; world.organisms().len()];
    for (si, source) in sources.iter().enumerate() {
        let available = source.reservoir().expect("finite source").stored();
        let total: f64 = demands.iter().map(|row| row[si]).sum();
        if !total.is_finite() {
            return Err(SimulationError::Math(spatial_math::MathError::NonFinite));
        }
        let factor = if total > available {
            available / total
        } else {
            1.0
        };
        let mut remaining = available;
        for oi in 0..world.organisms().len() {
            let allocated = (demands[oi][si] * factor).min(remaining);
            remaining -= allocated;
            world.sources_mut()[si]
                .allocate(allocated)
                .map_err(SimulationError::World)?;
            totals[oi] += allocated;
            ratios[oi][si] = if raw[oi][si] > 0.0 {
                allocated / raw[oi][si]
            } else {
                0.0
            };
        }
    }
    Ok(Some(Allocation { ratios, totals }))
}

/// Maximum changes applied on one tick.
pub const MAX_EVENTS_PER_TICK: usize = 16;

/// A typed world change applied at a fixed tick.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum EnvironmentEventKind {
    /// Move the source before resource evaluation on the event tick.
    MoveSource {
        /// Source identity.
        id: EntityId,
        /// New world position.
        position: Vec3,
    },
    /// Enable or disable a source before resource evaluation.
    SetSourceActive {
        /// Source identity.
        id: EntityId,
        /// New active state.
        active: bool,
    },
    /// Remove the child connection and its descendant subtree before resource sampling.
    PruneBranch {
        /// Stable organism identity.
        organism: EntityId,
        /// Stable non-root child node ID; the connection is identified by this child.
        child: u32,
    },
    /// Set the sole kinematic body's desired velocity before growth and movement.
    SetBodyVelocity {
        /// Stable body identity.
        id: EntityId,
        /// World units per simulated second.
        velocity: Vec3,
    },
}

/// An event for a completed-tick index, ordered by its stable insertion sequence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentEvent {
    /// Tick that applies this event; first simulation step is tick 1.
    pub tick: u64,
    /// Monotonically allocated order among all queued events.
    pub order: u64,
    /// Authoritative world mutation.
    pub kind: EnvironmentEventKind,
}

/// Applies events, samples all unsaturated nodes in the changed environment, then resolves
/// mature growth requests in node-ID order. Any failure leaves world and time unchanged.
pub fn advance_life(
    world: &mut WorldState,
    time: &mut SimulationTime,
    step: SimulationStep,
    events: &[EnvironmentEvent],
) -> Result<(), SimulationError> {
    advance_life_cached(world, time, step, events, &mut None)
}

/// Advances one tick while reusing a derived contact scene when its analytic solids are unchanged.
/// The cache is disposable and must not be serialized as authoritative world state.
/// A failed tick leaves world and time unchanged; the cache may be refreshed and is compared again
/// before its next use.
pub fn advance_life_cached(
    world: &mut WorldState,
    time: &mut SimulationTime,
    step: SimulationStep,
    events: &[EnvironmentEvent],
    contact_cache: &mut Option<ContactScene>,
) -> Result<(), SimulationError> {
    let next_tick = time
        .ticks()
        .checked_add(1)
        .ok_or(SimulationError::TimeOverflow)?;
    if events.len() > MAX_EVENTS_PER_TICK
        || events.iter().any(|e| e.tick != next_tick)
        || events.windows(2).any(|pair| pair[0].order >= pair[1].order)
    {
        return Err(SimulationError::InvalidEvents);
    }
    let mut proposed = world.clone();
    for event in events {
        match event.kind {
            EnvironmentEventKind::MoveSource { id, position } => proposed
                .source_mut(id)
                .map_err(SimulationError::World)?
                .move_to(position)
                .map_err(SimulationError::World)?,
            EnvironmentEventKind::SetSourceActive { id, active } => proposed
                .source_mut(id)
                .map_err(SimulationError::World)?
                .set_active(active),
            EnvironmentEventKind::PruneBranch { organism, child } => {
                proposed
                    .prune_branch(organism, child)
                    .map_err(SimulationError::World)?;
            }
            EnvironmentEventKind::SetBodyVelocity { id, velocity } => proposed
                .body_mut(id)
                .map_err(SimulationError::World)?
                .set_desired_velocity(velocity)
                .map_err(SimulationError::World)?,
        }
    }
    for source in proposed.sources_mut() {
        source.begin_tick().map_err(SimulationError::World)?;
    }
    let allocation = allocate_finite(&mut proposed, step)?;
    let sources = proposed.sources().to_vec();
    for (organism_index, organism) in proposed.organisms_mut().iter_mut().enumerate() {
        let nodes = organism.nodes();
        let mut children = vec![0u8; nodes.len()];
        for node in &nodes[1..] {
            let parent = nodes
                .iter()
                .position(|candidate| candidate.id() == node.parent().expect("validated tree"))
                .expect("validated parent");
            children[parent] += 1;
        }
        let mut samples = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            if children[index] >= organism.parameters().max_children() {
                samples.push((0.0, Vec3::new(0.0, 1.0, 0.0).expect("axis")));
                continue;
            }
            let mut concentration = 0.0;
            let mut strongest = None;
            let mut strongest_sample = 0.0;
            for (source_index, source) in sources.iter().enumerate() {
                let mut sample = source
                    .sample(node.position())
                    .map_err(SimulationError::World)?;
                if let Some(allocation) = &allocation {
                    sample *= allocation.ratios[organism_index][source_index];
                }
                concentration += sample;
                if sample > strongest_sample {
                    strongest_sample = sample;
                    strongest = Some(source.position());
                }
            }
            let parent_direction = if let Some(parent) = node.parent() {
                node.position()
                    .checked_sub(
                        nodes
                            .iter()
                            .find(|candidate| candidate.id() == parent)
                            .expect("validated parent")
                            .position(),
                    )
                    .map_err(|error| SimulationError::World(error.into()))?
                    .normalized()
                    .map_err(|error| SimulationError::World(error.into()))?
            } else {
                Vec3::new(0.0, 1.0, 0.0).expect("axis")
            };
            let toward = if let Some(position) = strongest {
                position
                    .checked_sub(node.position())
                    .map_err(|error| SimulationError::World(error.into()))?
            } else {
                Vec3::ZERO
            };
            // A second child diverges from the first; both still follow the local resource.
            let divergence = if children[index] == 1 { -0.65 } else { 0.0 };
            let direction = Vec3::new(
                parent_direction.x() * 0.4 + toward.x() * 0.35 + divergence,
                1.0,
                parent_direction.z() * 0.4 + toward.z() * 0.35,
            )
            .map_err(|error| SimulationError::World(error.into()))?;
            samples.push((
                concentration * organism.parameters().uptake() * step.seconds(),
                direction,
            ));
        }
        if let Some(allocation) = &allocation {
            organism
                .grow_allocated(&samples, allocation.totals[organism_index])
                .map_err(SimulationError::World)?;
        } else {
            organism.grow(&samples).map_err(SimulationError::World)?;
        }
    }
    let mut next_time = *time;
    advance(&mut proposed, &mut next_time, step)?;
    if let Some(body) = proposed.body().cloned() {
        let movement = body
            .desired_velocity()
            .checked_scale(step.seconds())
            .map_err(SimulationError::Math)?;
        let scene = match contact_cache {
            Some(scene) => {
                scene.refresh(&proposed).map_err(SimulationError::Contact)?;
                scene
            }
            None => contact_cache
                .insert(ContactScene::from_world(&proposed).map_err(SimulationError::Contact)?),
        };
        let outcome = scene
            .sweep(body.position(), movement, body.radius())
            .map_err(SimulationError::Contact)?;
        let (fraction, contact) = match outcome {
            SweepOutcome::Hit(hit) => {
                let length = movement.length().map_err(SimulationError::Math)?;
                let margin = if length == 0.0 {
                    0.0
                } else {
                    (1e-9 / length).min(hit.fraction)
                };
                (hit.fraction - margin, Some(hit))
            }
            SweepOutcome::Miss => (1.0, None),
            SweepOutcome::Indeterminate => {
                return Err(SimulationError::Contact(
                    crate::contact::ContactError::Indeterminate,
                ));
            }
        };
        let position = body
            .position()
            .checked_add(
                movement
                    .checked_scale(fraction)
                    .map_err(SimulationError::Math)?,
            )
            .map_err(SimulationError::Math)?;
        proposed
            .body_mut(body.id())
            .map_err(SimulationError::World)?
            .apply_motion(position, contact)
            .map_err(SimulationError::World)?;
    }
    proposed.validate().map_err(SimulationError::World)?;
    *world = proposed;
    *time = next_time;
    Ok(())
}

#[cfg(test)]
mod ecology_tests {
    use super::*;
    use world_state::{DeterministicSeed, GrowthParameters, OrganismLifecycle};

    fn fixture(stored: f64, refill: f64) -> WorldState {
        let mut world = WorldState::new(DeterministicSeed(8));
        let parameters = GrowthParameters::new(0.1, 0.3, 0.5, 1.0).unwrap();
        for x in [-0.6, 0.6] {
            world
                .spawn_organism(Vec3::new(x, 0.0, 0.0).unwrap(), parameters)
                .unwrap();
        }
        world
            .spawn_finite_source(
                Vec3::new(0.0, 2.0, 0.0).unwrap(),
                4.0,
                10.0,
                stored,
                0.2,
                refill,
            )
            .unwrap();
        world
    }

    #[test]
    fn proportional_competition_withdraws_at_most_one_finite_budget() {
        let mut world = fixture(0.2, 0.0);
        let mut time = SimulationTime::default();
        advance_life(
            &mut world,
            &mut time,
            SimulationStep::new(0.1).unwrap(),
            &[],
        )
        .unwrap();
        let stock = world.sources()[0].reservoir().unwrap();
        assert!((stock.last_allocated() - 0.2).abs() < 1e-12);
        assert!(stock.stored() >= 0.0);
        assert!((world.organisms()[0].budget() - 0.1).abs() < 1e-12);
        assert_eq!(world.organisms()[0].budget(), world.organisms()[1].budget());
        assert!(
            world
                .organisms()
                .iter()
                .all(|organism| organism.lifecycle_state() == OrganismLifecycle::Active)
        );
        assert!(
            (world.organisms().iter().map(|o| o.budget()).sum::<f64>() - stock.last_allocated())
                .abs()
                < 1e-12
        );
        assert_eq!(time.ticks(), 1);
    }

    #[test]
    fn zero_resource_stops_growth_and_invalid_step_is_atomic() {
        let mut world = fixture(0.0, 0.0);
        let mut time = SimulationTime::default();
        for _ in 0..10 {
            advance_life(
                &mut world,
                &mut time,
                SimulationStep::new(0.1).unwrap(),
                &[],
            )
            .unwrap();
        }
        assert!(
            world
                .organisms()
                .iter()
                .all(|organism| organism.nodes().len() == 1)
        );
        let before = world.clone();
        let previous_time = time;
        assert!(
            advance_life(
                &mut world,
                &mut time,
                SimulationStep::new(f64::MAX).unwrap(),
                &[]
            )
            .is_err()
        );
        assert_eq!(world, before);
        assert_eq!(time, previous_time);
    }

    #[test]
    fn unequal_demands_receive_proportional_stable_shares() {
        let mut world = WorldState::new(DeterministicSeed(9));
        let parameters = GrowthParameters::new(0.1, 0.3, 0.5, 1.0).unwrap();
        world.spawn_organism(Vec3::ZERO, parameters).unwrap();
        world
            .spawn_organism(Vec3::new(2.0, 0.0, 0.0).unwrap(), parameters)
            .unwrap();
        world
            .spawn_finite_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 10.0, 0.2, 0.2, 0.0)
            .unwrap();
        let mut repeated = world.clone();
        let mut time = SimulationTime::default();
        let mut repeated_time = time;
        let step = SimulationStep::new(0.1).unwrap();
        advance_life(&mut world, &mut time, step, &[]).unwrap();
        advance_life(&mut repeated, &mut repeated_time, step, &[]).unwrap();
        assert_eq!(world, repeated);
        let source = &world.sources()[0];
        let a = source.sample(Vec3::ZERO).unwrap();
        let b = source.sample(Vec3::new(2.0, 0.0, 0.0).unwrap()).unwrap();
        assert!((world.organisms()[0].budget() - 0.2 * a / (a + b)).abs() < 1e-12);
        assert!((world.organisms()[1].budget() - 0.2 * b / (a + b)).abs() < 1e-12);
        assert!(world.organisms()[0].budget() > world.organisms()[1].budget());
    }

    #[test]
    #[ignore = "local release-profile phase timing; run serially and explicitly"]
    fn finite_allocation_and_growth_commit_benchmark() {
        use std::time::Instant;
        for count in [1usize, 2, 4] {
            let mut world = WorldState::new(DeterministicSeed(41));
            let parameters = GrowthParameters::new(0.1, 0.3, 0.01, 1.0).unwrap();
            for index in 0..count {
                world
                    .spawn_organism(
                        Vec3::new(index as f64 * 1.2 - (count - 1) as f64 * 0.6, 0.0, 0.0).unwrap(),
                        parameters,
                    )
                    .unwrap();
            }
            world
                .spawn_finite_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 10.0, 0.0, 0.2, 0.2)
                .unwrap();
            let mut allocation_ms = Vec::new();
            let mut growth_ms = Vec::new();
            for _ in 0..50 {
                let mut copy = world.clone();
                copy.sources_mut()[0].begin_tick().unwrap();
                let started = Instant::now();
                let allocation = allocate_finite(&mut copy, SimulationStep::new(0.1).unwrap())
                    .unwrap()
                    .unwrap();
                allocation_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                let source = copy.sources()[0].clone();
                let started = Instant::now();
                for (index, organism) in copy.organisms_mut().iter_mut().enumerate() {
                    let gain = source.sample(organism.root()).unwrap()
                        * allocation.ratios[index][0]
                        * organism.parameters().uptake()
                        * 0.1;
                    organism
                        .grow_allocated(
                            &[(gain, Vec3::new(0.0, 1.0, 0.0).unwrap())],
                            allocation.totals[index],
                        )
                        .unwrap();
                }
                growth_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                assert!(
                    copy.organisms()
                        .iter()
                        .all(|organism| organism.nodes().len() == 2)
                );
            }
            allocation_ms.sort_by(f64::total_cmp);
            growth_ms.sort_by(f64::total_cmp);
            println!(
                "organisms={count} allocation_demand_solver_ms={:.4}/{:.4} growth_commit_ms={:.4}/{:.4}",
                allocation_ms[24], allocation_ms[47], growth_ms[24], growth_ms[47]
            );
        }
    }
}
