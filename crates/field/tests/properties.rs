//! Bounded numerical properties supplement deterministic counterexamples in unit tests.

use analytic_field::{
    AnalyticGradient, AxisAlignedBox, Field, Gradient, LipschitzField, Ray, RayOptions, RayOutcome,
    ScalarField, Sphere, trace,
};
use proptest::prelude::*;
use spatial_math::{Transform, Vec3};

fn p(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0xF002_2026),
        ..ProptestConfig::default()
    })]

    #[test]
    fn csg_is_unit_lipschitz_and_has_expected_strict_signs(
        x in -5.0f64..5.0, y in -5.0f64..5.0, z in -5.0f64..5.0,
        u in -5.0f64..5.0, v in -5.0f64..5.0, w in -5.0f64..5.0,
        radius in 0.25f64..3.0, offset in -2.0f64..2.0,
    ) {
        let a = Field::Sphere(Sphere::new(radius).unwrap());
        let b = Field::transformed(
            Field::Box(AxisAlignedBox::new(p(1.0, 1.5, 2.0)).unwrap()),
            Transform::new(p(offset, 0.0, 0.0), 1.0).unwrap(),
        );
        let point = p(x, y, z);
        let other = p(u, v, w);
        let (av, bv) = (a.sample(point).unwrap(), b.sample(point).unwrap());
        let union = Field::union(a.clone(), b.clone());
        let intersection = Field::intersection(a.clone(), b.clone());
        let difference = Field::difference(a, b);
        prop_assert_eq!(union.sample(point).unwrap() < 0.0, av < 0.0 || bv < 0.0);
        prop_assert_eq!(intersection.sample(point).unwrap() < 0.0, av < 0.0 && bv < 0.0);
        prop_assert_eq!(difference.sample(point).unwrap() < 0.0, av < 0.0 && bv > 0.0);
        for field in [&union, &intersection, &difference] {
            let delta = (field.sample(point).unwrap() - field.sample(other).unwrap()).abs();
            let separation = point.checked_sub(other).unwrap().length().unwrap();
            prop_assert!(delta <= separation + 1e-12);
            prop_assert!(field.ideal_zero_set_bound(point).unwrap().value() >= 0.0);
        }
    }

    #[test]
    fn uniform_sphere_transform_preserves_ideal_distance(
        x in -3.0f64..3.0, y in -3.0f64..3.0, z in -3.0f64..3.0,
        tx in -2.0f64..2.0, scale in 0.25f64..3.0, radius in 0.25f64..2.0,
    ) {
        let point = p(x, y, z);
        let transform = Transform::new(p(tx, 0.0, 0.0), scale).unwrap();
        let sphere = Sphere::new(radius).unwrap();
        let expected = sphere.sample(transform.to_local(point).unwrap()).unwrap() * scale;
        let actual = sphere.sample_transformed(transform, point).unwrap();
        prop_assert!((actual - expected).abs() <= 1e-12);
    }

    #[test]
    fn sphere_gradient_matches_finite_difference_away_from_centre(
        x in -3.0f64..3.0, y in -3.0f64..3.0, z in -3.0f64..3.0,
    ) {
        let point = p(x, y, z);
        prop_assume!(point.length().unwrap() > 0.5);
        let sphere = Sphere::new(1.0).unwrap();
        let Gradient::Defined(gradient) = sphere.gradient(point).unwrap() else { panic!("gradient should exist") };
        let h = 1e-6;
        let finite_difference = (sphere.sample(p(x + h, y, z)).unwrap() - sphere.sample(p(x - h, y, z)).unwrap()) / (2.0 * h);
        prop_assert!((gradient.x() - finite_difference).abs() <= 1e-6);
    }

    #[test]
    fn box_gradient_matches_finite_difference_off_seams(
        x in 1.2f64..3.0, y in 1.2f64..3.0,
    ) {
        let cube = AxisAlignedBox::new(p(1.0, 1.0, 1.0)).unwrap();
        let Gradient::Defined(gradient) = cube.gradient(p(x, y, 0.2)).unwrap() else { panic!("gradient should exist") };
        let h = 1e-6;
        let finite_difference = (cube.sample(p(x + h, y, 0.2)).unwrap() - cube.sample(p(x - h, y, 0.2)).unwrap()) / (2.0 * h);
        prop_assert!((gradient.x() - finite_difference).abs() <= 1e-6);
    }

    #[test]
    fn exterior_sphere_rays_do_not_step_past_analytic_entry(
        radius in 0.5f64..2.0, gap in 1.0f64..3.0, fraction in -0.8f64..0.8,
    ) {
        let sphere = Sphere::new(radius).unwrap();
        let start = radius + gap;
        let y = fraction * radius;
        let entry = start - (radius * radius - y * y).sqrt();
        let ray = Ray::new(p(-start, y, 0.0), p(1.0, 0.0, 0.0), 2.0 * start).unwrap();
        let outcome = trace(&sphere, ray, RayOptions::new(1e-8, 128).unwrap());
        let RayOutcome::Hit { distance, point } = outcome else { panic!("expected hit, got {outcome:?}") };
        prop_assert!(distance <= entry + 1e-12);
        prop_assert!((entry - distance).abs() <= 5e-8);
        prop_assert!((0.0..=1e-8).contains(&sphere.sample(point).unwrap()));
    }

    #[test]
    fn contained_sphere_difference_has_no_negative_region(
        x in -4.0f64..4.0, y in -4.0f64..4.0, z in -4.0f64..4.0,
        radius in 0.25f64..2.0, extra in 0.25f64..2.0,
    ) {
        let inner = Field::Sphere(Sphere::new(radius).unwrap());
        let outer = Field::Sphere(Sphere::new(radius + extra).unwrap());
        prop_assert!(Field::difference(inner, outer).sample(p(x, y, z)).unwrap() >= 0.0);
    }
}
