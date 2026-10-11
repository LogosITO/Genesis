//! Independent axis-aligned ray references for the analytic leaf path.
//! These do not certify Field Graph hit/normal arithmetic or the GR-2 broadphase.

use analytic_field::{AxisAlignedBox, Capsule, Sphere};
use analytic_renderer::{Bvh, Primitive, Ray, Scene};
use spatial_math::{Transform, Vec3};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

fn check(primitive: Primitive, origin: Vec3, expected_t: f64, expected_normal: Vec3) {
    let mut scene = Scene::default();
    scene.push(primitive).unwrap();
    let ray = Ray::new(origin, v(1.0, 0.0, 0.0), 0.0, expected_t + 2.0).unwrap();
    let direct = scene.intersect(ray).expect("analytic axis ray must hit");
    let bvh = Bvh::build(&scene)
        .unwrap()
        .intersect(ray)
        .expect("BVH must retain hit");
    assert_eq!(direct.id, primitive.id);
    assert_eq!(bvh.id, direct.id);
    assert_eq!(bvh.distance, direct.distance);
    assert_eq!(bvh.normal, direct.normal);
    assert!((direct.distance - expected_t).abs() <= 1e-11, "{direct:?}");
    assert_eq!(direct.normal, expected_normal);
}

#[test]
fn sphere_box_and_capsule_axis_oracles() {
    let color = [1.0; 3];
    let sphere =
        Primitive::sphere(1, Sphere::new(1.0).unwrap(), Transform::identity(), color).unwrap();
    check(sphere, v(-3.0, 0.0, 0.0), 2.0, v(-1.0, 0.0, 0.0));
    check(sphere, Vec3::ZERO, 1.0, v(1.0, 0.0, 0.0));
    check(sphere, v(-3.0, 1.0, 0.0), 3.0, v(0.0, 1.0, 0.0));
    let box_primitive = Primitive::axis_aligned_box(
        2,
        AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap(),
        Transform::new(v(4.0, 0.0, 0.0), 1.0).unwrap(),
        color,
    )
    .unwrap();
    check(box_primitive, Vec3::ZERO, 3.0, v(-1.0, 0.0, 0.0));
    check(box_primitive, v(4.0, 0.0, 0.0), 1.0, v(1.0, 0.0, 0.0));
    let capsule = Primitive::capsule(
        3,
        Capsule::new(v(8.0, 0.0, -1.0), v(8.0, 0.0, 1.0), 0.5).unwrap(),
        color,
    )
    .unwrap();
    check(capsule, v(6.0, 0.0, 0.0), 1.5, v(-1.0, 0.0, 0.0));
    check(capsule, v(8.0, 0.0, 0.0), 0.5, v(1.0, 0.0, 0.0));
    check(capsule, v(6.0, 0.5, 0.0), 2.0, v(0.0, 1.0, 0.0));
    let distant = Primitive::sphere(
        4,
        Sphere::new(1.0).unwrap(),
        Transform::new(v(9000.0, 0.0, 0.0), 1.0).unwrap(),
        color,
    )
    .unwrap();
    check(distant, v(8997.0, 0.0, 0.0), 2.0, v(-1.0, 0.0, 0.0));
}

#[test]
fn miss_overlap_and_nearly_coincident_order() {
    let mut scene = Scene::default();
    scene
        .push(
            Primitive::sphere(
                1,
                Sphere::new(1.0).unwrap(),
                Transform::identity(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .push(
            Primitive::sphere(
                2,
                Sphere::new(1.0).unwrap(),
                Transform::new(v(1e-9, 0.0, 0.0), 1.0).unwrap(),
                [1.0; 3],
            )
            .unwrap(),
        )
        .unwrap();
    let hit_ray = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    let direct = scene.intersect(hit_ray).unwrap();
    let bvh = Bvh::build(&scene).unwrap().intersect(hit_ray).unwrap();
    assert_eq!(direct.id, 1);
    assert_eq!(bvh.id, 1);
    assert!((direct.distance - 2.0).abs() <= 1e-12);
    let miss = Ray::new(v(-3.0, 2.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert!(scene.intersect(miss).is_none());
    assert!(Bvh::build(&scene).unwrap().intersect(miss).is_none());
}
