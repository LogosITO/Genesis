//! Opt-in CPU baseline for renderer snapshot construction, not a CI timing gate.

use analytic_field::Sphere;
use analytic_renderer::{Bvh, Primitive, Scene};
use spatial_math::{Transform, Vec3};
use std::{hint::black_box, time::Instant};
use world_state::{DeterministicSeed, WorldState};

fn measure(mut run: impl FnMut(), samples: usize) -> (f64, f64) {
    for _ in 0..3 {
        run();
    }
    let mut ms = (0..samples)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed().as_secs_f64() * 1_000.0
        })
        .collect::<Vec<_>>();
    ms.sort_by(f64::total_cmp);
    (ms[samples / 2], ms[(samples * 95).div_ceil(100) - 1])
}

#[test]
#[ignore = "local release-profile CPU scaling baseline; run serially with --nocapture"]
fn scene_push_and_bvh_scaling() {
    for count in [128, 512, 2_048, 4_096] {
        let sphere = Sphere::new(0.1).unwrap();
        let primitives = (0..count)
            .map(|index| {
                Primitive::sphere(
                    index,
                    sphere,
                    Transform::new(Vec3::new(index as f64 * 0.002, 0.0, 0.0).unwrap(), 1.0)
                        .unwrap(),
                    [1.0, 1.0, 1.0],
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let build_scene = || {
            let mut scene = Scene::default();
            for primitive in &primitives {
                scene.push(*primitive).unwrap();
            }
            assert_eq!(scene.primitives().len(), count as usize);
            black_box(scene)
        };
        let push = measure(
            || {
                black_box(build_scene());
            },
            11,
        );
        let scene = build_scene();
        let bvh = measure(
            || {
                let bvh = Bvh::build(&scene).unwrap();
                assert!(bvh.node_count() > 0);
                black_box(bvh);
            },
            11,
        );
        println!(
            "QA0_CPU {{\"kind\":\"scene\",\"primitives\":{count},\"push_ms_p50_p95\":[{},{}],\"bvh_ms_p50_p95\":[{},{}]}}",
            push.0, push.1, bvh.0, bvh.1
        );
    }
}

#[test]
#[ignore = "local release-profile CPU world validation and snapshot baseline"]
fn world_validate_and_snapshot_scaling() {
    for count in [128, 512] {
        let mut world = WorldState::new(DeterministicSeed(7));
        let sphere = Sphere::new(0.1).unwrap();
        for index in 0..count {
            world
                .spawn_sphere(
                    sphere,
                    Transform::new(Vec3::new(index as f64 * 0.002, 0.0, 0.0).unwrap(), 1.0)
                        .unwrap(),
                    0.0,
                )
                .unwrap();
        }
        let validate = measure(
            || {
                world.validate().unwrap();
            },
            21,
        );
        let snapshot = measure(
            || {
                let scene = Scene::from_world(&world).unwrap();
                assert_eq!(scene.primitives().len(), count);
                black_box(scene);
            },
            21,
        );
        println!(
            "QA0_CPU {{\"kind\":\"world\",\"entities\":{count},\"validate_ms_p50_p95\":[{},{}],\"snapshot_ms_p50_p95\":[{},{}]}}",
            validate.0, validate.1, snapshot.0, snapshot.1
        );
    }
}
