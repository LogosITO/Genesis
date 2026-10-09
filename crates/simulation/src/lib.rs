//! Fixed-step deterministic growth on one process and floating point model.

use serde::{Deserialize, Serialize};
use spatial_math::MathError;
use std::fmt;
use world_state::{WorldError, WorldState};

/// Simulation time as an integer count of completed steps.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationTime {
    ticks: u64,
}
impl SimulationTime {
    /// Completed steps.
    pub fn ticks(self) -> u64 {
        self.ticks
    }
    /// Restores a validated integer tick count.
    pub fn from_ticks(ticks: u64) -> Self {
        Self { ticks }
    }
}

/// Positive finite duration of a fixed step, in seconds.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationStep {
    seconds: f64,
}
impl SimulationStep {
    /// Constructs a fixed step.
    pub fn new(seconds: f64) -> Result<Self, MathError> {
        if !seconds.is_finite() {
            return Err(MathError::NonFinite);
        }
        if seconds <= 0.0 {
            return Err(MathError::NonPositive);
        }
        Ok(Self { seconds })
    }
    /// Duration in seconds.
    pub fn seconds(self) -> f64 {
        self.seconds
    }
}

/// Errors from a simulation step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationError {
    /// Radius computation was invalid or overflowed.
    Math(MathError),
    /// World query failed.
    World(WorldError),
    /// Tick counter exhausted.
    TimeOverflow,
    /// Events were not strictly ordered or exceeded the per-tick budget.
    InvalidEvents,
    /// Contact could not be safely classified or movement exceeded its numeric domain.
    Contact(contact::ContactError),
}
impl fmt::Display for SimulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SimulationError {}

/// Advances all sphere radii together. A failed step leaves world and time unchanged.
pub fn advance(
    world: &mut WorldState,
    time: &mut SimulationTime,
    step: SimulationStep,
) -> Result<(), SimulationError> {
    let next_tick = time
        .ticks
        .checked_add(1)
        .ok_or(SimulationError::TimeOverflow)?;
    let radii = world
        .entities()
        .iter()
        .map(|entity| {
            let radius = entity.sphere().radius() + entity.growth_per_second() * step.seconds;
            analytic_field::Sphere::new(radius).map_err(SimulationError::Math)
        })
        .collect::<Result<Vec<_>, _>>()?;
    world.apply_radii(&radii).map_err(SimulationError::World)?;
    time.ticks = next_tick;
    Ok(())
}

mod life;
pub use life::{EnvironmentEvent, EnvironmentEventKind, MAX_EVENTS_PER_TICK, advance_life};
pub mod contact;

#[cfg(test)]
mod tests {
    use super::*;
    use analytic_field::Sphere;
    use spatial_math::Transform;
    use world_state::DeterministicSeed;

    #[test]
    fn repeatable_growth_and_rollback() {
        let mut a = WorldState::new(DeterministicSeed(42));
        let id = a
            .spawn_sphere(Sphere::new(1.0).unwrap(), Transform::identity(), 0.25)
            .unwrap();
        let mut b = a.clone();
        let mut ta = SimulationTime::default();
        let mut tb = SimulationTime::default();
        let step = SimulationStep::new(0.5).unwrap();
        for _ in 0..8 {
            advance(&mut a, &mut ta, step).unwrap();
            advance(&mut b, &mut tb, step).unwrap();
        }
        assert_eq!(a, b);
        assert_eq!(ta, tb);
        assert_eq!(a.entity(id).unwrap().sphere().radius(), 2.0);
        assert_eq!(SimulationStep::new(0.0), Err(MathError::NonPositive));

        let mut failing = WorldState::new(DeterministicSeed(1));
        failing
            .spawn_sphere(Sphere::new(1.0).unwrap(), Transform::identity(), 0.25)
            .unwrap();
        failing
            .spawn_sphere(Sphere::new(1.0).unwrap(), Transform::identity(), 1e308)
            .unwrap();
        let original = failing.clone();
        let mut clock = SimulationTime::default();
        assert_eq!(
            advance(&mut failing, &mut clock, SimulationStep::new(2.0).unwrap()),
            Err(SimulationError::Math(MathError::NonFinite))
        );
        assert_eq!(failing, original);
        assert_eq!(clock.ticks(), 0);
    }
}
