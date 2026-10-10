//! World identity and read-only field occurrence regressions.
use spatial_math::{Transform, Vec3};
use world_state::{DeterministicSeed, MAX_FIELD_INSTANCES, WorldError, WorldState};

const TWIN: &[u8] = include_bytes!("../../../examples/field-graph/definitions/twin.json");
const MORPH: &[u8] = include_bytes!("../../../examples/field-graph/definitions/morph-lobes.json");
const PLINTH: &[u8] = include_bytes!("../../../examples/field-graph/definitions/plinth.json");

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
fn shared_definition_instances_keep_world_ids_and_transform_samples() {
    let mut world = WorldState::new(DeterministicSeed(77));
    let first = world.spawn_field(TWIN, Transform::identity()).unwrap();
    let second = world
        .spawn_field(TWIN, Transform::new(v(5.0, 0.0, 0.0), 2.0).unwrap())
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(world.field_definitions().len(), 1);
    assert!(std::ptr::eq(
        world.field_graph(first).unwrap(),
        world.field_graph(second).unwrap()
    ));
    assert_eq!(
        world.field_sample(first, Vec3::ZERO).unwrap() * 2.0,
        world.field_sample(second, v(5.0, 0.0, 0.0)).unwrap()
    );
    let second_bounds = world.field_bounds(second).unwrap();
    assert!(second_bounds.min.x() <= 1.9);
    assert!(second_bounds.max.x() >= 8.1);
    let old_generation = world.field_instance(second).unwrap().generation();
    world
        .set_field_transform(second, Transform::new(v(6.0, 0.0, 0.0), 2.0).unwrap())
        .unwrap();
    assert_eq!(
        world.field_instance(second).unwrap().generation(),
        old_generation + 1
    );
    world.set_field_enabled(second, false).unwrap();
    assert!(matches!(
        world.field_bounds(second),
        Err(WorldError::UnknownEntity)
    ));
    assert_eq!(
        world.field_sample(second, Vec3::ZERO),
        Err(WorldError::UnknownEntity)
    );
    world.remove_field(first).unwrap();
    let third = world.spawn_field(TWIN, Transform::identity()).unwrap();
    assert!(third.value() > second.value());
    world.validate().unwrap();
}

#[test]
fn invalid_edits_leave_world_unchanged_and_collision_world_empty() {
    let mut world = WorldState::new(DeterministicSeed(78));
    let id = world.spawn_field(TWIN, Transform::identity()).unwrap();
    let before = world.clone();
    assert!(world.spawn_field(b"{}", Transform::identity()).is_err());
    assert!(world.replace_field(id, b"{}").is_err());
    assert!(
        world
            .set_field_transform(id, Transform::new(v(9_999.0, 0.0, 0.0), 100.0).unwrap())
            .is_err()
    );
    assert_eq!(world, before);
    world.replace_field(id, PLINTH).unwrap();
    assert_eq!(world.field_instance(id).unwrap().id(), id);
    assert_ne!(
        world.field_instance(id).unwrap().revision(),
        before.field_instance(id).unwrap().revision()
    );
    assert_eq!(world.entities().len(), 0);
    assert_eq!(world.authored_instances().len(), 0);
    world.validate().unwrap();
}

#[test]
fn capacity_rejection_does_not_consume_an_id() {
    let mut world = WorldState::new(DeterministicSeed(79));
    for _ in 0..MAX_FIELD_INSTANCES {
        world.spawn_field(TWIN, Transform::identity()).unwrap();
    }
    let before = world.clone();
    assert_eq!(
        world.spawn_field(TWIN, Transform::identity()),
        Err(WorldError::Capacity)
    );
    assert_eq!(world, before);
}

#[test]
fn repeated_valid_edits_replace_unused_revisions() {
    let mut world = WorldState::new(DeterministicSeed(81));
    let id = world.spawn_field(TWIN, Transform::identity()).unwrap();
    for spaces in 1..=10 {
        let mut source = TWIN.to_vec();
        source.extend(std::iter::repeat_n(b' ', spaces));
        world.replace_field(id, &source).unwrap();
        assert_eq!(world.field_definitions().len(), 1);
    }
    world.remove_field(id).unwrap();
    assert!(world.field_definitions().is_empty());
    world.validate().unwrap();

    let mut full = WorldState::new(DeterministicSeed(82));
    let mut ids = Vec::new();
    for spaces in 0..4 {
        let mut source = TWIN.to_vec();
        source.extend(std::iter::repeat_n(b' ', spaces));
        ids.push(full.spawn_field(&source, Transform::identity()).unwrap());
    }
    assert_eq!(full.field_definitions().len(), 4);
    let mut fifth = TWIN.to_vec();
    fifth.extend(std::iter::repeat_n(b' ', 5));
    full.replace_field(ids[0], &fifth).unwrap();
    assert_eq!(full.field_definitions().len(), 4);
    full.validate().unwrap();
}

#[test]
fn resource_growth_changes_field_scale_without_rewriting_definition() {
    let mut world = WorldState::new(DeterministicSeed(83));
    let id = world
        .spawn_field(TWIN, Transform::new(v(0.0, 0.0, 0.0), 0.4).unwrap())
        .unwrap();
    let revision = world.field_instance(id).unwrap().revision();
    let source = world.field_definitions()[0].source().to_vec();
    let before = world.field_sample(id, Vec3::ZERO).unwrap();
    world.enable_field_growth(id, 1.0, 0.4, 0.8).unwrap();
    world.grow_field_allocated(id, 0.25).unwrap();
    let grown = world.field_instance(id).unwrap();
    assert_eq!(grown.id(), id);
    assert_eq!(grown.revision(), revision);
    assert_eq!(grown.transform().scale(), 0.5);
    assert_eq!(grown.growth().unwrap().total_allocated(), 0.25);
    assert!(world.field_sample(id, Vec3::ZERO).unwrap() < before);
    assert_eq!(world.field_definitions()[0].source(), source);
    let valid = world.clone();
    assert!(world.grow_field_allocated(id, f64::NAN).is_err());
    assert!(world.grow_field_allocated(id, 10.0).is_err());
    assert!(world.replace_field(id, PLINTH).is_err());
    assert!(
        world
            .set_field_transform(id, Transform::new(Vec3::ZERO, 0.6).unwrap())
            .is_err()
    );
    assert!(world.enable_field_growth(id, 1.0, 0.4, 0.8).is_err());
    assert_eq!(world, valid);
    world.validate().unwrap();
}

#[test]
fn two_lobe_morphology_updates_cpu_field_bounds_and_sites_atomically() {
    let mut world = WorldState::new(DeterministicSeed(85));
    let id = world.spawn_field(MORPH, Transform::identity()).unwrap();
    let source = world.field_definitions()[0].source().to_vec();
    let revision = world.field_instance(id).unwrap().revision();
    world
        .enable_field_morphology(id, [1.0, 0.5], [0.2, 0.4], [1.2, 1.2], true)
        .unwrap();
    let before = world.field_bounds(id).unwrap();
    let site = world.field_morph_site(id, 0).unwrap();
    world.grow_field_morphology(id, [0.5, 0.25]).unwrap();
    let morphology = world.field_instance(id).unwrap().morphology().unwrap();
    assert_eq!(morphology.radii(), [0.6, 0.6]);
    assert!(world.field_bounds(id).unwrap().min.x() < before.min.x());
    assert!(world.field_morph_site(id, 0).unwrap().x() < site.x());
    assert_eq!(world.field_instance(id).unwrap().revision(), revision);
    assert_eq!(world.field_definitions()[0].source(), source);
    let stable = world.clone();
    assert!(world.grow_field_morphology(id, [f64::NAN, 0.0]).is_err());
    assert!(world.grow_field_morphology(id, [10.0, 0.0]).is_err());
    assert!(world.replace_field(id, TWIN).is_err());
    assert_eq!(world, stable);
    world.validate().unwrap();
}
