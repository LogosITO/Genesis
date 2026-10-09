//! Authored save, replay and hostile-input regressions.
use spatial_math::{Transform, Vec3};
use world_runtime::Runtime;
use world_simulation::{EnvironmentEventKind, SimulationStep};
use world_state::{DeterministicSeed, WorldState};

const SOURCE: &[u8] = include_bytes!("../../../examples/authoring/world-single.json");
fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
fn pending_authored_events_survive_save_and_replay() {
    let mut world = WorldState::new(DeterministicSeed(5));
    let id = world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    let mut runtime = Runtime::new(world, SimulationStep::new(0.01).unwrap());
    runtime
        .schedule(
            1,
            EnvironmentEventKind::SetAuthoredTransform {
                id,
                transform: Transform::new(v(3.0, 0.0, 0.0), 2.0).unwrap(),
            },
        )
        .unwrap();
    runtime
        .schedule(
            2,
            EnvironmentEventKind::SetAuthoredEnabled { id, enabled: false },
        )
        .unwrap();
    let bytes = runtime.save_bytes().unwrap();
    let mut restored = Runtime::load_bytes(&bytes).unwrap();
    assert_eq!(runtime, restored);
    for _ in 0..2 {
        runtime.tick().unwrap();
        restored.tick().unwrap();
    }
    assert_eq!(runtime, restored);
    assert_eq!(
        runtime.world().authored_instances()[0]
            .transform()
            .translation(),
        v(3.0, 0.0, 0.0)
    );
    assert!(!runtime.world().authored_instances()[0].enabled());
    assert_eq!(runtime.world().authored_definitions()[0].source(), SOURCE);
    assert_eq!(runtime.world().authored_capsules().unwrap().len(), 0);
}

#[test]
fn corrupted_or_legacy_labeled_authored_saves_do_not_replace_runtime() {
    let mut world = WorldState::new(DeterministicSeed(6));
    world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    let mut runtime = Runtime::new(world, SimulationStep::new(0.01).unwrap());
    let original = runtime.clone();
    let valid: serde_json::Value = serde_json::from_slice(&runtime.save_bytes().unwrap()).unwrap();
    let mut mutations: Vec<serde_json::Value> = Vec::new();
    let mut old_label = valid.clone();
    old_label["format_version"] = 5.into();
    mutations.push(old_label);
    let mut wrong_hash = valid.clone();
    wrong_hash["runtime"]["world"]["authored_definitions"][0]["revision"][0] = 255.into();
    mutations.push(wrong_hash);
    let mut wrong_semantics = valid.clone();
    wrong_semantics["runtime"]["world"]["authored_definitions"][0]["compiler_semantics_version"] =
        999.into();
    mutations.push(wrong_semantics);
    let mut wrong_instance_semantics = valid.clone();
    wrong_instance_semantics["runtime"]["world"]["authored_instances"][0]["compiler_semantics_version"] =
        999.into();
    mutations.push(wrong_instance_semantics);
    let mut dangling = valid.clone();
    dangling["runtime"]["world"]["authored_instances"][0]["revision"][0] = 255.into();
    mutations.push(dangling);
    let mut invalid_scale = valid.clone();
    invalid_scale["runtime"]["world"]["authored_instances"][0]["transform"]["scale"] = 0.into();
    mutations.push(invalid_scale);
    let mut duplicate = valid;
    let entry = duplicate["runtime"]["world"]["authored_instances"][0].clone();
    duplicate["runtime"]["world"]["authored_instances"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    mutations.push(duplicate);
    for value in mutations {
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(runtime.load_into(&bytes).is_err());
        assert_eq!(runtime, original);
    }
}

#[test]
fn stale_event_is_rejected_before_schedule() {
    let mut world = WorldState::new(DeterministicSeed(7));
    let id = world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    world.remove_authored(id).unwrap();
    let mut runtime = Runtime::new(world, SimulationStep::new(0.01).unwrap());
    assert!(
        runtime
            .schedule(
                1,
                EnvironmentEventKind::SetAuthoredEnabled { id, enabled: false }
            )
            .is_err()
    );
    assert!(runtime.pending_events().is_empty());
}
