//! GR-2 research-only broadphase. It certifies exclusion of the ideal occupied
//! set under binary64 round-to-nearest; it never certifies a field-sample sign.

use spatial_math::Vec3;
use std::{hint::black_box, mem::size_of, sync::Arc, time::Instant};
use world_authoring::field_graph::{Graph, Node as GraphNode, compile_json};

#[path = "support/gr3_narrowphase.rs"]
mod gr3_narrowphase;

const ARCH: &[u8] = include_bytes!("../../../examples/field-graph/definitions/gr1-arch.json");
const SPHERE: &[u8] =
    br#"{"format_version":1,"id":"gr2-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
const LARGE_SPHERE: &[u8] =
    br#"{"format_version":1,"id":"gr2-large-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":2.0}]}"#;
const BOX: &[u8] = br#"{"format_version":1,"id":"gr2-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[1.0,1.0,1.0]}]}"#;
const NESTED: &[u8] = br#"{"format_version":1,"id":"gr2-nested","root":"t","nodes":[{"op":"sphere","id":"s","radius":1.0},{"op":"scale","id":"q","child":"s","factor":2.0},{"op":"translate","id":"t","child":"q","offset":[3.0,0.0,0.0]}]}"#;

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

// IEEE-754 basic +, -, and * are correctly rounded in the tested CPU model.
// One adjacent representable value on each side encloses each exact operation.
fn rounded(a: f64, b: f64, multiply: bool, upper: bool) -> Option<f64> {
    let value = if multiply { a * b } else { a + b };
    if !value.is_finite() {
        return None;
    }
    let bound = if upper {
        value.next_up()
    } else {
        value.next_down()
    };
    bound.is_finite().then_some(bound)
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn graph(graph: &Graph, translation: Vec3, scale: f64) -> Option<Self> {
        if !scale.is_finite()
            || scale <= 0.0
            || graph
                .nodes()
                .iter()
                .any(|node| matches!(node, GraphNode::SmoothUnion { .. }))
        {
            return None;
        }
        let local = graph.bounds();
        let low = [local.min.x(), local.min.y(), local.min.z()];
        let high = [local.max.x(), local.max.y(), local.max.z()];
        let offset = [translation.x(), translation.y(), translation.z()];
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for axis in 0..3 {
            min[axis] = rounded(
                rounded(low[axis], scale, true, false)?,
                offset[axis],
                false,
                false,
            )?;
            max[axis] = rounded(
                rounded(high[axis], scale, true, true)?,
                offset[axis],
                false,
                true,
            )?;
        }
        Some(Self { min, max })
    }

    fn capsule(a: Vec3, b: Vec3, radius: f64) -> Option<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        let a = [a.x(), a.y(), a.z()];
        let b = [b.x(), b.y(), b.z()];
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for axis in 0..3 {
            min[axis] = rounded(a[axis].min(b[axis]), -radius, false, false)?;
            max[axis] = rounded(a[axis].max(b[axis]), radius, false, true)?;
        }
        Some(Self { min, max })
    }

    fn union(self, other: Self) -> Self {
        let mut result = self;
        for axis in 0..3 {
            result.min[axis] = result.min[axis].min(other.min[axis]);
            result.max[axis] = result.max[axis].max(other.max[axis]);
        }
        result
    }

    fn ray(self, ray: Ray) -> RegionResult {
        let origin = [ray.origin.x(), ray.origin.y(), ray.origin.z()];
        let direction = [ray.direction.x(), ray.direction.y(), ray.direction.z()];
        let mut uncertain = false;
        for axis in 0..3 {
            let endpoint = |t: f64, upper: bool| {
                rounded(direction[axis], t, true, upper)
                    .and_then(|product| rounded(origin[axis], product, false, upper))
            };
            match (
                endpoint(ray.near, false),
                endpoint(ray.near, true),
                endpoint(ray.far, false),
                endpoint(ray.far, true),
            ) {
                (Some(a_lo), Some(a_hi), Some(b_lo), Some(b_hi)) => {
                    if a_hi.max(b_hi) < self.min[axis] || a_lo.min(b_lo) > self.max[axis] {
                        return RegionResult::OutsideCertified;
                    }
                }
                _ => uncertain = true,
            }
        }
        if uncertain {
            RegionResult::Indeterminate
        } else {
            RegionResult::Candidate
        }
    }
}

#[derive(Clone, Copy)]
struct Ray {
    origin: Vec3,
    direction: Vec3,
    near: f64,
    far: f64,
}

impl Ray {
    fn new(origin: Vec3, direction: Vec3, near: f64, far: f64) -> Option<Self> {
        if !near.is_finite()
            || !far.is_finite()
            || near < 0.0
            || far <= near
            || direction == Vec3::ZERO
        {
            return None;
        }
        Some(Self {
            origin,
            direction,
            near,
            far,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegionResult {
    OutsideCertified,
    Candidate,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Outcome {
    OutsideCertified,
    Candidates(Vec<u32>),
    Indeterminate(Issue),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Issue {
    NumericOrUnsupported,
    WorkBudget,
}

struct Leaf {
    id: u32,
    bounds: Option<Bounds>,
}

struct Scene {
    leaves: Vec<Leaf>,
    generation: u64,
}

impl Scene {
    fn new(leaves: Vec<Leaf>) -> Self {
        assert!(!leaves.is_empty() && leaves.len() <= 128);
        let mut ids: Vec<u32> = leaves.iter().map(|leaf| leaf.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), leaves.len());
        Self {
            leaves,
            generation: 0,
        }
    }

    fn replace(&mut self, index: usize, leaf: Leaf) {
        assert_eq!(self.leaves[index].id, leaf.id);
        self.leaves[index] = leaf;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("research generation exhausted");
    }
}

enum Node {
    Leaf {
        bounds: Option<Bounds>,
        index: usize,
    },
    Branch {
        bounds: Option<Bounds>,
        left: usize,
        right: usize,
    },
}

impl Node {
    fn bounds(&self) -> Option<Bounds> {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }
}

struct Hierarchy {
    nodes: Vec<Node>,
    root: usize,
    generation: u64,
}

#[derive(Debug)]
struct Report {
    outcome: Outcome,
    node_tests: usize,
    leaf_tests: usize,
    field_evaluations: usize,
    fallback: bool,
}

fn finish(
    mut candidates: Vec<u32>,
    node_tests: usize,
    leaf_tests: usize,
    fallback: bool,
) -> Report {
    candidates.sort_unstable();
    Report {
        outcome: if candidates.is_empty() {
            Outcome::OutsideCertified
        } else {
            Outcome::Candidates(candidates)
        },
        node_tests,
        leaf_tests,
        field_evaluations: 0,
        fallback,
    }
}

fn indeterminate(issue: Issue, node_tests: usize, leaf_tests: usize, fallback: bool) -> Report {
    Report {
        outcome: Outcome::Indeterminate(issue),
        node_tests,
        leaf_tests,
        field_evaluations: 0,
        fallback,
    }
}

fn flat(scene: &Scene, ray: Ray, budget: usize, fallback: bool) -> Report {
    let mut candidates = Vec::new();
    for (index, leaf) in scene.leaves.iter().enumerate() {
        if index == budget {
            return indeterminate(Issue::WorkBudget, index, index, fallback);
        }
        match leaf.bounds.map(|bounds| bounds.ray(ray)) {
            Some(RegionResult::OutsideCertified) => {}
            Some(RegionResult::Candidate) => candidates.push(leaf.id),
            _ => return indeterminate(Issue::NumericOrUnsupported, index + 1, index + 1, fallback),
        }
    }
    finish(candidates, scene.leaves.len(), scene.leaves.len(), fallback)
}

impl Hierarchy {
    fn build(scene: &Scene) -> Self {
        let mut nodes = Vec::with_capacity(scene.leaves.len() * 2 - 1);
        let mut indices: Vec<usize> = (0..scene.leaves.len()).collect();
        let root = Self::build_range(scene, &mut indices, &mut nodes, 0);
        Self {
            nodes,
            root,
            generation: scene.generation,
        }
    }

    fn build_range(
        scene: &Scene,
        indices: &mut [usize],
        nodes: &mut Vec<Node>,
        axis: usize,
    ) -> usize {
        if indices.len() == 1 {
            let index = indices[0];
            let id = nodes.len();
            nodes.push(Node::Leaf {
                bounds: scene.leaves[index].bounds,
                index,
            });
            return id;
        }
        indices.sort_by(|a, b| {
            let center = |index: usize| {
                scene.leaves[index].bounds.map_or(0.0, |bounds| {
                    bounds.min[axis] / 2.0 + bounds.max[axis] / 2.0
                })
            };
            center(*a).total_cmp(&center(*b)).then_with(|| a.cmp(b))
        });
        let (left, right) = indices.split_at_mut(indices.len() / 2);
        let left = Self::build_range(scene, left, nodes, (axis + 1) % 3);
        let right = Self::build_range(scene, right, nodes, (axis + 1) % 3);
        let bounds = match (nodes[left].bounds(), nodes[right].bounds()) {
            (Some(a), Some(b)) => Some(a.union(b)),
            _ => None,
        };
        let id = nodes.len();
        nodes.push(Node::Branch {
            bounds,
            left,
            right,
        });
        id
    }

    fn query(&self, scene: &Scene, ray: Ray, budget: usize) -> Report {
        if self.generation != scene.generation {
            return flat(scene, ray, budget, true);
        }
        let mut stack = vec![self.root];
        let mut candidates = Vec::new();
        let mut node_tests = 0;
        let mut leaf_tests = 0;
        while let Some(index) = stack.pop() {
            if node_tests == budget {
                return indeterminate(Issue::WorkBudget, node_tests, leaf_tests, false);
            }
            node_tests += 1;
            let node = &self.nodes[index];
            match node.bounds().map(|bounds| bounds.ray(ray)) {
                Some(RegionResult::OutsideCertified) => continue,
                Some(RegionResult::Candidate) => {}
                _ => {
                    return indeterminate(
                        Issue::NumericOrUnsupported,
                        node_tests,
                        leaf_tests,
                        false,
                    );
                }
            }
            match node {
                Node::Leaf { index, .. } => {
                    leaf_tests += 1;
                    candidates.push(scene.leaves[*index].id);
                }
                Node::Branch { left, right, .. } => {
                    stack.push(*right);
                    stack.push(*left);
                }
            }
        }
        finish(candidates, node_tests, leaf_tests, false)
    }
}

fn graph_leaf(id: u32, graph: &Graph, translation: Vec3, scale: f64) -> Leaf {
    Leaf {
        id,
        bounds: Bounds::graph(graph, translation, scale),
    }
}

#[test]
fn directed_bounds_and_ray_enclosures_cover_independent_axis_hits() {
    let sphere = compile_json(SPHERE).unwrap();
    let cube = compile_json(BOX).unwrap();
    let cases = [
        (
            graph_leaf(11, &sphere, v(0.0, 0.0, 0.0), 1.0),
            v(-3.0, 0.0, 0.0),
            4.0,
        ),
        (
            graph_leaf(12, &cube, v(4.0, 0.0, 0.0), 1.0),
            v(0.0, 0.0, 0.0),
            6.0,
        ),
        (
            Leaf {
                id: 13,
                bounds: Bounds::capsule(v(8.0, 0.0, -1.0), v(8.0, 0.0, 1.0), 0.5),
            },
            v(6.0, 0.0, 0.0),
            4.0,
        ),
        (
            graph_leaf(14, &sphere, v(9000.0, 0.0, 0.0), 1.0),
            v(8997.0, 0.0, 0.0),
            4.0,
        ),
    ];
    for (leaf, origin, far) in cases {
        let id = leaf.id;
        let scene = Scene::new(vec![leaf]);
        let ray = Ray::new(origin, v(1.0, 0.0, 0.0), 0.0, far).unwrap();
        assert_eq!(
            Hierarchy::build(&scene).query(&scene, ray, 3).outcome,
            Outcome::Candidates(vec![id])
        );
    }
    for leaf in [
        graph_leaf(21, &sphere, Vec3::ZERO, 1.0),
        graph_leaf(22, &cube, Vec3::ZERO, 1.0),
    ] {
        let id = leaf.id;
        let scene = Scene::new(vec![leaf]);
        for ray in [
            Ray::new(Vec3::ZERO, v(1.0, 0.0, 0.0), 0.0, 2.0).unwrap(), // starts inside
            Ray::new(v(-3.0, 1.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap(), // tangent/boundary
        ] {
            assert_eq!(
                Hierarchy::build(&scene).query(&scene, ray, 3).outcome,
                Outcome::Candidates(vec![id])
            );
        }
    }
    let sphere_scene = Scene::new(vec![graph_leaf(31, &sphere, Vec3::ZERO, 1.0)]);
    let before_entry = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 1.9).unwrap();
    assert_eq!(
        Hierarchy::build(&sphere_scene)
            .query(&sphere_scene, before_entry, 3)
            .outcome,
        Outcome::OutsideCertified
    );
    let entry_endpoint = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 2.0, 3.0).unwrap();
    assert_eq!(
        Hierarchy::build(&sphere_scene)
            .query(&sphere_scene, entry_endpoint, 3)
            .outcome,
        Outcome::Candidates(vec![31])
    );
    let near_tangent = Ray::new(v(-3.0, 1.0 + 1e-8, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        Hierarchy::build(&sphere_scene)
            .query(&sphere_scene, near_tangent, 3)
            .outcome,
        Outcome::OutsideCertified
    );
    let close = Scene::new(vec![
        graph_leaf(41, &sphere, Vec3::ZERO, 1.0),
        graph_leaf(42, &sphere, v(1e-9, 0.0, 0.0), 1.0),
    ]);
    let close_ray = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        Hierarchy::build(&close).query(&close, close_ray, 3).outcome,
        Outcome::Candidates(vec![41, 42])
    );
    let nested = compile_json(NESTED).unwrap();
    // (sphere radius 1 × graph scale 2 + local offset 3) × instance scale 0.5
    // + instance translation 100 has center 101.5 and radius 1.
    let nested_scene = Scene::new(vec![graph_leaf(51, &nested, v(100.0, 0.0, 0.0), 0.5)]);
    let nested_index = Hierarchy::build(&nested_scene);
    let short = Ray::new(v(99.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 1.4).unwrap();
    let reaching = Ray::new(v(99.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 1.5).unwrap();
    assert_eq!(
        nested_index.query(&nested_scene, short, 3).outcome,
        Outcome::OutsideCertified
    );
    assert_eq!(
        nested_index.query(&nested_scene, reaching, 3).outcome,
        Outcome::Candidates(vec![51])
    );
}

#[test]
fn deterministic_axis_oracle_has_no_hidden_hits() {
    let sphere = compile_json(SPHERE).unwrap();
    let cube = compile_json(BOX).unwrap();
    let mut state = 0x5eed_2026_u64;
    for _ in 0..1024 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let x = ((state >> 33) % 18_000) as f64 * 0.5 - 4500.0;
        let y = (((state >> 17) % 1000) as f64 - 500.0) * 0.01;
        for leaf in [
            graph_leaf(1, &sphere, v(x, y, 0.0), 1.0),
            graph_leaf(2, &cube, v(x, y, 0.0), 1.0),
            Leaf {
                id: 3,
                bounds: Bounds::capsule(v(x, y, -1.0), v(x, y, 1.0), 1.0),
            },
        ] {
            let id = leaf.id;
            let scene = Scene::new(vec![leaf]);
            let index = Hierarchy::build(&scene);
            let hit = Ray::new(v(x - 3.0, y, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
            // Independent centerline oracle: all three solids begin at x-1.
            assert_eq!(
                index.query(&scene, hit, 3).outcome,
                Outcome::Candidates(vec![id])
            );
            let miss = Ray::new(v(x - 3.0, y + 2.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
            assert_eq!(
                index.query(&scene, miss, 3).outcome,
                Outcome::OutsideCertified
            );
        }
    }
}

#[test]
fn uncertainty_budget_and_mutation_never_become_miss() {
    let sphere = compile_json(SPHERE).unwrap();
    let large_sphere = compile_json(LARGE_SPHERE).unwrap();
    let organic = compile_json(include_bytes!(
        "../../../examples/field-graph/definitions/gr1-organic-loop.json"
    ))
    .unwrap();
    let ray = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    let mut scene = Scene::new(vec![graph_leaf(7, &sphere, Vec3::ZERO, 1.0)]);
    let index = Hierarchy::build(&scene);
    assert_eq!(
        index.query(&scene, ray, 0).outcome,
        Outcome::Indeterminate(Issue::WorkBudget)
    );
    scene.replace(0, graph_leaf(7, &sphere, v(20.0, 0.0, 0.0), 1.0));
    let stale = index.query(&scene, ray, 128);
    assert!(stale.fallback);
    assert_eq!(stale.outcome, Outcome::OutsideCertified);
    let moved_ray = Ray::new(v(17.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    let stale_at_new_location = index.query(&scene, moved_ray, 128);
    assert!(stale_at_new_location.fallback);
    assert_eq!(stale_at_new_location.outcome, Outcome::Candidates(vec![7]));
    assert_eq!(
        Hierarchy::build(&scene).query(&scene, ray, 128).outcome,
        Outcome::OutsideCertified
    );
    let revised = Scene::new(vec![graph_leaf(9, &sphere, Vec3::ZERO, 1.0)]);
    let previous = Hierarchy::build(&revised);
    let mut revised = revised;
    let enlarged_ray = Ray::new(v(-3.0, 1.5, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        previous.query(&revised, enlarged_ray, 128).outcome,
        Outcome::OutsideCertified
    );
    revised.replace(0, graph_leaf(9, &large_sphere, Vec3::ZERO, 1.0));
    let refreshed = previous.query(&revised, enlarged_ray, 128);
    assert!(refreshed.fallback);
    assert_eq!(refreshed.outcome, Outcome::Candidates(vec![9]));
    scene.replace(0, graph_leaf(7, &organic, Vec3::ZERO, 1.0));
    assert_eq!(
        index.query(&scene, ray, 128).outcome,
        Outcome::Indeterminate(Issue::NumericOrUnsupported)
    );
    assert!(Bounds::graph(&sphere, v(f64::MAX, 0.0, 0.0), 1.0).is_none());
    let overflow_ray = Ray::new(v(f64::MAX, 0.0, 0.0), v(f64::MAX, 0.0, 0.0), 0.0, 2.0).unwrap();
    let original = Scene::new(vec![graph_leaf(7, &sphere, Vec3::ZERO, 1.0)]);
    assert_eq!(
        Hierarchy::build(&original)
            .query(&original, overflow_ray, 3)
            .outcome,
        Outcome::Indeterminate(Issue::NumericOrUnsupported)
    );
    assert!(Ray::new(Vec3::ZERO, Vec3::ZERO, 0.0, 1.0).is_none());
    assert_eq!((1.0_f64 + 1e-8) as f32, 1.0_f32); // GPU f32 loses this QA-2 boundary distinction.
}

fn scene(graph: &Graph, count: usize, spacing: f64) -> Scene {
    Scene::new(
        (0..count)
            .map(|index| {
                let x = (index % 16) as f64 * spacing;
                let y = (index / 16) as f64 * spacing;
                graph_leaf(index as u32 + 1, graph, v(x, y, 0.0), 1.0)
            })
            .collect(),
    )
}

fn rays() -> Vec<Ray> {
    (0..32)
        .flat_map(|y| {
            (0..32).map(move |x| {
                Ray::new(
                    v(x as f64 * 1.2 - 2.0, y as f64 * 0.8 - 2.0, -2.0),
                    v(0.0, 0.0, 1.0),
                    0.0,
                    4.0,
                )
                .unwrap()
            })
        })
        .collect()
}

fn dense_rays() -> Vec<Ray> {
    (0..32)
        .flat_map(|y| {
            (0..32).map(move |x| {
                Ray::new(
                    v(
                        x as f64 * 0.6 / 31.0 - 0.3,
                        y as f64 * 0.6 / 31.0 - 0.3,
                        -2.0,
                    ),
                    v(0.0, 0.0, 1.0),
                    0.0,
                    4.0,
                )
                .unwrap()
            })
        })
        .collect()
}

#[test]
#[ignore = "GR-2 release-profile CPU broadphase benchmark"]
fn benchmark_bounded_ray_candidates() {
    let graph = Arc::new(compile_json(ARCH).unwrap());
    for (count, spacing, updated, dense) in ([
        (1, 3.0),
        (8, 3.0),
        (32, 3.0),
        (128, 3.0),
        (8, 1.0),
        (32, 1.0),
        (128, 1.0),
        (8, 0.0),
        (32, 0.0),
        (128, 0.0),
    ])
    .into_iter()
    .map(|(count, spacing)| (count, spacing, false, false))
    .chain([
        (32, 3.0, true, false),
        (8, 0.0, false, true),
        (32, 0.0, false, true),
        (128, 0.0, false, true),
    ]) {
        let queries = if dense { dense_rays() } else { rays() };
        let mut scene = scene(&graph, count, spacing);
        let mut builds = Vec::new();
        let mut index = Hierarchy::build(&scene);
        for _ in 0..7 {
            let started = Instant::now();
            index = Hierarchy::build(&scene);
            builds.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let mut update_ms = None;
        let mut stale_fallback = 0;
        if updated {
            let mut updates = Vec::new();
            for step in 0..7 {
                let started = Instant::now();
                let x = if step % 2 == 0 { 40.0 } else { 0.0 };
                scene.replace(0, graph_leaf(1, &graph, v(x, 0.0, 0.0), 1.0));
                let replace_ms = started.elapsed().as_secs_f64() * 1000.0;
                if step == 0 {
                    stale_fallback = usize::from(index.query(&scene, queries[0], 255).fallback);
                }
                let started = Instant::now();
                index = Hierarchy::build(&scene);
                updates.push(replace_ms + started.elapsed().as_secs_f64() * 1000.0);
            }
            updates.sort_by(f64::total_cmp);
            update_ms = Some(updates[3]);
        }
        let mut rows = Vec::new();
        for method in ["flat", "hierarchy"] {
            let mut times = Vec::new();
            let mut node_tests = 0;
            let mut leaf_tests = 0;
            let mut candidates = 0;
            let mut indeterminate = 0;
            let mut fallback = 0;
            let mut field_evaluations = 0;
            for repetition in 0..7 {
                let started = Instant::now();
                for &ray in &queries {
                    let result = if method == "flat" {
                        flat(&scene, ray, 255, false)
                    } else {
                        index.query(&scene, ray, 255)
                    };
                    if repetition == 0 {
                        node_tests += result.node_tests;
                        leaf_tests += result.leaf_tests;
                        field_evaluations += result.field_evaluations;
                        fallback += usize::from(result.fallback);
                        match &result.outcome {
                            Outcome::Candidates(ids) => candidates += ids.len(),
                            Outcome::Indeterminate(_) => indeterminate += 1,
                            Outcome::OutsideCertified => {}
                        }
                    }
                    black_box(result.outcome);
                }
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            rows.push(serde_json::json!({"method":method,"median_query_batch_ms":times[3],"node_tests":node_tests,"leaf_tests":leaf_tests,"candidates":candidates,"indeterminate":indeterminate,"fallback":fallback,"field_evaluations":field_evaluations}));
        }
        for &ray in &queries {
            assert_eq!(
                flat(&scene, ray, 255, false).outcome,
                index.query(&scene, ray, 255).outcome
            );
        }
        builds.sort_by(f64::total_cmp);
        println!(
            "GR2_BENCH {}",
            serde_json::json!({"instances":count,"spacing":spacing,"updated":updated,"dense":dense,"rays":queries.len(),"repetitions":7,"index_build_median_ms":builds[3],"update_rebuild_median_ms":update_ms,"stale_fallback_checks":stale_fallback,"node_count":index.nodes.len(),"node_bytes":index.nodes.len()*size_of::<Node>(),"leaf_record_bytes":scene.leaves.len()*size_of::<Leaf>(),"results":rows})
        );
    }
}
