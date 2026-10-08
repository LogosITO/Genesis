//! Small embeddable coordinator; no graphics or UI dependency.

use serde::{Deserialize, Serialize};
use world_simulation::{
    EnvironmentEvent, EnvironmentEventKind, MAX_EVENTS_PER_TICK, SimulationError, SimulationStep,
    SimulationTime, advance_life,
};
use world_state::WorldState;

mod persistence;
pub use persistence::{MAX_SAVE_BYTES, PersistenceError};

/// Maximum scheduled but unapplied events in a runtime.
pub const MAX_PENDING_EVENTS: usize = 128;

/// Errors from scheduling a deterministic environmental event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleError {
    /// The tick is already completed or invalid.
    PastTick,
    /// An event or sequence budget was exhausted.
    Capacity,
    /// Source identity or position is invalid.
    InvalidSource,
}
impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ScheduleError {}

/// Owns a world and its fixed-step clock.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    world: WorldState,
    time: SimulationTime,
    step: SimulationStep,
    events: Vec<EnvironmentEvent>,
    next_order: u64,
}
impl Runtime {
    /// Creates a runtime from a world and fixed step.
    pub fn new(world: WorldState, step: SimulationStep) -> Self {
        Self {
            world,
            time: SimulationTime::default(),
            step,
            events: Vec::new(),
            next_order: 0,
        }
    }
    /// Advances exactly one fixed step.
    pub fn tick(&mut self) -> Result<(), SimulationError> {
        let tick = self
            .time
            .ticks()
            .checked_add(1)
            .ok_or(SimulationError::TimeOverflow)?;
        let applicable: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.tick == tick)
            .cloned()
            .collect();
        advance_life(&mut self.world, &mut self.time, self.step, &applicable)?;
        self.events.retain(|e| e.tick > tick);
        Ok(())
    }
    /// Schedules an event at a future completed-tick index. Insertion order is stable.
    pub fn schedule(&mut self, tick: u64, kind: EnvironmentEventKind) -> Result<(), ScheduleError> {
        if tick <= self.time.ticks() {
            return Err(ScheduleError::PastTick);
        }
        if self.events.len() == MAX_PENDING_EVENTS
            || self.events.iter().filter(|e| e.tick == tick).count() == MAX_EVENTS_PER_TICK
        {
            return Err(ScheduleError::Capacity);
        }
        let next = self
            .next_order
            .checked_add(1)
            .ok_or(ScheduleError::Capacity)?;
        let mut probe = self.world.clone();
        match &kind {
            EnvironmentEventKind::MoveSource { id, position } => probe
                .source_mut(*id)
                .and_then(|s| s.move_to(*position))
                .map_err(|_| ScheduleError::InvalidSource)?,
            EnvironmentEventKind::SetSourceActive { id, .. } => {
                probe
                    .source_mut(*id)
                    .map_err(|_| ScheduleError::InvalidSource)?;
            }
        }
        self.events.push(EnvironmentEvent {
            tick,
            order: self.next_order,
            kind,
        });
        self.next_order = next;
        Ok(())
    }
    /// Current immutable world state.
    pub fn world(&self) -> &WorldState {
        &self.world
    }
    /// Completed simulation steps.
    pub fn time(&self) -> SimulationTime {
        self.time
    }
    /// Fixed simulation step.
    pub fn step(&self) -> SimulationStep {
        self.step
    }
    /// Future events in stable insertion order.
    pub fn pending_events(&self) -> &[EnvironmentEvent] {
        &self.events
    }
    fn validate(&self) -> Result<(), PersistenceError> {
        self.world
            .validate()
            .map_err(|_| PersistenceError::InvalidState("world"))?;
        SimulationStep::new(self.step.seconds())
            .map_err(|_| PersistenceError::InvalidState("step"))?;
        if self.events.len() > MAX_PENDING_EVENTS {
            return Err(PersistenceError::InvalidState("event budget"));
        }
        let mut previous_order = None;
        for event in &self.events {
            if event.tick <= self.time.ticks()
                || event.order >= self.next_order
                || previous_order.is_some_and(|old| event.order <= old)
                || self
                    .events
                    .iter()
                    .filter(|other| other.tick == event.tick)
                    .count()
                    > MAX_EVENTS_PER_TICK
            {
                return Err(PersistenceError::InvalidState("event ordering"));
            }
            let mut probe = self.world.clone();
            match event.kind {
                EnvironmentEventKind::MoveSource { id, position } => probe
                    .source_mut(id)
                    .and_then(|s| s.move_to(position))
                    .map_err(|_| PersistenceError::InvalidState("event source"))?,
                EnvironmentEventKind::SetSourceActive { id, .. } => {
                    probe
                        .source_mut(id)
                        .map_err(|_| PersistenceError::InvalidState("event source"))?;
                }
            }
            previous_order = Some(event.order);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spatial_math::Vec3;
    use world_state::{DeterministicSeed, GrowthParameters};

    fn fixture() -> (Runtime, world_state::EntityId) {
        let mut world = WorldState::new(DeterministicSeed(9));
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.3, 0.1, 1.0).unwrap(),
            )
            .unwrap();
        let source = world
            .spawn_source(Vec3::new(0.0, 1.0, 0.0).unwrap(), 3.0, 1.0)
            .unwrap();
        (
            Runtime::new(world, SimulationStep::new(0.1).unwrap()),
            source,
        )
    }

    #[test]
    fn event_order_and_failure_are_atomic() {
        let (mut runtime, source) = fixture();
        runtime
            .schedule(
                1,
                EnvironmentEventKind::MoveSource {
                    id: source,
                    position: Vec3::new(1.0, 1.0, 0.0).unwrap(),
                },
            )
            .unwrap();
        runtime
            .schedule(
                1,
                EnvironmentEventKind::MoveSource {
                    id: source,
                    position: Vec3::new(2.0, 1.0, 0.0).unwrap(),
                },
            )
            .unwrap();
        runtime.tick().unwrap();
        assert_eq!(
            runtime.world.sources()[0].position(),
            Vec3::new(2.0, 1.0, 0.0).unwrap()
        );
        assert!(runtime.pending_events().is_empty());
        assert_eq!(runtime.world.organisms()[0].nodes().len(), 1);
        let before = runtime.clone();
        let invalid = EnvironmentEvent {
            tick: 2,
            order: 0,
            kind: EnvironmentEventKind::MoveSource {
                id: source,
                position: Vec3::new(1001.0, 0.0, 0.0).unwrap(),
            },
        };
        assert!(
            advance_life(
                &mut runtime.world,
                &mut runtime.time,
                runtime.step,
                &[invalid]
            )
            .is_err()
        );
        assert_eq!(runtime, before);
        assert_eq!(
            runtime.schedule(
                1,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false
                }
            ),
            Err(ScheduleError::PastTick)
        );
    }

    #[test]
    fn malformed_saves_and_load_into_leave_state_unchanged() {
        let (mut runtime, source) = fixture();
        runtime
            .schedule(
                4,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false,
                },
            )
            .unwrap();
        let good = runtime.save_bytes().unwrap();
        let original = runtime.clone();
        let mut value: serde_json::Value = serde_json::from_slice(&good).unwrap();
        value["format_version"] = 2.into();
        assert!(matches!(
            runtime.load_into(&serde_json::to_vec(&value).unwrap()),
            Err(PersistenceError::Version(2))
        ));
        value["format_version"] = 1.into();
        value["runtime"]["world"]["organisms"][0]["nodes"][0]["parent"] = 9.into();
        assert!(matches!(
            runtime.load_into(&serde_json::to_vec(&value).unwrap()),
            Err(PersistenceError::InvalidState("world"))
        ));
        assert_eq!(runtime, original);
        assert!(matches!(
            Runtime::load_bytes(&vec![b'x'; MAX_SAVE_BYTES + 1]),
            Err(PersistenceError::TooLarge)
        ));
        assert!(Runtime::load_bytes(b"{").is_err());
    }

    #[test]
    fn save_new_does_not_overwrite_valid_file() {
        let (runtime, _) = fixture();
        let path = std::env::temp_dir().join(format!(
            "genesis-life-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        runtime.save_new(&path).unwrap();
        assert_eq!(Runtime::load_file(&path).unwrap(), runtime);
        assert!(runtime.save_new(&path).is_err());
        assert_eq!(Runtime::load_file(&path).unwrap(), runtime);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn event_and_work_budgets_are_explicit() {
        let (mut runtime, source) = fixture();
        for _ in 0..MAX_EVENTS_PER_TICK {
            runtime
                .schedule(
                    1,
                    EnvironmentEventKind::SetSourceActive {
                        id: source,
                        active: true,
                    },
                )
                .unwrap();
        }
        assert_eq!(
            runtime.schedule(
                1,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: true
                }
            ),
            Err(ScheduleError::Capacity)
        );
        for tick in 2..=7 {
            for _ in 0..MAX_EVENTS_PER_TICK {
                runtime
                    .schedule(
                        tick,
                        EnvironmentEventKind::SetSourceActive {
                            id: source,
                            active: true,
                        },
                    )
                    .unwrap();
            }
        }
        assert_eq!(runtime.pending_events().len(), 112);
        for _ in 0..MAX_EVENTS_PER_TICK {
            runtime
                .schedule(
                    8,
                    EnvironmentEventKind::SetSourceActive {
                        id: source,
                        active: true,
                    },
                )
                .unwrap();
        }
        assert_eq!(
            runtime.schedule(
                9,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: true
                }
            ),
            Err(ScheduleError::Capacity)
        );
    }

    #[test]
    fn deactivating_source_stops_subsequent_growth() {
        let (mut active, source) = fixture();
        let mut inactive = active.clone();
        inactive
            .schedule(
                1,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false,
                },
            )
            .unwrap();
        for _ in 0..10 {
            active.tick().unwrap();
            inactive.tick().unwrap();
        }
        assert!(active.world().organisms()[0].nodes().len() > 1);
        assert_eq!(inactive.world().organisms()[0].nodes().len(), 1);
        assert!(!inactive.world().sources()[0].active());
    }
}
