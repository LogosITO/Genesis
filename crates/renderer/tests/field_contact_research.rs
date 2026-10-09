//! Research comparison only: field graph geometry has no WorldState collision authority.
use analytic_field::Sphere;
use spatial_math::{Transform, Vec3};
use world_authoring::field_graph::{
    SolidEntryOutcome, SolidSweepOptions, compile_json, sweep_sphere_exterior,
};
use world_simulation::contact::{ContactScene, SweepOutcome};
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
fn exterior_sphere_expansion_agrees_with_analytic_sweeps_away_from_tangencies() {
    for centres in [vec![0.0], vec![0.0, 2.5], vec![0.0, 0.5]] {
        let mut world = WorldState::new(DeterministicSeed(7));
        let mut nodes = vec![serde_json::json!({"op":"sphere","id":"s","radius":1.0})];
        let mut root = "s".to_owned();
        for (index, &x) in centres.iter().enumerate() {
            world
                .spawn_sphere(
                    Sphere::new(1.0).unwrap(),
                    Transform::new(v(x, 0.0, 0.0), 1.0).unwrap(),
                    0.0,
                )
                .unwrap();
            let moved = format!("t{index}");
            nodes.push(
                serde_json::json!({"op":"translate","id":moved,"child":"s","offset":[x,0.0,0.0]}),
            );
            if index == 0 {
                root = moved;
            } else {
                let joined = format!("u{index}");
                nodes.push(serde_json::json!({"op":"union","id":joined,"left":root,"right":moved}));
                root = joined;
            }
        }
        let source = serde_json::json!({"format_version":1,"id":"sweep","root":root,"nodes":nodes});
        let graph = compile_json(&serde_json::to_vec(&source).unwrap()).unwrap();
        let scene = ContactScene::from_world(&world).unwrap();
        let start = v(-3.0, 0.0, 0.0);
        let movement = v(8.0, 0.0, 0.0);
        let radius = 0.5;
        let SweepOutcome::Hit(contact) = scene.sweep(start, movement, radius).unwrap() else {
            panic!("analytic sweep should hit")
        };
        let mut distance = 0.0;
        for _ in 0..256 {
            let point = start.checked_add(v(distance, 0.0, 0.0)).unwrap();
            let inflated_value = graph.sample(point).unwrap() - radius;
            if inflated_value <= 1e-5 {
                break;
            }
            distance += inflated_value * 0.8;
        }
        assert!(
            (distance / 8.0 - contact.fraction).abs() < 1e-4,
            "centres={centres:?} field={distance} analytic={contact:?}"
        );
        // The supported exact primitive distances make min(F_i-r) an exterior step bound.
        assert!(graph.sample(start).unwrap() - radius > 0.0);
        let field_contact = sweep_sphere_exterior(
            &graph,
            start,
            movement,
            radius,
            SolidSweepOptions {
                tolerance: 1e-5,
                iterations: 256,
                safety: 0.8,
                max_work: 16_384,
            },
        )
        .unwrap();
        let SolidEntryOutcome::ApproximateEntry { distance } = field_contact.outcome else {
            panic!("expected approximate field contact: {field_contact:?}");
        };
        assert!((distance / 8.0 - contact.fraction).abs() < 1e-4);
    }
}
