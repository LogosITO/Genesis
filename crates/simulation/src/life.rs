//! Tick-indexed environmental events and atomic bounded growth.

use crate::{SimulationError, SimulationStep, SimulationTime, advance};
use serde::{Deserialize, Serialize};
use spatial_math::Vec3;
use world_state::{EntityId, WorldState};

/// Maximum changes applied on one tick.
pub const MAX_EVENTS_PER_TICK: usize = 16;

/// A typed change to the continuous resource environment.
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
}

/// An event for a completed-tick index, ordered by its stable insertion sequence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentEvent {
    /// Tick that applies this event; first simulation step is tick 1.
    pub tick: u64,
    /// Monotonically allocated order among all queued events.
    pub order: u64,
    /// Environmental mutation.
    pub kind: EnvironmentEventKind,
}

/// Applies events, samples the previous tips in the changed environment, proposes one child
/// per organism, then commits world and time together. Any failure leaves both unchanged.
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
        }
    }
    let sources = proposed.sources().to_vec();
    for organism in proposed.organisms_mut() {
        let tip = organism
            .nodes()
            .last()
            .expect("validated organism has a root")
            .position();
        let mut concentration = 0.0;
        let mut strongest = None;
        let mut strongest_sample = 0.0;
        for source in &sources {
            let sample = source.sample(tip).map_err(SimulationError::World)?;
            concentration += sample;
            if sample > strongest_sample {
                strongest_sample = sample;
                strongest = Some(source.position());
            }
        }
        let direction = if let Some(position) = strongest {
            let delta = position
                .checked_sub(tip)
                .map_err(|error| SimulationError::World(error.into()))?;
            Vec3::new(delta.x(), 1.0, delta.z())
                .map_err(|error| SimulationError::World(error.into()))?
        } else {
            Vec3::new(0.0, 1.0, 0.0).expect("finite axis")
        };
        let gained = concentration * organism.parameters().uptake() * step.seconds();
        organism
            .grow(gained, direction)
            .map_err(SimulationError::World)?;
    }
    let mut next_time = *time;
    advance(&mut proposed, &mut next_time, step)?;
    *world = proposed;
    *time = next_time;
    Ok(())
}
