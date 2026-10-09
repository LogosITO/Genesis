//! Independent analytic entry references and ideal-only spatial classifications.
use analytic_field::{AxisAlignedBox, ScalarField};
use spatial_math::Vec3;
use world_authoring::field_graph::{
    IdealRegionEstimate, RegionAabb, SampleSign, SolidEntryOutcome, SolidQueryIssue,
    SolidRayOptions, TraceOptions, classify_sample, compile_json, estimate_region,
    trace_exterior_entry,
};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}
fn graph(nodes: Vec<serde_json::Value>, root: &str) -> world_authoring::field_graph::Graph {
    let source =
        serde_json::json!({"format_version":1,"id":"solid_test","root":root,"nodes":nodes});
    compile_json(&serde_json::to_vec(&source).unwrap()).unwrap()
}
fn sphere(id: &str, r: f64) -> serde_json::Value {
    serde_json::json!({"op":"sphere","id":id,"radius":r})
}
fn at(id: &str, child: &str, p: [f64; 3]) -> serde_json::Value {
    serde_json::json!({"op":"translate","id":id,"child":child,"offset":p})
}
fn union(id: &str, a: &str, b: &str) -> serde_json::Value {
    serde_json::json!({"op":"union","id":id,"left":a,"right":b})
}
fn opts() -> SolidRayOptions {
    SolidRayOptions {
        ray: TraceOptions {
            near: 0.0,
            far: 20.0,
            tolerance: 1e-5,
            iterations: 256,
            safety: 0.8,
        },
        max_work: 16_384,
    }
}
fn entry(g: &world_authoring::field_graph::Graph, o: Vec3, d: Vec3) -> SolidEntryOutcome {
    trace_exterior_entry(g, o, d, opts()).unwrap()
}

// Quadratic intersection is independent of the graph's field sample and sphere tracer.
fn sphere_entry(o: Vec3, d: Vec3, c: Vec3, r: f64) -> Option<f64> {
    let q = o.checked_sub(c).unwrap();
    let b = q.x() * d.x() + q.y() * d.y() + q.z() * d.z();
    let discriminant = b * b - (q.x() * q.x() + q.y() * q.y() + q.z() * q.z() - r * r);
    if discriminant < 0.0 {
        None
    } else {
        let t = -b - discriminant.sqrt();
        (t >= 0.0).then_some(t)
    }
}
fn assert_near_entry(g: &world_authoring::field_graph::Graph, o: Vec3, d: Vec3, expected: f64) {
    match entry(g, o, d) {
        SolidEntryOutcome::ApproximateEntry { distance } => assert!(
            (distance - expected).abs() < 0.001,
            "actual={distance} expected={expected}"
        ),
        other => panic!("expected analytic entry {expected}, got {other:?}"),
    }
}

#[test]
fn primitive_transforms_and_union_entries_match_independent_roots() {
    let o = v(0.0, 0.0, -3.0);
    let d = v(0.0, 0.0, 1.0);
    let single = graph(vec![sphere("s", 1.0)], "s");
    assert_near_entry(&single, o, d, sphere_entry(o, d, Vec3::ZERO, 1.0).unwrap());
    let box_graph = graph(
        vec![serde_json::json!({"op":"box","id":"b","half_extent":[1,1,1]})],
        "b",
    );
    assert_near_entry(&box_graph, o, d, 2.0); // independent near slab: z = -1
    let translated = graph(vec![sphere("s", 1.0), at("m", "s", [2.0, 0.0, 0.0])], "m");
    let shifted_o = v(2.0, 0.0, -3.0);
    assert_near_entry(
        &translated,
        shifted_o,
        d,
        sphere_entry(shifted_o, d, v(2.0, 0.0, 0.0), 1.0).unwrap(),
    );
    let scaled = graph(
        vec![
            sphere("s", 1.0),
            serde_json::json!({"op":"scale","id":"large","child":"s","factor":2.0}),
        ],
        "large",
    );
    assert_near_entry(&scaled, v(0.0, 0.0, -4.0), d, 2.0);
    let disjoint = graph(
        vec![
            sphere("s", 1.0),
            at("a", "s", [0.0, 0.0, 0.0]),
            at("b", "s", [0.0, 0.0, 5.0]),
            union("u", "a", "b"),
        ],
        "u",
    );
    assert_near_entry(&disjoint, o, d, 2.0); // first of two occupied intervals
    let overlapping = graph(
        vec![
            sphere("s", 1.0),
            at("a", "s", [0.0, 0.0, 0.0]),
            at("b", "s", [0.0, 0.0, 1.0]),
            union("u", "a", "b"),
        ],
        "u",
    );
    assert_near_entry(&overlapping, o, d, 2.0);
    let coincident = graph(vec![sphere("s", 1.0), union("u", "s", "s")], "u");
    assert_near_entry(&coincident, o, d, 2.0);
    let nested = graph(
        vec![
            sphere("outer", 2.0),
            sphere("inner", 1.0),
            union("u", "outer", "inner"),
        ],
        "u",
    );
    assert_near_entry(&nested, v(0.0, 0.0, -4.0), d, 2.0);
    assert_eq!(
        entry(&single, v(3.0, 0.0, -3.0), d),
        SolidEntryOutcome::IdealModelMiss
    );
    assert_eq!(
        entry(&single, Vec3::ZERO, d),
        SolidEntryOutcome::Indeterminate(SolidQueryIssue::StartsInside)
    );
    let tangent_origin = v(0.9999, 0.0, -3.0);
    let reference = sphere_entry(tangent_origin, d, Vec3::ZERO, 1.0).unwrap();
    match entry(&single, tangent_origin, d) {
        SolidEntryOutcome::ApproximateEntry { distance } => {
            assert!((distance - reference).abs() < 0.02)
        }
        SolidEntryOutcome::Exhausted => {}
        other => panic!("near tangent falsely classified: {other:?}"),
    }
}

#[test]
fn adjacent_box_face_is_internal_zero_not_external_boundary() {
    let boxes = graph(
        vec![
            serde_json::json!({"op":"box","id":"box","half_extent":[1,1,1]}),
            at("left", "box", [-1.0, 0.0, 0.0]),
            at("right", "box", [1.0, 0.0, 0.0]),
            union("u", "left", "right"),
        ],
        "u",
    );
    assert_eq!(boxes.sample(Vec3::ZERO).unwrap(), 0.0);
    assert_eq!(
        classify_sample(&boxes, Vec3::ZERO, 1e-8).unwrap(),
        SampleSign::BoundaryCandidate
    );
    for x in [-0.5, -0.25, 0.25, 0.5] {
        assert!(boxes.sample(v(x, 0.0, 0.0)).unwrap() < 0.0);
    }
    // The union equals [-2,2] × [-1,1]², so B(0,0.5) is wholly occupied.
    // Thus the shared-face centre has positive distance to the external boundary.
    assert_near_entry(&boxes, v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 1.0);
    let region = RegionAabb::new(v(-0.1, -0.1, -0.1), v(0.1, 0.1, 0.1)).unwrap();
    assert_eq!(
        estimate_region(&boxes, region),
        IdealRegionEstimate::Unknown
    );
}

#[test]
fn ideal_region_bounds_and_numerical_failures_are_explicit() {
    let g = graph(vec![sphere("s", 1.0)], "s");
    let far = RegionAabb::new(v(2.9, -0.1, -0.1), v(3.1, 0.1, 0.1)).unwrap();
    assert!(matches!(estimate_region(&g,far),IdealRegionEstimate::Outside{lower} if lower>0.0));
    let inside = RegionAabb::new(v(-0.1, -0.1, -0.1), v(0.1, 0.1, 0.1)).unwrap();
    assert!(matches!(estimate_region(&g,inside),IdealRegionEstimate::Inside{upper} if upper<0.0));
    let crossing = RegionAabb::new(v(0.8, -0.2, -0.2), v(1.2, 0.2, 0.2)).unwrap();
    assert_eq!(estimate_region(&g, crossing), IdealRegionEstimate::Unknown);
    assert!(RegionAabb::new(v(2.0, 0.0, 0.0), v(1.0, 0.0, 0.0)).is_err());
    assert!(RegionAabb::new(v(0.0, 0.0, 0.0), v(10_001.0, 0.0, 0.0)).is_err());
    assert!(classify_sample(&g, Vec3::ZERO, f64::NAN).is_err());
    assert_eq!(
        classify_sample(&g, v(f64::MAX, f64::MAX, 0.0), 0.0).unwrap(),
        SampleSign::Indeterminate
    );
    assert_eq!(
        trace_exterior_entry(
            &g,
            v(0.0, 0.0, -3.0),
            v(0.0, 0.0, 1.0),
            SolidRayOptions {
                max_work: 1,
                ..opts()
            }
        )
        .unwrap(),
        SolidEntryOutcome::Indeterminate(SolidQueryIssue::WorkBudget)
    );
    assert_eq!(
        trace_exterior_entry(
            &g,
            v(0.0, 0.0, -3.0),
            v(0.0, 0.0, 1.0),
            SolidRayOptions {
                ray: TraceOptions {
                    iterations: 1,
                    ..opts().ray
                },
                ..opts()
            }
        )
        .unwrap(),
        SolidEntryOutcome::Exhausted
    );
    assert!(compile_json(br#"{"format_version":1,"id":"bad","root":"s","nodes":[{"op":"sphere","id":"s","radius":0}]}"#).is_err());
    let exact_box = AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap();
    assert_eq!(exact_box.sample(v(0.0, 0.0, 0.0)).unwrap(), -1.0);
    let repeated = graph(vec![sphere("s", 1.0), union("u", "s", "s")], "u");
    assert_eq!(repeated.visits(), 3);
    assert_eq!(
        trace_exterior_entry(
            &repeated,
            v(0.0, 0.0, -3.0),
            v(0.0, 0.0, 1.0),
            SolidRayOptions {
                max_work: 2,
                ..opts()
            }
        )
        .unwrap(),
        SolidEntryOutcome::Indeterminate(SolidQueryIssue::WorkBudget)
    );
    assert_eq!(
        classify_sample(&g, v(f64::MIN_POSITIVE, 0.0, 0.0), 1e-12).unwrap(),
        SampleSign::Negative
    );
}
