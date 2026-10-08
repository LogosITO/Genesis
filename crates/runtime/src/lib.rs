//! Small embeddable coordinator; no graphics or UI dependency.

use world_simulation::{SimulationError, SimulationStep, SimulationTime, advance};
use world_state::WorldState;

/// Owns a world and its fixed-step clock.
pub struct Runtime {
    world: WorldState,
    time: SimulationTime,
    step: SimulationStep,
}
impl Runtime {
    /// Creates a runtime from a world and fixed step.
    pub fn new(world: WorldState, step: SimulationStep) -> Self {
        Self {
            world,
            time: SimulationTime::default(),
            step,
        }
    }
    /// Advances exactly one fixed step.
    pub fn tick(&mut self) -> Result<(), SimulationError> {
        advance(&mut self.world, &mut self.time, self.step)
    }
    /// Current immutable world state.
    pub fn world(&self) -> &WorldState {
        &self.world
    }
    /// Completed simulation steps.
    pub fn time(&self) -> SimulationTime {
        self.time
    }
}
