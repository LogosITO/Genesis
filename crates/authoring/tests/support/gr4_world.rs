//! Research-only Field Graph queries derived from validated authoritative state.

use super::*;
use spatial_math::Transform;
use world_state::{
    DeterministicSeed, FieldDefinition, FieldInstance, ParameterBinding, ParameterComponent,
    ParameterRule, WorldError, WorldState,
};

const SPHERE_SOURCE: &[u8] = br#"{"format_version":1,"id":"gr4-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
const BOX_SOURCE: &[u8] = br#"{"format_version":1,"id":"gr4-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[1.0,0.5,0.25]}]}"#;
const BIG_SPHERE_SOURCE: &[u8] = br#"{"format_version":1,"id":"gr4-big-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":2.0}]}"#;
const UNION_SOURCE: &[u8] = br#"{"format_version":1,"id":"gr4-union","root":"u","nodes":[{"op":"sphere","id":"a","radius":1.0},{"op":"box","id":"b","half_extent":[0.5,0.5,0.5]},{"op":"translate","id":"t","child":"b","offset":[3.0,0.0,0.0]},{"op":"union","id":"u","left":"a","right":"t"}]}"#;
const SMOOTH_SOURCE: &[u8] = br#"{"format_version":1,"id":"gr4-smooth","root":"u","nodes":[{"op":"sphere","id":"a","radius":1.0},{"op":"sphere","id":"b","radius":1.0},{"op":"translate","id":"t","child":"b","offset":[2.0,0.0,0.0]},{"op":"smooth_union","id":"u","left":"a","right":"t","width":0.2}]}"#;
const SCALE_THEN_TRANSLATE: &[u8] = br#"{"format_version":1,"id":"gr4-st","root":"t","nodes":[{"op":"sphere","id":"s","radius":1.0},{"op":"scale","id":"q","child":"s","factor":3.0},{"op":"translate","id":"t","child":"q","offset":[2.0,0.0,0.0]}]}"#;
const TRANSLATE_THEN_SCALE: &[u8] = br#"{"format_version":1,"id":"gr4-ts","root":"q","nodes":[{"op":"sphere","id":"s","radius":1.0},{"op":"translate","id":"t","child":"s","offset":[2.0,0.0,0.0]},{"op":"scale","id":"q","child":"t","factor":3.0}]}"#;
const NESTED_BOX: &[u8] = br#"{"format_version":1,"id":"gr4-nested-box","root":"q2","nodes":[{"op":"box","id":"b","half_extent":[1.0,0.5,0.25]},{"op":"translate","id":"t","child":"b","offset":[2.0,0.0,0.0]},{"op":"scale","id":"q1","child":"t","factor":2.0},{"op":"scale","id":"q2","child":"q1","factor":0.5}]}"#;

#[derive(Debug, Eq, PartialEq)]
enum SnapshotIssue {
    InvalidWorld(WorldError),
    IdOutOfRange,
    MissingGraph,
    Stale,
}

// shortcut: this prototype covers enabled visual Field Graphs only; expand the domain
// before any whole-world API or collision integration.
struct FieldSnapshot {
    captured_instances: Vec<FieldInstance>,
    captured_definitions: Vec<FieldDefinition>,
    scene: Option<ResearchScene>,
}

impl FieldSnapshot {
    fn derive(world: &WorldState) -> Result<Self, SnapshotIssue> {
        world.validate().map_err(SnapshotIssue::InvalidWorld)?;
        let mut instances = Vec::new();
        for source in world
            .field_instances()
            .iter()
            .filter(|source| source.enabled())
        {
            let id = u32::try_from(source.id().value()).map_err(|_| SnapshotIssue::IdOutOfRange)?;
            let graph = world
                .field_graph(source.id())
                .ok_or(SnapshotIssue::MissingGraph)?;
            instances.push(instance(
                id,
                &Arc::new(graph.clone()),
                source.transform().translation(),
                source.transform().scale(),
            ));
        }
        Ok(Self {
            captured_instances: world.field_instances().to_vec(),
            captured_definitions: world.field_definitions().to_vec(),
            scene: (!instances.is_empty()).then(|| ResearchScene::new(instances)),
        })
    }

    fn is_current(&self, world: &WorldState) -> bool {
        self.captured_instances == world.field_instances()
            && self.captured_definitions == world.field_definitions()
    }

    fn query_snapshot(&self, ray: Ray, path: Path, budget: usize) -> NarrowOutcome {
        self.scene.as_ref().map_or(NarrowOutcome::Miss, |scene| {
            scene.query(ray, path, budget).outcome
        })
    }

    fn query_current(
        &self,
        world: &WorldState,
        ray: Ray,
        path: Path,
        budget: usize,
    ) -> Result<NarrowOutcome, SnapshotIssue> {
        world.validate().map_err(SnapshotIssue::InvalidWorld)?;
        if !self.is_current(world) {
            return Err(SnapshotIssue::Stale);
        }
        Ok(self.query_snapshot(ray, path, budget))
    }
}

fn transform(x: f64, scale: f64) -> Transform {
    Transform::new(v(x, 0.0, 0.0), scale).unwrap()
}

fn axis_ray(x: f64, far: f64) -> Ray {
    Ray::new(v(x, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, far).unwrap()
}

#[test]
fn transformed_oracles_and_ambiguous_ownership() {
    let mut world = WorldState::new(DeterministicSeed(4));
    let sphere = world
        .spawn_field(SPHERE_SOURCE, transform(4.0, 2.0))
        .unwrap();
    let box_id = world.spawn_field(BOX_SOURCE, transform(12.0, 0.5)).unwrap();
    let snapshot = FieldSnapshot::derive(&world).unwrap();
    // Independent axis-line formula: entry = center_x - scale * radius/half_extent_x - origin_x.
    for (origin, expected, id) in [(-1.0, 3.0, sphere), (9.0, 2.5, box_id)] {
        for path in [Path::Flat, Path::Hierarchy] {
            let NarrowOutcome::Hit(hit) =
                snapshot.query_snapshot(axis_ray(origin, 20.0), path, 256)
            else {
                panic!("transformed entry must qualify");
            };
            assert_eq!(u64::from(hit.instance_id), id.value());
            assert!(hit.parameter.contains(expected));
            assert!(hit.position[0].contains(origin + expected));
        }
    }
    let miss = Ray::new(v(-1.0, 3.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 20.0).unwrap();
    assert_eq!(
        snapshot.query_snapshot(miss, Path::Hierarchy, 256),
        NarrowOutcome::Miss
    );
    let mut negative = WorldState::new(DeterministicSeed(40));
    negative
        .spawn_field(SPHERE_SOURCE, transform(-4.0, 1.0))
        .unwrap();
    expect_hit(
        FieldSnapshot::derive(&negative).unwrap().query_snapshot(
            axis_ray(-7.0, 10.0),
            Path::Flat,
            256,
        ),
        0,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    let coincident = world
        .spawn_field(SPHERE_SOURCE, transform(4.0, 2.0))
        .unwrap();
    assert_ne!(sphere, coincident);
    let snapshot = FieldSnapshot::derive(&world).unwrap();
    let NarrowOutcome::HitAmbiguous {
        parameter,
        position,
    } = snapshot.query_snapshot(axis_ray(-1.0, 20.0), Path::Hierarchy, 256)
    else {
        panic!("common exterior boundary exists without a unique owner");
    };
    assert!(parameter.contains(3.0));
    assert!(position[0].contains(2.0));
}

#[test]
fn independent_off_axis_and_nested_transform_oracles() {
    // Analytical ray/sphere quadratic, independent of the interval implementation.
    let sphere_entry = |center_x: f64, radius: f64, y: f64, z: f64| {
        (center_x - (radius * radius - y * y - z * z).sqrt()) / 2.0
    };
    for (source, center_x) in [(SCALE_THEN_TRANSLATE, 11.0), (TRANSLATE_THEN_SCALE, 13.0)] {
        let mut world = WorldState::new(DeterministicSeed(10));
        world.spawn_field(source, transform(10.0, 0.5)).unwrap();
        let snapshot = FieldSnapshot::derive(&world).unwrap();
        for &(y, z) in &[(0.0, 0.0), (0.5, 0.25), (-0.25, 0.5)] {
            let ray = Ray::new(v(0.0, y, z), v(2.0, 0.0, 0.0), 0.0, 20.0).unwrap();
            let NarrowOutcome::Hit(hit) = snapshot.query_snapshot(ray, Path::Hierarchy, 256) else {
                panic!("off-axis transformed sphere must qualify");
            };
            let expected = sphere_entry(center_x, 1.5, y, z);
            assert!(
                hit.parameter.contains(expected),
                "expected {expected}, got {hit:?}"
            );
            assert!(hit.position[0].contains(2.0 * expected));
        }
    }
    let mut world = WorldState::new(DeterministicSeed(11));
    world.spawn_field(NESTED_BOX, transform(10.0, 2.0)).unwrap();
    let snapshot = FieldSnapshot::derive(&world).unwrap();
    // Nested graph maps the box center to x=2 and half extents remain [1,.5,.25];
    // instance scale two maps center to 14 and half extents to [2,1,.5].
    for &(y, z, normal) in &[(0.0, 0.0, true), (1.0, 0.0, false), (1.0, 0.5, false)] {
        let ray = Ray::new(v(0.0, y, z), v(2.0, 0.0, 0.0), 0.0, 20.0).unwrap();
        let outcome = snapshot.query_snapshot(ray, Path::Hierarchy, 256);
        if normal {
            expect_hit(outcome, 0, 6.0, Some([-1.0, 0.0, 0.0]));
        } else {
            // A ray exactly on a parallel face is deliberately unresolved by GR-3.
            assert!(matches!(
                outcome,
                NarrowOutcome::Indeterminate(NarrowIssue::BoundaryOrTangent)
            ));
        }
    }
}

#[test]
fn interval_edge_cases_do_not_create_certainty() {
    assert!(Enclosure::point(f64::NAN).is_none());
    assert!(Enclosure::point(f64::INFINITY).is_none());
    let one = Enclosure::point(1.0).unwrap();
    let signed_zero = Enclosure { lo: -0.0, hi: 0.0 };
    assert!(one.div(signed_zero).is_none());
    assert!(one.div(Enclosure { lo: -1.0, hi: 1.0 }).is_none());
    assert!(
        Enclosure::point(f64::MAX)
            .unwrap()
            .mul(one.add(one).unwrap())
            .is_none()
    );
    let tiny = Enclosure::point(f64::from_bits(1)).unwrap();
    assert!(tiny.mul(tiny).unwrap().contains(0.0));
    assert!(Enclosure { lo: -1.0, hi: 1.0 }.sqrt().is_none());
}

#[test]
fn authoritative_edits_never_reuse_stale_geometry() {
    let mut world = WorldState::new(DeterministicSeed(5));
    let id = world
        .spawn_field(SPHERE_SOURCE, Transform::identity())
        .unwrap();
    let first = FieldSnapshot::derive(&world).unwrap();
    let ray = axis_ray(-3.0, 10.0);
    let unrebuilt: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    assert!(matches!(
        first.query_current(&unrebuilt, ray, Path::Hierarchy, 256),
        Err(SnapshotIssue::InvalidWorld(_))
    ));
    expect_hit(
        first
            .query_current(&world, ray, Path::Hierarchy, 256)
            .unwrap(),
        0,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    assert!(world.replace_field(id, b"{}").is_err());
    assert!(first.is_current(&world));
    world.set_field_transform(id, transform(5.0, 1.0)).unwrap();
    assert_eq!(
        first.query_current(&world, ray, Path::Hierarchy, 256),
        Err(SnapshotIssue::Stale)
    );
    expect_hit(
        first.query_snapshot(ray, Path::Hierarchy, 256),
        0,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    let moved = FieldSnapshot::derive(&world).unwrap();
    expect_hit(
        moved
            .query_current(&world, ray, Path::Hierarchy, 256)
            .unwrap(),
        0,
        7.0,
        Some([-1.0, 0.0, 0.0]),
    );
    world.replace_field(id, BIG_SPHERE_SOURCE).unwrap();
    assert!(!moved.is_current(&world));
    let bigger = FieldSnapshot::derive(&world).unwrap();
    expect_hit(
        bigger.query_snapshot(ray, Path::Flat, 256),
        0,
        6.0,
        Some([-1.0, 0.0, 0.0]),
    );
    world.set_field_enabled(id, false).unwrap();
    assert_eq!(
        bigger.query_current(&world, ray, Path::Flat, 256),
        Err(SnapshotIssue::Stale)
    );
    assert_eq!(
        FieldSnapshot::derive(&world)
            .unwrap()
            .query_snapshot(ray, Path::Hierarchy, 256),
        NarrowOutcome::Miss
    );
    world.remove_field(id).unwrap();
    let replacement = world
        .spawn_field(SPHERE_SOURCE, Transform::identity())
        .unwrap();
    assert_ne!(id, replacement);
    assert!(!bigger.is_current(&world));
}

#[test]
fn unsupported_graph_is_never_certified_empty() {
    let mut world = WorldState::new(DeterministicSeed(6));
    world
        .spawn_field(SMOOTH_SOURCE, Transform::identity())
        .unwrap();
    let snapshot = FieldSnapshot::derive(&world).unwrap();
    assert!(matches!(
        snapshot.query_snapshot(axis_ray(-3.0, 10.0), Path::Hierarchy, 256),
        NarrowOutcome::Indeterminate(_)
    ));
    assert_eq!(
        snapshot.query_snapshot(axis_ray(-3.0, 10.0), Path::Hierarchy, 0),
        NarrowOutcome::Indeterminate(NarrowIssue::Broadphase(Issue::WorkBudget))
    );
}

#[test]
fn reset_and_unrepresentable_world_ids_are_explicit() {
    let mut world = WorldState::new(DeterministicSeed(12));
    world
        .spawn_field(SPHERE_SOURCE, Transform::identity())
        .unwrap();
    let snapshot = FieldSnapshot::derive(&world).unwrap();
    let ray = axis_ray(-3.0, 10.0);
    world = WorldState::new(DeterministicSeed(12));
    assert_eq!(
        snapshot.query_current(&world, ray, Path::Hierarchy, 256),
        Err(SnapshotIssue::Stale)
    );
    world
        .spawn_field(SPHERE_SOURCE, Transform::identity())
        .unwrap();
    // Counterexample: equal field bytes/IDs after reset cannot prove world lineage.
    assert!(snapshot.is_current(&world));
    world = WorldState::new(DeterministicSeed(12));
    let mut encoded = serde_json::to_value(&world).unwrap();
    encoded["next_id"] = serde_json::json!(u64::from(u32::MAX) + 1);
    let mut world: WorldState = serde_json::from_value(encoded).unwrap();
    world
        .spawn_field(SPHERE_SOURCE, Transform::identity())
        .unwrap();
    world.validate().unwrap();
    assert!(matches!(
        FieldSnapshot::derive(&world),
        Err(SnapshotIssue::IdOutOfRange)
    ));
}

#[test]
fn hard_union_and_maximum_valid_instance_count() {
    let mut world = WorldState::new(DeterministicSeed(7));
    let union = world
        .spawn_field(UNION_SOURCE, Transform::identity())
        .unwrap();
    for index in 1..world_state::MAX_FIELD_INSTANCES {
        world
            .spawn_field(SPHERE_SOURCE, transform(index as f64 * 10.0, 1.0))
            .unwrap();
    }
    assert!(
        world
            .spawn_field(SPHERE_SOURCE, Transform::identity())
            .is_err()
    );
    let snapshot = FieldSnapshot::derive(&world).unwrap();
    let outcome = snapshot.query_snapshot(axis_ray(-3.0, 20.0), Path::Hierarchy, 512);
    let NarrowOutcome::Hit(hit) = outcome else {
        panic!("first union entry must qualify");
    };
    assert_eq!(u64::from(hit.instance_id), union.value());
    assert_eq!(hit.ordinal, 0);
    assert!(hit.parameter.contains(2.0));
    assert_eq!(
        outcome,
        snapshot.query_snapshot(axis_ray(-3.0, 20.0), Path::Flat, 512)
    );
}

#[test]
fn parameter_growth_and_restore_refresh_the_snapshot() {
    let mut world = WorldState::new(DeterministicSeed(8));
    let id = world
        .spawn_field(SPHERE_SOURCE, Transform::identity())
        .unwrap();
    world
        .enable_field_parameters(
            id,
            vec![ParameterRule {
                binding: ParameterBinding {
                    node: "s".into(),
                    component: ParameterComponent::SphereRadius,
                },
                uptake: 1.0,
                gain_per_unit: 0.2,
                maximum: 2.0,
                positive_site: true,
                feedback: true,
            }],
        )
        .unwrap();
    let ray = axis_ray(-3.0, 10.0);
    let initial = FieldSnapshot::derive(&world).unwrap();
    expect_hit(
        initial.query_snapshot(ray, Path::Flat, 256),
        0,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    for step in 1..=3 {
        world.grow_field_parameters(id, &[0.5]).unwrap();
        assert_eq!(
            initial.query_current(&world, ray, Path::Hierarchy, 256),
            Err(SnapshotIssue::Stale)
        );
        let fresh = FieldSnapshot::derive(&world).unwrap();
        let NarrowOutcome::Hit(hit) = fresh.query_snapshot(ray, Path::Hierarchy, 256) else {
            panic!("current radius must yield a qualified entry");
        };
        assert!(hit.parameter.contains(2.0 - step as f64 * 0.1));
        assert_eq!(
            fresh.query_snapshot(ray, Path::Flat, 256),
            fresh.query_snapshot(ray, Path::Hierarchy, 256)
        );
    }
    let before_invalid = world.clone();
    assert!(world.grow_field_parameters(id, &[f64::NAN]).is_err());
    assert_eq!(world, before_invalid);

    let saved = serde_json::to_vec(&world).unwrap();
    let mut restored: WorldState = serde_json::from_slice(&saved).unwrap();
    restored.rebuild_fields().unwrap();
    restored.validate().unwrap();
    let snapshot = FieldSnapshot::derive(&restored).unwrap();
    assert_eq!(
        snapshot.query_snapshot(ray, Path::Flat, 256),
        FieldSnapshot::derive(&world)
            .unwrap()
            .query_snapshot(ray, Path::Flat, 256)
    );
    assert!(snapshot.is_current(&world));
    restored.grow_field_parameters(id, &[0.5]).unwrap();
    assert_eq!(
        snapshot.query_current(&restored, ray, Path::Hierarchy, 256),
        Err(SnapshotIssue::Stale)
    );
}

#[test]
#[ignore = "GR-4 release-profile valid-WorldState lifecycle benchmark"]
fn benchmark_valid_world_lifecycle() {
    for &(count, spacing, dense) in &[
        (1, 3.0, false),
        (4, 3.0, false),
        (8, 3.0, false),
        (8, 0.0, false),
        (8, 0.0, true),
    ] {
        let mut world = WorldState::new(DeterministicSeed(0x5eed_2028));
        for index in 0..count {
            world
                .spawn_field(SPHERE_SOURCE, transform(index as f64 * spacing, 1.0))
                .unwrap();
        }
        let mut validation = Vec::new();
        let mut construction = Vec::new();
        let mut index_build = Vec::new();
        for _ in 0..7 {
            let started = Instant::now();
            world.validate().unwrap();
            validation.push(started.elapsed().as_secs_f64() * 1_000.0);
            let started = Instant::now();
            let snapshot = FieldSnapshot::derive(&world).unwrap();
            construction.push(started.elapsed().as_secs_f64() * 1_000.0);
            let scene = snapshot.scene.as_ref().unwrap();
            let started = Instant::now();
            black_box(Hierarchy::build(&scene.broad));
            index_build.push(started.elapsed().as_secs_f64() * 1_000.0);
        }
        validation.sort_by(f64::total_cmp);
        construction.sort_by(f64::total_cmp);
        index_build.sort_by(f64::total_cmp);
        let snapshot = FieldSnapshot::derive(&world).unwrap();
        let scene = snapshot.scene.as_ref().unwrap();
        let rays: Vec<_> = (0..1024)
            .map(|index| {
                let y = if dense { 0.0 } else { (index % 8) as f64 * 0.5 };
                Ray::new(v(-3.0, y, 0.0), v(1.0, 0.0, 0.0), 0.0, 30.0).unwrap()
            })
            .collect();
        let mut paths = Vec::new();
        for (label, path) in [("flat", Path::Flat), ("hierarchy", Path::Hierarchy)] {
            let mut batches = Vec::new();
            let mut per_query = Vec::new();
            let mut counts = [0usize; 4];
            let mut candidates = 0usize;
            let mut shape_tests = 0usize;
            for repetition in 0..7 {
                let batch = Instant::now();
                for &ray in &rays {
                    let started = Instant::now();
                    let record = scene.query(ray, path, 512);
                    per_query.push(started.elapsed().as_nanos() as f64);
                    if repetition == 0 {
                        let class = match record.outcome {
                            NarrowOutcome::Miss => 0,
                            NarrowOutcome::Hit(_) => 1,
                            NarrowOutcome::HitAmbiguous { .. } => 2,
                            NarrowOutcome::Indeterminate(_) => 3,
                        };
                        counts[class] += 1;
                        candidates += record.candidate_count;
                        shape_tests += record.shape_tests;
                    }
                    black_box(record.outcome);
                }
                batches.push(batch.elapsed().as_secs_f64() * 1_000.0);
            }
            batches.sort_by(f64::total_cmp);
            per_query.sort_by(f64::total_cmp);
            paths.push(serde_json::json!({
                "method":label,"batch_median_ms":batches[3],
                "query_p50_ns":per_query[per_query.len()/2],
                "query_p95_ns":per_query[per_query.len()*95/100],
                "miss":counts[0],"unique_hit":counts[1],"ambiguous_hit":counts[2],
                "indeterminate":counts[3],"candidates":candidates,"shape_tests":shape_tests
            }));
        }
        let mut current_batches = Vec::new();
        for _ in 0..7 {
            let started = Instant::now();
            for &ray in &rays {
                black_box(
                    snapshot
                        .query_current(&world, ray, Path::Hierarchy, 512)
                        .unwrap(),
                );
            }
            current_batches.push(started.elapsed().as_secs_f64() * 1_000.0);
        }
        current_batches.sort_by(f64::total_cmp);
        let old = FieldSnapshot::derive(&world).unwrap();
        let first_id = world.field_instances()[0].id();
        let started = Instant::now();
        world
            .set_field_transform(first_id, transform(50.0, 1.0))
            .unwrap();
        let edit_us = started.elapsed().as_secs_f64() * 1_000_000.0;
        let stale = !old.is_current(&world);
        let started = Instant::now();
        let changed = FieldSnapshot::derive(&world).unwrap();
        let refresh_us = started.elapsed().as_secs_f64() * 1_000_000.0;
        assert!(stale && changed.is_current(&world));
        println!(
            "GR4_WORLD_BENCH {}",
            serde_json::json!({
                "instances":count,"spacing":spacing,"dense":dense,"rays":rays.len(),
                "repetitions":7,"validation_median_ms":validation[3],
                "snapshot_median_ms":construction[3],"index_build_median_ms":index_build[3],
                "edit_us":edit_us,"refresh_us":refresh_us,"stale_detected":stale,
                "current_hierarchy_batch_median_ms":current_batches[3],
                "nodes":scene.index.nodes.len(),"terms":scene.shapes.iter().flatten().map(Vec::len).sum::<usize>(),
                "query_storage_lower_bound_bytes":scene.broad.leaves.len()*size_of::<Leaf>()
                    + scene.index.nodes.len()*size_of::<super::super::Node>()
                    + scene.shapes.iter().flatten().map(Vec::len).sum::<usize>()*size_of::<Term>(),
                "paths":paths
            })
        );
    }
}
