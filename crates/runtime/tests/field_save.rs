//! Version-7 visual field save integrity and legacy-version gate.
use spatial_math::Transform;
use world_runtime::Runtime;
use world_simulation::SimulationStep;
use world_state::{DeterministicSeed, WorldState};

const TWIN: &[u8] = include_bytes!("../../../examples/field-graph/definitions/twin.json");

#[test]
fn field_world_round_trip_and_tamper_rejection() {
    let mut world = WorldState::new(DeterministicSeed(80));
    let id = world.spawn_field(TWIN, Transform::identity()).unwrap();
    let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
    runtime.tick().unwrap();
    let saved = runtime.save_bytes().unwrap();
    let mut loaded = Runtime::load_bytes(&saved).unwrap();
    assert_eq!(loaded, runtime);
    loaded.tick().unwrap();
    runtime.tick().unwrap();
    assert_eq!(loaded, runtime);
    assert!(loaded.world().field_graph(id).is_some());

    let mut json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(json["format_version"], 7);
    json["format_version"] = 6.into();
    assert!(Runtime::load_bytes(&serde_json::to_vec(&json).unwrap()).is_err());
    json["format_version"] = 7.into();
    json["runtime"]["world"]["field_definitions"][0]["source"][0] = 32.into();
    let corrupted = serde_json::to_vec(&json).unwrap();
    assert!(Runtime::load_bytes(&corrupted).is_err());
    let before = runtime.clone();
    assert!(runtime.load_into(&corrupted).is_err());
    assert_eq!(runtime, before);

    let empty = Runtime::new(
        WorldState::new(DeterministicSeed(81)),
        SimulationStep::new(0.1).unwrap(),
    );
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&empty.save_bytes().unwrap()).unwrap();
    legacy["format_version"] = 6.into();
    assert_eq!(
        Runtime::load_bytes(&serde_json::to_vec(&legacy).unwrap()).unwrap(),
        empty
    );
}
