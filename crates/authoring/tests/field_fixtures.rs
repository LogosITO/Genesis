//! Checked research scenes with distinct geometric purposes.
use spatial_math::Vec3;
use world_authoring::field_graph::{
    MAX_NODES, RegionAabb, RegionExclusion, compile_json, exclude_region,
};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
fn shared_face_fixture_has_an_internal_zero_not_an_external_boundary() {
    let source = include_bytes!("../../../examples/field-graph/definitions/shared-face.json");
    let graph = compile_json(source).unwrap();
    assert_eq!(graph.nodes().len(), 4);
    assert_eq!(graph.sample(Vec3::ZERO).unwrap(), 0.0);
    assert!(graph.sample(v(-0.5, 0.0, 0.0)).unwrap() < 0.0);
    assert!(graph.sample(v(0.5, 0.0, 0.0)).unwrap() < 0.0);
    let centre = RegionAabb::new(Vec3::ZERO, Vec3::ZERO).unwrap();
    assert_eq!(exclude_region(&graph, centre), RegionExclusion::Unknown);
    assert_eq!(graph.revision(), compile_json(source).unwrap().revision());
}

#[test]
fn balanced_fixture_fills_node_budget_without_exceeding_visit_budget() {
    let graph = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/balanced-16.json"
    ))
    .unwrap();
    assert_eq!(graph.nodes().len(), MAX_NODES);
    assert_eq!(graph.visits(), 47);
    for y in [-1.5, -0.5, 0.5, 1.5] {
        for x in [-1.5, -0.5, 0.5, 1.5] {
            assert!((graph.sample(v(x, y, 0.0)).unwrap() + 0.35).abs() < 1e-12);
        }
    }
    assert!(graph.sample(Vec3::ZERO).unwrap() > 0.0);
    let bounds = graph.bounds();
    assert!(bounds.min.x() <= -1.85 && bounds.max.x() >= 1.85);
    assert!(bounds.min.y() <= -1.85 && bounds.max.y() >= 1.85);
}
