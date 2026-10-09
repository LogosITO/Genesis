//! Authored occurrence identity, validation and transform regressions.
use spatial_math::{Transform, Vec3};
use world_state::{DeterministicSeed, MAX_AUTHORED_INSTANCES, WorldError, WorldState};

const SOURCE: &[u8] = include_bytes!("../../../examples/authoring/world-single.json");

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
fn shared_definition_distinct_world_ids_and_monotonic_removal() {
    let mut world = WorldState::new(DeterministicSeed(1));
    let first = world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    let second = world
        .spawn_authored(SOURCE, Transform::new(v(3.0, 0.0, 0.0), 2.0).unwrap(), true)
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(world.authored_definitions().len(), 1);
    let capsules = world.authored_capsules().unwrap();
    assert_eq!(capsules.len(), 2);
    assert_eq!(capsules[1].0.start, v(3.0, 0.0, 0.0));
    assert_eq!(capsules[1].0.end, v(3.0, 2.0, 0.0));
    assert_eq!(capsules[1].0.radius, 0.2);
    let old = capsules[0].0.reference;
    assert!(world.authored_segment(old).is_some());
    world.remove_authored(first).unwrap();
    assert!(world.authored_segment(old).is_none());
    let third = world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    assert!(third.value() > second.value());
    assert!(world.authored_segment(old).is_none());
    world.validate().unwrap();
}

#[test]
fn invalid_changes_are_atomic_and_old_selections_expire() {
    let mut world = WorldState::new(DeterministicSeed(2));
    let id = world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    let old = world.authored_capsules().unwrap()[0].0.reference;
    let before = world.clone();
    assert!(
        world
            .spawn_authored(b"{}", Transform::identity(), true)
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .set_authored_transform(id, Transform::new(Vec3::ZERO, 1e-6).unwrap())
            .is_err()
    );
    assert_eq!(world, before);
    let replacement = String::from_utf8(SOURCE.to_vec())
        .unwrap()
        .replace("world-single", "world-other");
    assert!(world.replace_authored(id, b"{}").is_err());
    assert_eq!(world, before);
    world.replace_authored(id, replacement.as_bytes()).unwrap();
    assert_eq!(world.authored_instance(id).unwrap().id(), id);
    assert!(world.authored_segment(old).is_none());
    let current = world.authored_capsules().unwrap()[0].0.reference;
    world
        .set_authored_transform(id, Transform::new(v(4.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    assert!(world.authored_segment(current).is_none());
    let moved = world.authored_capsules().unwrap()[0].0.reference;
    world.set_authored_enabled(id, false).unwrap();
    assert!(world.authored_segment(moved).is_none());
    world.validate().unwrap();
}

#[test]
fn capacity_and_unsupported_transform_domain() {
    let mut world = WorldState::new(DeterministicSeed(3));
    for _ in 0..MAX_AUTHORED_INSTANCES {
        world
            .spawn_authored(SOURCE, Transform::identity(), true)
            .unwrap();
    }
    let before = world.clone();
    assert_eq!(
        world.spawn_authored(SOURCE, Transform::identity(), true),
        Err(WorldError::Capacity)
    );
    assert_eq!(world, before);
    let mut empty = WorldState::new(DeterministicSeed(4));
    assert!(
        empty
            .spawn_authored(
                SOURCE,
                Transform::new(v(9_001.0, 0.0, 0.0), 1.0).unwrap(),
                true
            )
            .is_err()
    );
    assert!(empty.authored_instances().is_empty());
    assert!(Transform::new(Vec3::ZERO, f64::NAN).is_err());
    assert!(Transform::new(Vec3::ZERO, -1.0).is_err());
}

#[test]
fn aggregate_segment_budget_rejects_without_partial_insert() {
    let source = br#"{
        "format_version":1,"id":"many","revision":1,"axiom":"FFFF",
        "rules":[{"symbol":"F","replacement":"FF"}],"iterations":8,
        "step_length":0.01,"yaw_degrees":0,"pitch_degrees":0,"branch_radius":0.01,
        "budgets":{"max_symbols":1024,"max_stack_depth":1,"max_segments":1024,"max_work":4096}
    }"#;
    let mut world = WorldState::new(DeterministicSeed(8));
    for _ in 0..4 {
        world
            .spawn_authored(source, Transform::identity(), true)
            .unwrap();
    }
    assert_eq!(world.authored_capsules().unwrap().len(), 4096);
    let before = world.clone();
    assert_eq!(
        world.spawn_authored(source, Transform::identity(), true),
        Err(WorldError::Capacity)
    );
    assert_eq!(world, before);
}
