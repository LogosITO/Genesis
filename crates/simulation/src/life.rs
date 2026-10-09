//! Tick-indexed environmental events and atomic bounded growth.

use crate::{SimulationError, SimulationStep, SimulationTime, advance};
use serde::{Deserialize, Serialize};
use spatial_math::Vec3;
use world_state::{EntityId, WorldState};

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
        }
    }
    let sources = proposed.sources().to_vec();
    for organism in proposed.organisms_mut() {
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
            for source in &sources {
                let sample = source
                    .sample(node.position())
                    .map_err(SimulationError::World)?;
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
        organism.grow(&samples).map_err(SimulationError::World)?;
    }
    let mut next_time = *time;
    advance(&mut proposed, &mut next_time, step)?;
    *world = proposed;
    *time = next_time;
    Ok(())
}
