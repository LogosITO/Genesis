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
    /// Growth target is absent, the root, or conflicts with an already queued cut.
    InvalidTarget,
    /// Body identity or requested velocity is invalid.
    InvalidBody,
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
            EnvironmentEventKind::PruneBranch { organism, child } => {
                let tree = probe
                    .organisms()
                    .iter()
                    .find(|tree| tree.id() == *organism)
                    .ok_or(ScheduleError::InvalidTarget)?;
                if self.events.iter().any(|event| match event.kind {
                    EnvironmentEventKind::PruneBranch {
                        organism: other,
                        child: other_child,
                    } if other == *organism => {
                        tree.contains_branch(*child, other_child)
                            || tree.contains_branch(other_child, *child)
                    }
                    _ => false,
                }) {
                    return Err(ScheduleError::InvalidTarget);
                }
                probe
                    .prune_branch(*organism, *child)
                    .map_err(|_| ScheduleError::InvalidTarget)?;
            }
            EnvironmentEventKind::SetBodyVelocity { id, velocity } => probe
                .body_mut(*id)
                .and_then(|body| body.set_desired_velocity(*velocity))
                .map_err(|_| ScheduleError::InvalidBody)?,
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
                EnvironmentEventKind::PruneBranch { organism, child } => {
                    let tree = probe
                        .organisms()
                        .iter()
                        .find(|tree| tree.id() == organism)
                        .ok_or(PersistenceError::InvalidState("event target"))?;
                    if self.events.iter().any(|prior| {
                        prior.order < event.order
                            && match prior.kind {
                                EnvironmentEventKind::PruneBranch {
                                    organism: other,
                                    child: other_child,
                                } if other == organism => {
                                    tree.contains_branch(child, other_child)
                                        || tree.contains_branch(other_child, child)
                                }
                                _ => false,
                            }
                    }) {
                        return Err(PersistenceError::InvalidState("event target"));
                    }
                    probe
                        .prune_branch(organism, child)
                        .map_err(|_| PersistenceError::InvalidState("event target"))?;
                }
                EnvironmentEventKind::SetBodyVelocity { id, velocity } => probe
                    .body_mut(id)
                    .and_then(|body| body.set_desired_velocity(velocity))
                    .map_err(|_| PersistenceError::InvalidState("event body"))?,
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
        value["format_version"] = 6.into();
        assert!(matches!(
            runtime.load_into(&serde_json::to_vec(&value).unwrap()),
            Err(PersistenceError::Version(6))
        ));
        value["format_version"] = 1.into();
        value["runtime"]["world"]["organisms"][0]
            .as_object_mut()
            .unwrap()
            .remove("budget");
        value["runtime"]["world"]["organisms"][0]
            .as_object_mut()
            .unwrap()
            .remove("next_node_id");
        value["runtime"]["world"]["organisms"][0]["parameters"]
            .as_object_mut()
            .unwrap()
            .remove("max_children");
        assert_eq!(
            Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()).unwrap(),
            runtime
        );
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
    fn malformed_branch_topology_and_budget_are_rejected() {
        let (mut runtime, _) = fixture();
        for _ in 0..4 {
            runtime.tick().unwrap();
        }
        assert!(runtime.world().organisms()[0].nodes().len() >= 3);
        let base: serde_json::Value =
            serde_json::from_slice(&runtime.save_bytes().unwrap()).unwrap();
        for (path, bad) in [
            ("parent", serde_json::json!(2)),
            ("id", serde_json::json!(0)),
        ] {
            let mut value = base.clone();
            value["runtime"]["world"]["organisms"][0]["nodes"][1][path] = bad;
            assert!(matches!(
                Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()),
                Err(PersistenceError::InvalidState("world"))
            ));
        }
        for (key, bad) in [
            ("budget", serde_json::json!(-1.0)),
            (
                "parameters",
                serde_json::json!({"node_radius":0.1,"segment_length":0.3,"threshold":0.1,"uptake":1.0,"max_children":1}),
            ),
        ] {
            let mut value = base.clone();
            value["runtime"]["world"]["organisms"][0][key] = bad;
            assert!(matches!(
                Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()),
                Err(PersistenceError::InvalidState("world"))
            ));
        }
    }

    #[test]
    fn prune_events_are_atomic_persistent_and_replayable() {
        let (mut runtime, _) = fixture();
        for _ in 0..5 {
            runtime.tick().unwrap();
        }
        let organism = runtime.world().organisms()[0].id();
        assert!(runtime.world().organisms()[0].node(1).is_some());
        let kind = EnvironmentEventKind::PruneBranch { organism, child: 1 };
        assert_eq!(
            runtime.schedule(10, EnvironmentEventKind::PruneBranch { organism, child: 0 }),
            Err(ScheduleError::InvalidTarget)
        );
        runtime.schedule(10, kind.clone()).unwrap();
        assert_eq!(
            runtime.schedule(11, kind.clone()),
            Err(ScheduleError::InvalidTarget)
        );
        let bytes = runtime.save_bytes().unwrap();
        let mut malformed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        malformed["runtime"]["events"][0]["kind"]["PruneBranch"]["child"] = 999.into();
        assert!(matches!(
            Runtime::load_bytes(&serde_json::to_vec(&malformed).unwrap()),
            Err(PersistenceError::InvalidState("event target"))
        ));
        let mut loaded = Runtime::load_bytes(&bytes).unwrap();
        assert_eq!(loaded.pending_events(), runtime.pending_events());
        while runtime.time().ticks() < 12 {
            runtime.tick().unwrap();
            loaded.tick().unwrap();
        }
        assert_eq!(runtime, loaded);
        assert!(runtime.world().organisms()[0].node(1).is_none());
        assert!(runtime.pending_events().is_empty());
        assert_eq!(
            runtime.schedule(13, kind.clone()),
            Err(ScheduleError::InvalidTarget)
        );
        assert_eq!(
            Runtime::load_bytes(&runtime.save_bytes().unwrap()).unwrap(),
            runtime
        );

        let (mut another, _) = fixture();
        for _ in 0..5 {
            another.tick().unwrap();
        }
        let before = another.clone();
        let events = [
            EnvironmentEvent {
                tick: 6,
                order: 0,
                kind: kind.clone(),
            },
            EnvironmentEvent {
                tick: 6,
                order: 1,
                kind,
            },
        ];
        assert!(
            advance_life(&mut another.world, &mut another.time, another.step, &events).is_err()
        );
        assert_eq!(another, before);
    }

    #[test]
    fn legacy_dense_allocators_migrate_but_v3_requires_allocator() {
        let (mut runtime, _) = fixture();
        for _ in 0..5 {
            runtime.tick().unwrap();
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(&runtime.save_bytes().unwrap()).unwrap();
        value["runtime"]["world"]["organisms"][0]
            .as_object_mut()
            .unwrap()
            .remove("next_node_id");
        assert!(matches!(
            Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()),
            Err(PersistenceError::InvalidState("world"))
        ));
        value["format_version"] = 2.into();
        assert_eq!(
            Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()).unwrap(),
            runtime
        );
    }

    #[test]
    fn body_events_save_resume_validation_and_atomic_failure() {
        let (mut runtime, source) = fixture();
        let body = runtime
            .world
            .spawn_body(Vec3::new(0.12, 0.3, -1.0).unwrap(), 0.1)
            .unwrap();
        let move_event = EnvironmentEventKind::SetBodyVelocity {
            id: body,
            velocity: Vec3::new(0.0, 0.0, 2.0).unwrap(),
        };
        assert_eq!(
            runtime.schedule(
                1,
                EnvironmentEventKind::SetBodyVelocity {
                    id: source,
                    velocity: Vec3::ZERO
                }
            ),
            Err(ScheduleError::InvalidBody)
        );
        assert_eq!(
            runtime.schedule(
                1,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: Vec3::new(101.0, 0.0, 0.0).unwrap()
                }
            ),
            Err(ScheduleError::InvalidBody)
        );
        runtime.schedule(1, move_event).unwrap();
        runtime
            .schedule(
                1,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: Vec3::new(0.0, 0.0, 1.0).unwrap(),
                },
            )
            .unwrap();
        let saved = runtime.save_bytes().unwrap();
        let mut bad: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        bad["runtime"]["world"]["body"]["radius"] = (-1.0).into();
        assert!(matches!(
            Runtime::load_bytes(&serde_json::to_vec(&bad).unwrap()),
            Err(PersistenceError::InvalidState("world"))
        ));
        let mut bad: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        bad["runtime"]["world"]["body"]["desired_velocity"]["x"] = 101.0.into();
        assert!(matches!(
            Runtime::load_bytes(&serde_json::to_vec(&bad).unwrap()),
            Err(PersistenceError::InvalidState("world"))
        ));
        let mut resumed = Runtime::load_bytes(&saved).unwrap();
        for _ in 0..30 {
            runtime.tick().unwrap();
            resumed.tick().unwrap();
        }
        assert_eq!(runtime, resumed);
        assert_eq!(runtime.world.body().unwrap().id(), body);
        assert_eq!(
            runtime.world.body().unwrap().desired_velocity(),
            Vec3::new(0.0, 0.0, 1.0).unwrap()
        );
        let before = runtime.clone();
        let invalid = [EnvironmentEvent {
            tick: 31,
            order: 0,
            kind: EnvironmentEventKind::SetBodyVelocity {
                id: body,
                velocity: Vec3::new(101.0, 0.0, 0.0).unwrap(),
            },
        }];
        assert!(
            advance_life(
                &mut runtime.world,
                &mut runtime.time,
                runtime.step,
                &invalid
            )
            .is_err()
        );
        assert_eq!(runtime, before);

        let (legacy, _) = fixture();
        let mut json: serde_json::Value =
            serde_json::from_slice(&legacy.save_bytes().unwrap()).unwrap();
        json["format_version"] = 3.into();
        json["runtime"]["world"]
            .as_object_mut()
            .unwrap()
            .remove("body");
        assert_eq!(
            Runtime::load_bytes(&serde_json::to_vec(&json).unwrap()).unwrap(),
            legacy
        );
    }

    #[test]
    fn finite_resource_save_replay_and_v4_legacy_migration() {
        let mut world = WorldState::new(DeterministicSeed(32));
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.3, 0.5, 1.0).unwrap(),
            )
            .unwrap();
        let source = world
            .spawn_finite_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 10.0, 0.0, 0.2, 0.2)
            .unwrap();
        let mut continuous = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        continuous
            .schedule(
                15,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false,
                },
            )
            .unwrap();
        for _ in 0..10 {
            continuous.tick().unwrap();
        }
        let saved = continuous.save_bytes().unwrap();
        let mut mislabeled: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        mislabeled["format_version"] = 4.into();
        assert!(Runtime::load_bytes(&serde_json::to_vec(&mislabeled).unwrap()).is_err());
        let mut resumed = Runtime::load_bytes(&saved).unwrap();
        for _ in 10..30 {
            continuous.tick().unwrap();
            resumed.tick().unwrap();
        }
        assert_eq!(continuous, resumed);
        let mut bad: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        bad["runtime"]["world"]["sources"][0]["reservoir"]["stored"] = (-1.0).into();
        let before = resumed.clone();
        assert!(
            resumed
                .load_into(&serde_json::to_vec(&bad).unwrap())
                .is_err()
        );
        assert_eq!(resumed, before);

        let (legacy, _) = fixture();
        let mut old: serde_json::Value =
            serde_json::from_slice(&legacy.save_bytes().unwrap()).unwrap();
        old["format_version"] = 4.into();
        old["runtime"]["world"]["sources"][0]
            .as_object_mut()
            .unwrap()
            .remove("reservoir");
        assert_eq!(
            Runtime::load_bytes(&serde_json::to_vec(&old).unwrap()).unwrap(),
            legacy
        );
    }

    #[test]
    fn finite_ecology_growth_and_pruning_refresh_body_colliders() {
        use world_simulation::contact::{ContactScene, SweepOutcome};
        let mut world = WorldState::new(DeterministicSeed(34));
        let parameters = GrowthParameters::new(0.14, 0.32, 0.1, 1.0).unwrap();
        let organism = world
            .spawn_organism(Vec3::new(0.6, 0.0, 0.0).unwrap(), parameters)
            .unwrap();
        world
            .spawn_organism(Vec3::new(-0.6, 0.0, 0.0).unwrap(), parameters)
            .unwrap();
        let source = world
            .spawn_finite_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 10.0, 0.0, 0.2, 0.2)
            .unwrap();
        let start = Vec3::new(0.6, 0.32, -1.0).unwrap();
        let movement = Vec3::new(0.0, 0.0, 2.0).unwrap();
        let body = world.spawn_body(start, 0.1).unwrap();
        let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        runtime.tick().unwrap();
        let grown = ContactScene::from_world(runtime.world()).unwrap();
        assert!(matches!(
            grown.sweep(start, movement, 0.1).unwrap(),
            SweepOutcome::Hit(_)
        ));
        assert_eq!(
            grown.sweep(start, movement, 0.1),
            grown.sweep_direct(start, movement, 0.1)
        );
        runtime
            .schedule(2, EnvironmentEventKind::PruneBranch { organism, child: 1 })
            .unwrap();
        runtime
            .schedule(
                2,
                EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false,
                },
            )
            .unwrap();
        runtime.tick().unwrap();
        assert!(runtime.world().organisms()[0].node(1).is_none());
        let pruned = ContactScene::from_world(runtime.world()).unwrap();
        assert_eq!(
            pruned.sweep(start, movement, 0.1).unwrap(),
            SweepOutcome::Miss
        );
        assert_eq!(
            pruned.sweep(start, movement, 0.1),
            pruned.sweep_direct(start, movement, 0.1)
        );
        runtime
            .schedule(
                3,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: Vec3::new(0.0, 0.0, 4.0).unwrap(),
                },
            )
            .unwrap();
        for _ in 0..5 {
            runtime.tick().unwrap();
        }
        assert!(runtime.world().body().unwrap().position().z() > 0.9);
        assert!(runtime.world().body().unwrap().contact().is_none());
    }

    #[test]
    fn four_organism_finite_ecology_stays_bounded_for_two_thousand_ticks() {
        let mut world = WorldState::new(DeterministicSeed(35));
        let parameters = GrowthParameters::new(0.1, 0.3, 0.5, 1.0).unwrap();
        for x in [-1.5, -0.5, 0.5, 1.5] {
            world
                .spawn_organism(Vec3::new(x, 0.0, 0.0).unwrap(), parameters)
                .unwrap();
        }
        world
            .spawn_finite_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 10.0, 0.0, 0.2, 0.2)
            .unwrap();
        let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        for _ in 0..2000 {
            runtime
                .tick()
                .unwrap_or_else(|error| panic!("tick {}: {error:?}", runtime.time().ticks() + 1));
            let stock = runtime.world().sources()[0].reservoir().unwrap();
            assert!((0.0..=stock.capacity()).contains(&stock.stored()));
            assert!(stock.last_allocated() <= 0.2 + 1e-12);
            assert!(
                runtime
                    .world()
                    .organisms()
                    .iter()
                    .all(|organism| organism.budget() >= 0.0)
            );
        }
        let saved = runtime.save_bytes().unwrap();
        assert_eq!(Runtime::load_bytes(&saved).unwrap(), runtime);
        assert_eq!(runtime.time().ticks(), 2000);
    }

    #[test]
    fn new_growth_overlapping_body_is_reported_without_teleporting() {
        let mut world = WorldState::new(DeterministicSeed(17));
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.3, 0.1, 1.0)
                    .unwrap()
                    .with_max_children(1)
                    .unwrap(),
            )
            .unwrap();
        world
            .spawn_source(Vec3::new(0.0, 1.0, 0.0).unwrap(), 3.0, 10.0)
            .unwrap();
        let position = Vec3::new(0.0, 0.3, 0.0).unwrap();
        world.spawn_body(position, 0.1).unwrap();
        let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        runtime.tick().unwrap();
        let body = runtime.world().body().unwrap();
        assert_eq!(body.position(), position);
        assert!(body.contact().unwrap().initial_overlap);
        assert_eq!(body.contact().unwrap().fraction, 0.0);
        assert_eq!(body.last_displacement(), Vec3::ZERO);
    }

    #[test]
    fn repeated_fast_body_steps_stop_at_sphere_and_invalid_sweep_rolls_back() {
        let mut world = WorldState::new(DeterministicSeed(24));
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(1.0, 0.3, 0.1, 1.0).unwrap(),
            )
            .unwrap();
        let body = world
            .spawn_body(Vec3::new(-5.0, 0.0, 0.0).unwrap(), 0.5)
            .unwrap();
        let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
        runtime
            .schedule(
                1,
                EnvironmentEventKind::SetBodyVelocity {
                    id: body,
                    velocity: Vec3::new(100.0, 0.0, 0.0).unwrap(),
                },
            )
            .unwrap();
        for _ in 0..10 {
            runtime.tick().unwrap();
            let x = runtime.world().body().unwrap().position().x();
            assert!((-1.50000001..=-1.5).contains(&x));
            assert!(runtime.world().body().unwrap().contact().is_some());
        }
        let mut far = WorldState::new(DeterministicSeed(25));
        let id = far
            .spawn_body(Vec3::new(999.0, 0.0, 0.0).unwrap(), 0.1)
            .unwrap();
        let mut invalid = Runtime::new(far, SimulationStep::new(1.0).unwrap());
        invalid
            .schedule(
                1,
                EnvironmentEventKind::SetBodyVelocity {
                    id,
                    velocity: Vec3::new(100.0, 0.0, 0.0).unwrap(),
                },
            )
            .unwrap();
        let before = invalid.clone();
        assert!(invalid.tick().is_err());
        assert_eq!(invalid, before);
    }

    #[test]
    fn two_thousand_ticks_of_prune_regrow_and_save_resume_are_bounded() {
        use world_simulation::contact::ContactScene;
        let mut world = WorldState::new(DeterministicSeed(18));
        let organism = world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.3, 0.12, 1.0)
                    .unwrap()
                    .with_max_children(1)
                    .unwrap(),
            )
            .unwrap();
        world
            .spawn_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 1.0)
            .unwrap();
        world
            .spawn_body(Vec3::new(0.12, 0.32, -1.0).unwrap(), 0.1)
            .unwrap();
        let mut continuous = Runtime::new(world, SimulationStep::new(0.01).unwrap());
        fn drive(runtime: &mut Runtime, organism: world_state::EntityId) {
            if runtime.time().ticks() % 25 == 24
                && let Some(child) = runtime.world().organisms()[0]
                    .nodes()
                    .iter()
                    .find(|node| node.parent() == Some(0))
                    .map(|node| node.id())
            {
                runtime
                    .schedule(
                        runtime.time().ticks() + 1,
                        EnvironmentEventKind::PruneBranch { organism, child },
                    )
                    .unwrap();
            }
            let previous_id = runtime.world().organisms()[0].next_node_id();
            runtime.tick().unwrap();
            runtime.world().validate().unwrap();
            assert!(runtime.world().organisms()[0].next_node_id() >= previous_id);
            assert!(runtime.world().organisms()[0].nodes().len() <= world_state::MAX_NODES);
            if runtime.time().ticks().is_multiple_of(100) {
                let scene = ContactScene::from_world(runtime.world()).unwrap();
                assert!(scene.solid_count() < world_state::MAX_NODES * 2);
                assert_eq!(
                    scene.sweep(
                        Vec3::new(0.12, 0.32, -1.0).unwrap(),
                        Vec3::new(0.0, 0.0, 2.0).unwrap(),
                        0.1
                    ),
                    scene.sweep_direct(
                        Vec3::new(0.12, 0.32, -1.0).unwrap(),
                        Vec3::new(0.0, 0.0, 2.0).unwrap(),
                        0.1
                    )
                );
                assert!(runtime.save_bytes().unwrap().len() < MAX_SAVE_BYTES);
            }
        }
        for _ in 0..1000 {
            drive(&mut continuous, organism);
        }
        let mut replay = Runtime::load_bytes(&continuous.save_bytes().unwrap()).unwrap();
        for _ in 1000..2000 {
            drive(&mut continuous, organism);
            drive(&mut replay, organism);
        }
        assert_eq!(continuous, replay);
        assert!(continuous.world().organisms()[0].next_node_id() > 20);
        assert_eq!(continuous.time().ticks(), 2000);
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
