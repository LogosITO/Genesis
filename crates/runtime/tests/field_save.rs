//! Version-7 visual field save integrity and legacy-version gate.
use spatial_math::{Transform, Vec3};
use world_runtime::Runtime;
use world_simulation::SimulationStep;
use world_state::{
    DeterministicSeed, ParameterBinding, ParameterComponent, ParameterRule, WorldState,
};

const TWIN: &[u8] = include_bytes!("../../../examples/field-graph/definitions/twin.json");
const MORPH: &[u8] = include_bytes!("../../../examples/field-graph/definitions/morph-lobes.json");
const BOX: &[u8] = br#"{"format_version":1,"id":"grow_box","root":"body","nodes":[{"op":"box","id":"body","half_extent":[0.4,0.3,0.3]}]}"#;

#[test]
fn generic_parameter_world_uses_one_ledger_and_requires_v10() {
    let mut world = WorldState::new(DeterministicSeed(91));
    let sphere = world.spawn_field(MORPH, Transform::identity()).unwrap();
    let box_id = world
        .spawn_field(
            BOX,
            Transform::new(Vec3::new(0.8, 0.0, 0.0).unwrap(), 1.0).unwrap(),
        )
        .unwrap();
    for (id, node, component) in [
        (sphere, "left_ball", ParameterComponent::SphereRadius),
        (box_id, "body", ParameterComponent::BoxHalfX),
    ] {
        world
            .enable_field_parameters(
                id,
                vec![ParameterRule {
                    binding: ParameterBinding {
                        node: node.into(),
                        component,
                    },
                    uptake: 2.0,
                    gain_per_unit: 0.2,
                    maximum: 1.2,
                    positive_site: true,
                    feedback: true,
                }],
            )
            .unwrap();
    }
    world
        .spawn_finite_source(Vec3::new(0.5, 0.0, 0.0).unwrap(), 2.0, 10.0, 0.5, 0.5, 0.0)
        .unwrap();
    let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
    runtime.tick().unwrap();
    let allocated: f64 = runtime
        .world()
        .field_instances()
        .iter()
        .map(|field| field.parameters().unwrap().channels()[0].total_allocated())
        .sum();
    assert!(allocated > 0.0 && allocated <= 0.5);
    assert_eq!(
        runtime.world().sources()[0]
            .reservoir()
            .unwrap()
            .total_allocated(),
        allocated
    );
    let save = runtime.save_bytes().unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&save).unwrap();
    assert_eq!(value["format_version"], 10);
    assert_eq!(Runtime::load_bytes(&save).unwrap(), runtime);
    value["format_version"] = 9.into();
    assert!(Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&save).unwrap();
    value["runtime"]["world"]["field_instances"][0]["parameters"]["channels"][0]["current"] =
        serde_json::json!(-1.0);
    assert!(Runtime::load_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut resumed = Runtime::load_bytes(&save).unwrap();
    runtime.tick().unwrap();
    resumed.tick().unwrap();
    assert_eq!(resumed, runtime);
}

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

#[test]
fn dynamic_field_requires_v8_and_survives_repeated_save_load() {
    let mut world = WorldState::new(DeterministicSeed(84));
    let id = world
        .spawn_field(TWIN, Transform::new(spatial_math::Vec3::ZERO, 0.4).unwrap())
        .unwrap();
    world.enable_field_growth(id, 1.0, 0.4, 0.8).unwrap();
    world.grow_field_allocated(id, 0.25).unwrap();
    let revision = world.field_instance(id).unwrap().revision();
    let mut runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
    let saved = runtime.save_bytes().unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(json["format_version"], 8);
    for _ in 0..3 {
        runtime = Runtime::load_bytes(&runtime.save_bytes().unwrap()).unwrap();
        assert_eq!(
            runtime.world().field_instance(id).unwrap().revision(),
            revision
        );
    }
    assert_eq!(
        runtime
            .world()
            .field_instance(id)
            .unwrap()
            .growth()
            .unwrap()
            .total_allocated(),
        0.25
    );
    json["format_version"] = 7.into();
    assert!(Runtime::load_bytes(&serde_json::to_vec(&json).unwrap()).is_err());
    json["format_version"] = 8.into();
    json["runtime"]["world"]["field_instances"][0]["growth"]["total_allocated"] = (-1.0).into();
    let corrupt = serde_json::to_vec(&json).unwrap();
    let before = runtime.clone();
    assert!(runtime.load_into(&corrupt).is_err());
    assert_eq!(runtime, before);
}

#[test]
fn two_lobe_morphology_requires_v9_and_rebuilds_exact_source() {
    let mut world = WorldState::new(DeterministicSeed(86));
    let id = world.spawn_field(MORPH, Transform::identity()).unwrap();
    world
        .enable_field_morphology(id, [1.0, 0.8], [0.2, 0.3], [1.2, 1.2], true)
        .unwrap();
    world.grow_field_morphology(id, [0.5, 0.25]).unwrap();
    let runtime = Runtime::new(world, SimulationStep::new(0.1).unwrap());
    let saved = runtime.save_bytes().unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(json["format_version"], 9);
    let mut loaded = Runtime::load_bytes(&saved).unwrap();
    assert_eq!(loaded, runtime);
    assert_eq!(loaded.world().field_definitions()[0].source(), MORPH);
    for _ in 0..3 {
        loaded = Runtime::load_bytes(&loaded.save_bytes().unwrap()).unwrap();
    }
    assert_eq!(loaded, runtime);
    json["format_version"] = 8.into();
    assert!(Runtime::load_bytes(&serde_json::to_vec(&json).unwrap()).is_err());
    json["format_version"] = 9.into();
    json["runtime"]["world"]["field_instances"][0]["morphology"]["radii"][0] = (-1.0).into();
    let corrupt = serde_json::to_vec(&json).unwrap();
    let before = loaded.clone();
    assert!(loaded.load_into(&corrupt).is_err());
    assert_eq!(loaded, before);
}
