//! Renderer and contact snapshots of authoritative authored capsules.
use analytic_renderer::{PickOutcome, Ray, Scene, SemanticTarget};
use spatial_math::{Transform, Vec3};
use world_simulation::contact::{ContactScene, SweepOutcome};
use world_state::{ColliderId, DeterministicSeed, WorldState};

const SOURCE: &[u8] = include_bytes!("../world-single.json");
fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}
fn ray(x: f64) -> Ray {
    Ray::new(v(x, 0.5, -2.0), v(0.0, 0.0, 1.0), 0.0, 10.0).unwrap()
}

#[test]
fn picking_and_contact_track_two_world_identities_and_updates() {
    let mut world = WorldState::new(DeterministicSeed(9));
    let first = world
        .spawn_authored(SOURCE, Transform::identity(), true)
        .unwrap();
    let second = world
        .spawn_authored(SOURCE, Transform::new(v(3.0, 0.0, 0.0), 1.0).unwrap(), true)
        .unwrap();
    let scene = Scene::from_world(&world).unwrap();
    let selected = match scene.pick_ray(ray(0.0)).unwrap() {
        PickOutcome::Hit(hit) => match hit.target {
            SemanticTarget::Authored(reference) => reference,
            _ => panic!("wrong target"),
        },
        other => panic!("unexpected pick: {other:?}"),
    };
    assert_eq!(selected.instance, first);
    assert!(
        matches!(scene.pick_ray(ray(3.0)).unwrap(), PickOutcome::Hit(hit)
        if matches!(hit.target, SemanticTarget::Authored(reference) if reference.instance == second && reference.segment == selected.segment))
    );
    let mut contact = ContactScene::from_world(&world).unwrap();
    let sweep = |contact: &ContactScene, x| {
        contact
            .sweep(v(x, 0.5, -2.0), v(0.0, 0.0, 4.0), 0.1)
            .unwrap()
    };
    assert_eq!(contact.solid_count(), 2);
    assert_eq!(
        sweep(&contact, 0.0),
        contact
            .sweep_direct(v(0.0, 0.5, -2.0), v(0.0, 0.0, 4.0), 0.1)
            .unwrap()
    );
    assert!(
        matches!(sweep(&contact, 0.0), SweepOutcome::Hit(hit) if hit.collider == ColliderId::Authored(selected))
    );
    assert!(!contact.refresh(&world).unwrap());
    world
        .set_authored_transform(second, Transform::new(v(4.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    assert!(contact.refresh(&world).unwrap());
    assert!(matches!(sweep(&contact, 3.0), SweepOutcome::Miss));
    assert!(matches!(sweep(&contact, 4.0), SweepOutcome::Hit(_)));
    world.set_authored_enabled(first, false).unwrap();
    assert!(world.authored_segment(selected).is_none());
    assert!(contact.refresh(&world).unwrap());
    assert_eq!(contact.solid_count(), 1);
    assert!(matches!(sweep(&contact, 0.0), SweepOutcome::Miss));
    assert!(matches!(
        Scene::from_world(&world)
            .unwrap()
            .pick_ray(ray(0.0))
            .unwrap(),
        PickOutcome::Miss
    ));
    world.remove_authored(second).unwrap();
    assert!(contact.refresh(&world).unwrap());
    assert_eq!(contact.solid_count(), 0);
    assert_eq!(Scene::from_world(&world).unwrap().primitives().len(), 0);
}

#[test]
fn non_solid_occurrence_renders_without_contact() {
    let mut world = WorldState::new(DeterministicSeed(10));
    let id = world
        .spawn_authored(SOURCE, Transform::identity(), false)
        .unwrap();
    assert!(
        matches!(Scene::from_world(&world).unwrap().pick_ray(ray(0.0)).unwrap(),
        PickOutcome::Hit(hit) if matches!(hit.target, SemanticTarget::Authored(reference) if reference.instance == id))
    );
    let contact = ContactScene::from_world(&world).unwrap();
    assert_eq!(contact.solid_count(), 0);
    assert!(matches!(
        contact
            .sweep(v(0.0, 0.5, -2.0), v(0.0, 0.0, 4.0), 0.1)
            .unwrap(),
        SweepOutcome::Miss
    ));
}

#[test]
#[ignore = "local CPU timing experiment; run explicitly with --nocapture"]
fn authored_world_cpu_measurements() {
    use std::time::Instant;
    use world_runtime::Runtime;
    use world_simulation::SimulationStep;
    fn median(mut samples: Vec<f64>) -> f64 {
        samples.sort_by(f64::total_cmp);
        samples[samples.len() / 2]
    }
    let cases: [(&str, &[u8]); 2] = [
        ("single", SOURCE),
        ("branch-a", include_bytes!("../branch-a.json")),
    ];
    for (name, source) in cases {
        for count in [1usize, 2, 8] {
            let mut compile = Vec::new();
            let mut first_spawn = Vec::new();
            let mut reused_spawn = Vec::new();
            let mut transform = Vec::new();
            let mut snapshot = Vec::new();
            let mut render_bvh = Vec::new();
            let mut contact_bvh = Vec::new();
            let mut save = Vec::new();
            let mut load = Vec::new();
            let mut segments = 0;
            let mut save_bytes = 0;
            for _ in 0..30 {
                let start = Instant::now();
                std::hint::black_box(world_authoring::compile_json(source).unwrap());
                compile.push(start.elapsed().as_secs_f64() * 1e3);
                let mut world = WorldState::new(DeterministicSeed(17));
                let start = Instant::now();
                let first = world
                    .spawn_authored(source, Transform::identity(), true)
                    .unwrap();
                first_spawn.push(start.elapsed().as_secs_f64() * 1e3);
                let start = Instant::now();
                for i in 1..count {
                    world
                        .spawn_authored(
                            source,
                            Transform::new(v(i as f64 * 3.0, 0.0, 0.0), 1.0).unwrap(),
                            true,
                        )
                        .unwrap();
                }
                reused_spawn.push(start.elapsed().as_secs_f64() * 1e3);
                segments = world.authored_capsules().unwrap().len();
                let start = Instant::now();
                world
                    .set_authored_transform(first, Transform::new(v(0.5, 0.0, 0.0), 1.0).unwrap())
                    .unwrap();
                transform.push(start.elapsed().as_secs_f64() * 1e3);
                let start = Instant::now();
                let scene = Scene::from_world(&world).unwrap();
                snapshot.push(start.elapsed().as_secs_f64() * 1e3);
                let start = Instant::now();
                std::hint::black_box(analytic_renderer::Bvh::build(&scene).unwrap());
                render_bvh.push(start.elapsed().as_secs_f64() * 1e3);
                let start = Instant::now();
                std::hint::black_box(ContactScene::from_world(&world).unwrap());
                contact_bvh.push(start.elapsed().as_secs_f64() * 1e3);
                let runtime = Runtime::new(world, SimulationStep::new(0.01).unwrap());
                let start = Instant::now();
                let bytes = runtime.save_bytes().unwrap();
                save.push(start.elapsed().as_secs_f64() * 1e3);
                save_bytes = bytes.len();
                let start = Instant::now();
                std::hint::black_box(Runtime::load_bytes(&bytes).unwrap());
                load.push(start.elapsed().as_secs_f64() * 1e3);
            }
            println!(
                "{}",
                serde_json::json!({
                    "kind": "authored-world-cpu-measurement", "definition": name,
                    "instances": count, "segments": segments, "definition_records": 1,
                    "source_bytes": source.len(), "save_bytes": save_bytes,
                    "samples": 30, "compile_ms_p50": median(compile),
                    "first_spawn_ms_p50": median(first_spawn),
                    "reused_spawn_total_ms_p50": median(reused_spawn),
                    "transform_ms_p50": median(transform),
                    "snapshot_ms_p50": median(snapshot),
                    "render_bvh_ms_p50": median(render_bvh),
                    "contact_bvh_ms_p50": median(contact_bvh),
                    "save_ms_p50": median(save), "load_ms_p50": median(load),
                })
            );
        }
    }
}
