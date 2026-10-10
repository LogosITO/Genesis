//! Checked research scenes with distinct geometric purposes.
use analytic_field::Gradient;
use spatial_math::Vec3;
use world_authoring::field_graph::{
    DistanceSemantics, MAX_NODES, RegionAabb, RegionExclusion, SolidEntryOutcome, SolidQueryIssue,
    SolidRayOptions, TraceOptions, compile_json, exclude_region, trace_exterior_entry,
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

#[test]
fn smooth_bridge_changes_the_occupied_set_without_claiming_exact_distance() {
    let graph = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/smooth-bridge.json"
    ))
    .unwrap();
    let hard = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/hard-bridge.json"
    ))
    .unwrap();
    assert_eq!(
        graph.distance_semantics(),
        DistanceSemantics::SignedUnitLipschitz
    );
    assert!((graph.sample(Vec3::ZERO).unwrap() + 0.3).abs() < 1e-12);
    assert!(graph.sample(v(0.0, 0.5, 0.0)).unwrap() < 0.0);
    assert!(hard.sample(v(0.0, 0.5, 0.0)).unwrap() > 0.0);
    assert!(graph.sample(v(0.0, 0.9, 0.0)).unwrap() > 0.0);
    let bounds = graph.bounds();
    assert!(bounds.min.x() < -2.1 && bounds.max.x() > 2.1);
    assert!(bounds.min.y() < -1.2 && bounds.max.y() > 1.2);
    assert!(
        matches!(graph.gradient(v(0.0, 0.5, 0.0)).unwrap(), Gradient::Defined(g)
        if g.x().abs() < 1e-12 && (g.y()-0.5 / 1.06_f64.sqrt()).abs() < 1e-12)
    );
    let options = SolidRayOptions {
        ray: TraceOptions {
            near: 0.0,
            far: 8.0,
            tolerance: 1e-5,
            iterations: 128,
            safety: 0.8,
        },
        max_work: 8192,
    };
    let outcome =
        trace_exterior_entry(&graph, v(0.0, 0.0, -4.0), v(0.0, 0.0, 1.0), options).unwrap();
    assert_eq!(
        outcome,
        SolidEntryOutcome::Indeterminate(SolidQueryIssue::UnsupportedOperation)
    );
    assert!(trace_exterior_entry(&graph, v(0.0, 0.0, -4.0), Vec3::ZERO, options).is_err());
}

#[test]
fn smooth_union_rejects_invalid_width_and_non_sdf_children() {
    let source = include_str!("../../../examples/field-graph/definitions/smooth-bridge.json");
    for width in ["0", "-1", "101", "null"] {
        let invalid = source.replace("\"width\": 0.8", &format!("\"width\": {width}"));
        assert!(compile_json(invalid.as_bytes()).is_err(), "width {width}");
    }
    let invalid = source.replace(
        "\"left\": \"left\", \"right\": \"right\"",
        "\"left\": \"bridge\", \"right\": \"right\"",
    );
    assert!(compile_json(invalid.as_bytes()).is_err());
    let nested = r#"{"format_version":1,"id":"nested","root":"outer","nodes":[{"op":"sphere","id":"s","radius":1},{"op":"smooth_union","id":"inner","left":"s","right":"s","width":1},{"op":"smooth_union","id":"outer","left":"inner","right":"s","width":1}]}"#;
    assert!(compile_json(nested.as_bytes()).is_err());
}
