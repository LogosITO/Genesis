//! GR-3 test-only interval narrowphase over validated hard Field Graphs.
//! All inputs denote exact real values of their stored binary64 numbers.

use super::*;
use std::collections::BTreeMap;

#[path = "gr4_world.rs"]
mod gr4_world;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Enclosure {
    lo: f64,
    hi: f64,
}

impl Enclosure {
    fn point(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self {
            lo: value,
            hi: value,
        })
    }

    fn outward(lo: f64, hi: f64) -> Option<Self> {
        let lo = lo.next_down();
        let hi = hi.next_up();
        (lo.is_finite() && hi.is_finite() && lo <= hi).then_some(Self { lo, hi })
    }

    fn add(self, rhs: Self) -> Option<Self> {
        Self::outward(self.lo + rhs.lo, self.hi + rhs.hi)
    }

    fn sub(self, rhs: Self) -> Option<Self> {
        Self::outward(self.lo - rhs.hi, self.hi - rhs.lo)
    }

    fn mul(self, rhs: Self) -> Option<Self> {
        let products = [
            self.lo * rhs.lo,
            self.lo * rhs.hi,
            self.hi * rhs.lo,
            self.hi * rhs.hi,
        ];
        if products.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Self::outward(
            products.iter().copied().fold(f64::INFINITY, f64::min),
            products.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    fn div(self, rhs: Self) -> Option<Self> {
        if rhs.lo <= 0.0 && rhs.hi >= 0.0 {
            return None;
        }
        let quotients = [
            self.lo / rhs.lo,
            self.lo / rhs.hi,
            self.hi / rhs.lo,
            self.hi / rhs.hi,
        ];
        if quotients.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Self::outward(
            quotients.iter().copied().fold(f64::INFINITY, f64::min),
            quotients.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    fn sqrt(self) -> Option<Self> {
        (self.lo >= 0.0).then(|| Self::outward(self.lo.sqrt(), self.hi.sqrt()))?
    }

    fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }

    fn max(self, rhs: Self) -> Self {
        Self {
            lo: self.lo.max(rhs.lo),
            hi: self.hi.max(rhs.hi),
        }
    }

    fn min(self, rhs: Self) -> Self {
        Self {
            lo: self.lo.min(rhs.lo),
            hi: self.hi.min(rhs.hi),
        }
    }

    fn contains(self, value: f64) -> bool {
        self.lo <= value && value <= self.hi
    }
}

fn point3(value: Vec3) -> Option<[Enclosure; 3]> {
    Some([
        Enclosure::point(value.x())?,
        Enclosure::point(value.y())?,
        Enclosure::point(value.z())?,
    ])
}

fn ray_point(ray: Ray, t: Enclosure) -> Option<[Enclosure; 3]> {
    let origin = point3(ray.origin)?;
    let direction = point3(ray.direction)?;
    Some([
        origin[0].add(direction[0].mul(t)?)?,
        origin[1].add(direction[1].mul(t)?)?,
        origin[2].add(direction[2].mul(t)?)?,
    ])
}

#[derive(Clone, Copy)]
enum Shape {
    Sphere {
        center: [Enclosure; 3],
        radius: Enclosure,
    },
    Box {
        center: [Enclosure; 3],
        extent: [Enclosure; 3],
    },
}

#[derive(Clone, Copy)]
struct Term {
    ordinal: u32,
    shape: Shape,
}

fn flatten(graph: &Graph, translation: Vec3, scale: f64) -> Option<Vec<Term>> {
    let scale = Enclosure::point(scale)?;
    if scale.lo <= 0.0 {
        return None;
    }
    let mut terms = Vec::new();
    fn visit(
        graph: &Graph,
        index: u32,
        center: [Enclosure; 3],
        scale: Enclosure,
        terms: &mut Vec<Term>,
    ) -> Option<()> {
        if terms.len() >= 64 {
            return None;
        }
        match graph.nodes().get(index as usize)? {
            GraphNode::Sphere(sphere) => {
                let radius = scale.mul(Enclosure::point(sphere.radius())?)?;
                if radius.lo <= 0.0 {
                    return None;
                }
                terms.push(Term {
                    ordinal: terms.len() as u32,
                    shape: Shape::Sphere { center, radius },
                });
            }
            GraphNode::Box(bounds) => {
                let extent = point3(bounds.half_extent())?;
                let extent = [
                    scale.mul(extent[0])?,
                    scale.mul(extent[1])?,
                    scale.mul(extent[2])?,
                ];
                if extent.iter().any(|axis| axis.lo <= 0.0) {
                    return None;
                }
                terms.push(Term {
                    ordinal: terms.len() as u32,
                    shape: Shape::Box { center, extent },
                });
            }
            GraphNode::Translate { child, offset } => {
                let offset = point3(*offset)?;
                let center = [
                    center[0].add(scale.mul(offset[0])?)?,
                    center[1].add(scale.mul(offset[1])?)?,
                    center[2].add(scale.mul(offset[2])?)?,
                ];
                visit(graph, *child, center, scale, terms)?;
            }
            GraphNode::Scale { child, factor } => {
                let scale = scale.mul(Enclosure::point(*factor)?)?;
                if scale.lo <= 0.0 {
                    return None;
                }
                visit(graph, *child, center, scale, terms)?;
            }
            GraphNode::Union { left, right } => {
                visit(graph, *left, center, scale, terms)?;
                visit(graph, *right, center, scale, terms)?;
            }
            GraphNode::SmoothUnion { .. } => return None,
        }
        Some(())
    }
    visit(graph, graph.root(), point3(translation)?, scale, &mut terms)?;
    Some(terms)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NarrowIssue {
    Broadphase(Issue),
    UnsupportedOrOverflow,
    StartsInside,
    BoundaryOrTangent,
    WorkBudget,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct QualifiedHit {
    instance_id: u32,
    ordinal: u32,
    parameter: Enclosure,
    position: [Enclosure; 3],
    normal: Option<[Enclosure; 3]>,
}

struct PrimitiveHit {
    parameter: Enclosure,
    position: [Enclosure; 3],
    normal: Option<[Enclosure; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NarrowOutcome {
    Miss,
    Hit(QualifiedHit),
    HitAmbiguous {
        parameter: Enclosure,
        position: [Enclosure; 3],
    },
    Indeterminate(NarrowIssue),
}

fn dot(a: [Enclosure; 3], b: [Enclosure; 3]) -> Option<Enclosure> {
    Enclosure::point(0.0)?
        .add(a[0].mul(b[0])?)?
        .add(a[1].mul(b[1])?)?
        .add(a[2].mul(b[2])?)
}

fn sphere_hit(
    ray: Ray,
    center: [Enclosure; 3],
    radius: Enclosure,
) -> Result<Option<PrimitiveHit>, NarrowIssue> {
    let numeric = NarrowIssue::UnsupportedOrOverflow;
    let start = ray_point(ray, Enclosure::point(ray.near).ok_or(numeric)?).ok_or(numeric)?;
    let delta = [
        start[0].sub(center[0]).ok_or(numeric)?,
        start[1].sub(center[1]).ok_or(numeric)?,
        start[2].sub(center[2]).ok_or(numeric)?,
    ];
    let r2 = radius.mul(radius).ok_or(numeric)?;
    let start_sign = dot(delta, delta).ok_or(numeric)?.sub(r2).ok_or(numeric)?;
    if start_sign.hi < 0.0 {
        return Err(NarrowIssue::StartsInside);
    }
    if start_sign.lo <= 0.0 {
        return Err(NarrowIssue::BoundaryOrTangent);
    }

    let origin = point3(ray.origin).ok_or(numeric)?;
    let direction = point3(ray.direction).ok_or(numeric)?;
    let delta = [
        origin[0].sub(center[0]).ok_or(numeric)?,
        origin[1].sub(center[1]).ok_or(numeric)?,
        origin[2].sub(center[2]).ok_or(numeric)?,
    ];
    let two = Enclosure::point(2.0).ok_or(numeric)?;
    let four = Enclosure::point(4.0).ok_or(numeric)?;
    let a = dot(direction, direction).ok_or(numeric)?;
    if a.lo <= 0.0 {
        return Err(numeric);
    }
    let b = two
        .mul(dot(delta, direction).ok_or(numeric)?)
        .ok_or(numeric)?;
    let c = dot(delta, delta).ok_or(numeric)?.sub(r2).ok_or(numeric)?;
    let discriminant = b
        .mul(b)
        .ok_or(numeric)?
        .sub(four.mul(a).ok_or(numeric)?.mul(c).ok_or(numeric)?)
        .ok_or(numeric)?;
    if discriminant.hi < 0.0 {
        return Ok(None);
    }
    if discriminant.lo <= 0.0 {
        return Err(NarrowIssue::BoundaryOrTangent);
    }
    let root = discriminant.sqrt().ok_or(numeric)?;
    let denominator = two.mul(a).ok_or(numeric)?;
    let entry = b
        .neg()
        .sub(root)
        .ok_or(numeric)?
        .div(denominator)
        .ok_or(numeric)?;
    let exit = b
        .neg()
        .add(root)
        .ok_or(numeric)?
        .div(denominator)
        .ok_or(numeric)?;
    if entry.lo > ray.far || exit.hi < ray.near {
        return Ok(None);
    }
    if entry.lo <= ray.near || entry.hi >= ray.far || entry.hi >= exit.lo {
        return Err(NarrowIssue::BoundaryOrTangent);
    }
    let position = ray_point(ray, entry).ok_or(numeric)?;
    let normal = [
        position[0]
            .sub(center[0])
            .ok_or(numeric)?
            .div(radius)
            .ok_or(numeric)?,
        position[1]
            .sub(center[1])
            .ok_or(numeric)?
            .div(radius)
            .ok_or(numeric)?,
        position[2]
            .sub(center[2])
            .ok_or(numeric)?
            .div(radius)
            .ok_or(numeric)?,
    ];
    Ok(Some(PrimitiveHit {
        parameter: entry,
        position,
        normal: Some(normal),
    }))
}

fn box_hit(
    ray: Ray,
    center: [Enclosure; 3],
    extent: [Enclosure; 3],
) -> Result<Option<PrimitiveHit>, NarrowIssue> {
    let numeric = NarrowIssue::UnsupportedOrOverflow;
    let lower = [
        center[0].sub(extent[0]).ok_or(numeric)?,
        center[1].sub(extent[1]).ok_or(numeric)?,
        center[2].sub(extent[2]).ok_or(numeric)?,
    ];
    let upper = [
        center[0].add(extent[0]).ok_or(numeric)?,
        center[1].add(extent[1]).ok_or(numeric)?,
        center[2].add(extent[2]).ok_or(numeric)?,
    ];
    let start = ray_point(ray, Enclosure::point(ray.near).ok_or(numeric)?).ok_or(numeric)?;
    let outside =
        (0..3).any(|axis| start[axis].hi < lower[axis].lo || start[axis].lo > upper[axis].hi);
    let inside =
        (0..3).all(|axis| start[axis].lo > lower[axis].hi && start[axis].hi < upper[axis].lo);
    if inside {
        return Err(NarrowIssue::StartsInside);
    }
    if !outside {
        return Err(NarrowIssue::BoundaryOrTangent);
    }

    let origin = point3(ray.origin).ok_or(numeric)?;
    let direction = [ray.direction.x(), ray.direction.y(), ray.direction.z()];
    let mut entry_axes = [None; 3];
    let mut entry = None;
    let mut exit = None;
    for axis in 0..3 {
        let d = direction[axis];
        if d == 0.0 {
            if origin[axis].hi < lower[axis].lo || origin[axis].lo > upper[axis].hi {
                return Ok(None);
            }
            if origin[axis].lo <= lower[axis].hi || origin[axis].hi >= upper[axis].lo {
                return Err(NarrowIssue::BoundaryOrTangent);
            }
            continue;
        }
        let d = Enclosure::point(d).ok_or(numeric)?;
        let (near_plane, far_plane) = if direction[axis] > 0.0 {
            (lower[axis], upper[axis])
        } else {
            (upper[axis], lower[axis])
        };
        let axis_entry = near_plane
            .sub(origin[axis])
            .ok_or(numeric)?
            .div(d)
            .ok_or(numeric)?;
        let axis_exit = far_plane
            .sub(origin[axis])
            .ok_or(numeric)?
            .div(d)
            .ok_or(numeric)?;
        entry_axes[axis] = Some(axis_entry);
        entry = Some(entry.map_or(axis_entry, |previous: Enclosure| previous.max(axis_entry)));
        exit = Some(exit.map_or(axis_exit, |previous: Enclosure| previous.min(axis_exit)));
    }
    let entry = entry.ok_or(numeric)?;
    let exit = exit.ok_or(numeric)?;
    if entry.lo > exit.hi || exit.hi < ray.near || entry.lo > ray.far {
        return Ok(None);
    }
    if entry.lo <= ray.near || entry.hi >= ray.far || entry.hi >= exit.lo {
        return Err(NarrowIssue::BoundaryOrTangent);
    }
    let position = ray_point(ray, entry).ok_or(numeric)?;
    let unique_axis = (0..3).find(|&axis| {
        entry_axes[axis].is_some_and(|value| {
            (0..3).all(|other| {
                other == axis
                    || entry_axes[other].is_none_or(|other_value| value.lo > other_value.hi)
            })
        })
    });
    let normal = unique_axis.map(|axis| {
        let mut normal = [Enclosure::point(0.0).unwrap(); 3];
        normal[axis] = Enclosure::point(if direction[axis] > 0.0 { -1.0 } else { 1.0 }).unwrap();
        normal
    });
    Ok(Some(PrimitiveHit {
        parameter: entry,
        position,
        normal,
    }))
}

#[derive(Clone)]
struct ResearchInstance {
    id: u32,
    graph: Arc<Graph>,
    translation: Vec3,
    scale: f64,
}

impl ResearchInstance {
    fn leaf(&self) -> Leaf {
        graph_leaf(self.id, &self.graph, self.translation, self.scale)
    }

    fn shapes(&self) -> Option<Vec<Term>> {
        flatten(&self.graph, self.translation, self.scale)
    }
}

#[derive(Clone, Copy)]
enum Path {
    Flat,
    Hierarchy,
}

struct ResearchScene {
    instances: Vec<ResearchInstance>,
    shapes: Vec<Option<Vec<Term>>>,
    lookup: BTreeMap<u32, usize>,
    broad: Scene,
    index: Hierarchy,
}

#[derive(Debug)]
struct QueryRecord {
    outcome: NarrowOutcome,
    candidate_count: usize,
    node_tests: usize,
    leaf_tests: usize,
    shape_tests: usize,
    fallback: bool,
    broad_ns: u128,
    narrow_ns: u128,
}

impl ResearchScene {
    fn new(instances: Vec<ResearchInstance>) -> Self {
        let broad = Scene::new(instances.iter().map(ResearchInstance::leaf).collect());
        let index = Hierarchy::build(&broad);
        let shapes = instances.iter().map(ResearchInstance::shapes).collect();
        let lookup = instances
            .iter()
            .enumerate()
            .map(|(index, instance)| (instance.id, index))
            .collect();
        Self {
            instances,
            shapes,
            lookup,
            broad,
            index,
        }
    }

    fn replace(&mut self, index: usize, instance: ResearchInstance) {
        assert_eq!(self.instances[index].id, instance.id);
        self.broad.replace(index, instance.leaf());
        self.shapes[index] = instance.shapes();
        self.instances[index] = instance;
    }

    fn rebuild(&mut self) {
        self.index = Hierarchy::build(&self.broad);
    }

    fn query(&self, ray: Ray, path: Path, budget: usize) -> QueryRecord {
        let started = Instant::now();
        let broad = match path {
            Path::Flat => flat(&self.broad, ray, budget, false),
            Path::Hierarchy => self.index.query(&self.broad, ray, budget),
        };
        let broad_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let (outcome, candidate_count, shape_tests) = match broad.outcome {
            Outcome::OutsideCertified => (NarrowOutcome::Miss, 0, 0),
            Outcome::Indeterminate(issue) => (
                NarrowOutcome::Indeterminate(NarrowIssue::Broadphase(issue)),
                0,
                0,
            ),
            Outcome::Candidates(ids) => {
                let count = ids.len();
                let (outcome, work) = self.narrow(&ids, ray, budget);
                (outcome, count, work)
            }
        };
        QueryRecord {
            outcome,
            candidate_count,
            node_tests: broad.node_tests,
            leaf_tests: broad.leaf_tests,
            shape_tests,
            fallback: broad.fallback,
            broad_ns,
            narrow_ns: started.elapsed().as_nanos(),
        }
    }

    fn narrow(&self, ids: &[u32], ray: Ray, budget: usize) -> (NarrowOutcome, usize) {
        let mut hits = Vec::new();
        let mut work = 0;
        for id in ids {
            let index = self.lookup[id];
            let Some(terms) = self.shapes[index].as_ref() else {
                return (
                    NarrowOutcome::Indeterminate(NarrowIssue::UnsupportedOrOverflow),
                    work,
                );
            };
            for term in terms {
                if work == budget {
                    return (NarrowOutcome::Indeterminate(NarrowIssue::WorkBudget), work);
                }
                work += 1;
                let result = match term.shape {
                    Shape::Sphere { center, radius } => sphere_hit(ray, center, radius),
                    Shape::Box { center, extent } => box_hit(ray, center, extent),
                };
                match result {
                    Ok(None) => {}
                    Ok(Some(hit)) => hits.push(QualifiedHit {
                        instance_id: *id,
                        ordinal: term.ordinal,
                        parameter: hit.parameter,
                        position: hit.position,
                        normal: hit.normal,
                    }),
                    Err(issue) => return (NarrowOutcome::Indeterminate(issue), work),
                }
            }
        }
        if hits.is_empty() {
            return (NarrowOutcome::Miss, work);
        }
        hits.sort_by(|a, b| {
            a.parameter
                .lo
                .total_cmp(&b.parameter.lo)
                .then_with(|| a.instance_id.cmp(&b.instance_id))
                .then_with(|| a.ordinal.cmp(&b.ordinal))
        });
        let first = hits[0];
        if hits[1..]
            .iter()
            .any(|other| first.parameter.hi >= other.parameter.lo)
        {
            // Every candidate proved an exterior entry; the earliest union entry exists
            // even when interval overlap prevents identifying its owner.
            let parameter = hits
                .iter()
                .fold(first.parameter, |acc, hit| acc.min(hit.parameter));
            return match ray_point(ray, parameter) {
                Some(position) => (
                    NarrowOutcome::HitAmbiguous {
                        parameter,
                        position,
                    },
                    work,
                ),
                None => (
                    NarrowOutcome::Indeterminate(NarrowIssue::UnsupportedOrOverflow),
                    work,
                ),
            };
        }
        (NarrowOutcome::Hit(first), work)
    }
}

fn instance(id: u32, graph: &Arc<Graph>, translation: Vec3, scale: f64) -> ResearchInstance {
    ResearchInstance {
        id,
        graph: Arc::clone(graph),
        translation,
        scale,
    }
}

fn expect_hit(outcome: NarrowOutcome, id: u32, parameter: f64, normal: Option<[f64; 3]>) {
    let NarrowOutcome::Hit(hit) = outcome else {
        panic!("expected qualified hit, got {outcome:?}");
    };
    assert_eq!(hit.instance_id, id);
    assert!(hit.parameter.contains(parameter), "{hit:?}");
    assert!(hit.parameter.hi - hit.parameter.lo < 1e-8, "{hit:?}");
    for component in hit.position {
        assert!(component.hi - component.lo < 1e-7, "{hit:?}");
    }
    match (hit.normal, normal) {
        (Some(found), Some(expected)) => {
            for axis in 0..3 {
                assert!(found[axis].contains(expected[axis]), "{hit:?}");
                assert!(found[axis].hi - found[axis].lo < 1e-7, "{hit:?}");
            }
        }
        (None, None) => {}
        _ => panic!("unexpected normal qualification: {hit:?}"),
    }
}

const DISJOINT_UNION: &[u8] = br#"{"format_version":1,"id":"gr3-disjoint","root":"u","nodes":[{"op":"sphere","id":"a","radius":1.0},{"op":"translate","id":"left","child":"a","offset":[-2.0,0.0,0.0]},{"op":"sphere","id":"b","radius":1.0},{"op":"translate","id":"right","child":"b","offset":[2.0,0.0,0.0]},{"op":"union","id":"u","left":"left","right":"right"}]}"#;
const OVERLAP_UNION: &[u8] = br#"{"format_version":1,"id":"gr3-overlap","root":"u","nodes":[{"op":"sphere","id":"a","radius":1.0},{"op":"sphere","id":"b","radius":1.0},{"op":"translate","id":"right","child":"b","offset":[0.5,0.0,0.0]},{"op":"union","id":"u","left":"a","right":"right"}]}"#;
const COINCIDENT_UNION: &[u8] = br#"{"format_version":1,"id":"gr3-coincident","root":"u","nodes":[{"op":"sphere","id":"a","radius":1.0},{"op":"sphere","id":"b","radius":1.0},{"op":"union","id":"u","left":"a","right":"b"}]}"#;

#[test]
fn qualified_primitive_axis_oracles() {
    let sphere = Arc::new(compile_json(SPHERE).unwrap());
    let box_graph = Arc::new(compile_json(BOX).unwrap());
    let sphere_scene = ResearchScene::new(vec![instance(11, &sphere, Vec3::ZERO, 1.0)]);
    let box_scene = ResearchScene::new(vec![instance(22, &box_graph, v(4.0, 0.0, 0.0), 1.0)]);
    for path in [Path::Flat, Path::Hierarchy] {
        let ray = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
        let NarrowOutcome::Hit(positioned) = sphere_scene.query(ray, path, 256).outcome else {
            panic!("sphere axis hit lost");
        };
        assert!(positioned.position[0].contains(-1.0));
        assert!(positioned.position[1].contains(0.0));
        expect_hit(
            sphere_scene.query(ray, path, 256).outcome,
            11,
            2.0,
            Some([-1.0, 0.0, 0.0]),
        );
        let fast = Ray::new(v(-3.0, 0.0, 0.0), v(2.0, 0.0, 0.0), 0.0, 3.0).unwrap();
        expect_hit(
            sphere_scene.query(fast, path, 256).outcome,
            11,
            1.0,
            Some([-1.0, 0.0, 0.0]),
        );
        let face = Ray::new(Vec3::ZERO, v(1.0, 0.0, 0.0), 0.0, 8.0).unwrap();
        expect_hit(
            box_scene.query(face, path, 256).outcome,
            22,
            3.0,
            Some([-1.0, 0.0, 0.0]),
        );
    }
}

#[test]
fn boundary_inside_and_feature_normals_are_explicit() {
    let sphere = Arc::new(compile_json(SPHERE).unwrap());
    let box_graph = Arc::new(compile_json(BOX).unwrap());
    let sphere_scene = ResearchScene::new(vec![instance(1, &sphere, Vec3::ZERO, 1.0)]);
    let box_scene = ResearchScene::new(vec![instance(2, &box_graph, Vec3::ZERO, 1.0)]);
    let miss = Ray::new(v(-3.0, 2.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        sphere_scene.query(miss, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Miss
    );
    let aabb_overlap = Ray::new(v(-3.0, 0.9, 0.9), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        sphere_scene
            .query(aabb_overlap, Path::Hierarchy, 256)
            .outcome,
        NarrowOutcome::Miss
    );
    let tangent = Ray::new(v(-3.0, 1.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert!(matches!(
        sphere_scene.query(tangent, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Indeterminate(_)
    ));
    let inside = Ray::new(Vec3::ZERO, v(1.0, 0.0, 0.0), 0.0, 3.0).unwrap();
    assert_eq!(
        sphere_scene.query(inside, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Indeterminate(NarrowIssue::StartsInside)
    );
    assert_eq!(
        box_scene.query(inside, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Indeterminate(NarrowIssue::StartsInside)
    );
    let edge = Ray::new(v(-3.0, -3.0, 0.0), v(1.0, 1.0, 0.0), 0.0, 6.0).unwrap();
    expect_hit(
        box_scene.query(edge, Path::Hierarchy, 256).outcome,
        2,
        2.0,
        None,
    );
    let vertex = Ray::new(v(-3.0, -3.0, -3.0), v(1.0, 1.0, 1.0), 0.0, 6.0).unwrap();
    expect_hit(
        box_scene.query(vertex, Path::Hierarchy, 256).outcome,
        2,
        2.0,
        None,
    );
    let tiny = Ray::new(v(-3.0, 0.0, 0.0), v(1e-12, 0.0, 0.0), 0.0, 1.0).unwrap();
    assert_eq!(
        sphere_scene.query(tiny, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Miss
    );
    let boundary = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 2.0, 6.0).unwrap();
    assert!(matches!(
        sphere_scene.query(boundary, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Indeterminate(_)
    ));
    assert_eq!((1.0_f64 + 1e-8) as f32, 1.0_f32);
}

#[test]
fn unions_transforms_identity_and_mutation() {
    let disjoint = Arc::new(compile_json(DISJOINT_UNION).unwrap());
    let overlap = Arc::new(compile_json(OVERLAP_UNION).unwrap());
    let coincident = Arc::new(compile_json(COINCIDENT_UNION).unwrap());
    let ray = Ray::new(v(-5.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 10.0).unwrap();
    let scene = ResearchScene::new(vec![instance(17, &disjoint, Vec3::ZERO, 1.0)]);
    expect_hit(
        scene.query(ray, Path::Hierarchy, 256).outcome,
        17,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    let reverse = Ray::new(v(5.0, 0.0, 0.0), v(-1.0, 0.0, 0.0), 0.0, 10.0).unwrap();
    let reverse_hit = scene.query(reverse, Path::Hierarchy, 256).outcome;
    expect_hit(reverse_hit, 17, 2.0, Some([1.0, 0.0, 0.0]));
    let NarrowOutcome::Hit(reverse_hit) = reverse_hit else {
        unreachable!("qualified by expect_hit")
    };
    assert_eq!(reverse_hit.ordinal, 1);
    let scene = ResearchScene::new(vec![instance(18, &overlap, Vec3::ZERO, 1.0)]);
    let front = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    expect_hit(
        scene.query(front, Path::Hierarchy, 256).outcome,
        18,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    let scene = ResearchScene::new(vec![instance(19, &coincident, Vec3::ZERO, 1.0)]);
    let NarrowOutcome::HitAmbiguous {
        parameter,
        position,
    } = scene.query(front, Path::Hierarchy, 256).outcome
    else {
        panic!("coincident surfaces must retain a qualified geometric hit");
    };
    assert!(parameter.contains(2.0));
    assert!(position[0].contains(-1.0));
    let single = Arc::new(compile_json(SPHERE).unwrap());
    let separate_instances = ResearchScene::new(vec![
        instance(91, &single, Vec3::ZERO, 1.0),
        instance(92, &single, v(4.0, 0.0, 0.0), 1.0),
    ]);
    let front_long = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 8.0).unwrap();
    expect_hit(
        separate_instances
            .query(front_long, Path::Hierarchy, 256)
            .outcome,
        91,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );

    let nested = Arc::new(compile_json(NESTED).unwrap());
    let scene = ResearchScene::new(vec![instance(23, &nested, v(100.0, 0.0, 0.0), 0.5)]);
    let nested_ray = Ray::new(v(99.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    expect_hit(
        scene.query(nested_ray, Path::Hierarchy, 256).outcome,
        23,
        1.5,
        Some([-1.0, 0.0, 0.0]),
    );

    let small = Arc::new(compile_json(SPHERE).unwrap());
    let large = Arc::new(compile_json(LARGE_SPHERE).unwrap());
    let mut scene = ResearchScene::new(vec![instance(31, &small, Vec3::ZERO, 1.0)]);
    let raised_ray = Ray::new(v(-3.0, 1.5, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        scene.query(raised_ray, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Miss
    );
    scene.replace(0, instance(31, &large, Vec3::ZERO, 1.0));
    let stale = scene.query(raised_ray, Path::Hierarchy, 256);
    assert!(stale.fallback);
    assert!(matches!(stale.outcome, NarrowOutcome::Hit(_)));
    scene.rebuild();
    assert!(!scene.query(raised_ray, Path::Hierarchy, 256).fallback);
    scene.replace(0, instance(31, &small, v(20.0, 0.0, 0.0), 1.0));
    let moved = Ray::new(v(17.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    expect_hit(
        scene.query(moved, Path::Hierarchy, 256).outcome,
        31,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
}

#[test]
fn near_ties_overflow_and_budget_never_become_miss() {
    let sphere = Arc::new(compile_json(SPHERE).unwrap());
    let scene = ResearchScene::new(vec![
        instance(1, &sphere, Vec3::ZERO, 1.0),
        instance(2, &sphere, v(1e-15, 0.0, 0.0), 1.0),
    ]);
    let ray = Ray::new(v(-3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert!(matches!(
        scene.query(ray, Path::Hierarchy, 256).outcome,
        NarrowOutcome::HitAmbiguous { parameter, .. } if parameter.contains(2.0)
    ));
    assert_eq!(
        scene.query(ray, Path::Hierarchy, 0).outcome,
        NarrowOutcome::Indeterminate(NarrowIssue::Broadphase(Issue::WorkBudget))
    );
    assert_eq!(
        scene.query(ray, Path::Flat, 1).outcome,
        NarrowOutcome::Indeterminate(NarrowIssue::Broadphase(Issue::WorkBudget))
    );
    let one = ResearchScene::new(vec![instance(1, &sphere, Vec3::ZERO, 1.0)]);
    let union = Arc::new(compile_json(DISJOINT_UNION).unwrap());
    let two_terms = ResearchScene::new(vec![instance(1, &union, Vec3::ZERO, 1.0)]);
    let both = Ray::new(v(-5.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 10.0).unwrap();
    assert_eq!(
        two_terms.query(both, Path::Hierarchy, 1).outcome,
        NarrowOutcome::Indeterminate(NarrowIssue::WorkBudget)
    );
    let huge = Ray::new(v(f64::MAX, 0.0, 0.0), v(f64::MAX, 0.0, 0.0), 0.0, 2.0).unwrap();
    assert!(matches!(
        one.query(huge, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Indeterminate(_)
    ));
    let smooth = Arc::new(
        compile_json(include_bytes!(
            "../../../../examples/field-graph/definitions/gr1-organic-loop.json"
        ))
        .unwrap(),
    );
    let unsupported = ResearchScene::new(vec![instance(3, &smooth, Vec3::ZERO, 1.0)]);
    assert_eq!(
        unsupported.query(ray, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Indeterminate(NarrowIssue::Broadphase(Issue::NumericOrUnsupported))
    );
    let distant = ResearchScene::new(vec![instance(4, &sphere, v(9000.0, 0.0, 0.0), 1.0)]);
    let near = Ray::new(v(8997.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    expect_hit(
        distant.query(near, Path::Hierarchy, 256).outcome,
        4,
        2.0,
        Some([-1.0, 0.0, 0.0]),
    );
    let small = ResearchScene::new(vec![instance(5, &sphere, Vec3::ZERO, 0.01)]);
    let close = Ray::new(v(-0.03, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 0.1).unwrap();
    expect_hit(
        small.query(close, Path::Hierarchy, 256).outcome,
        5,
        0.02,
        Some([-1.0, 0.0, 0.0]),
    );
}

#[test]
fn deterministic_axis_oracle_and_near_tangent_counterexample() {
    let sphere = Arc::new(compile_json(SPHERE).unwrap());
    let mut state = 0x5eed_2027_u64;
    for _ in 0..1024 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let center = ((state >> 33) % 2000) as f64 - 1000.0;
        let scene = ResearchScene::new(vec![instance(77, &sphere, v(center, 0.0, 0.0), 1.0)]);
        let hit = Ray::new(v(center - 3.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
        expect_hit(
            scene.query(hit, Path::Hierarchy, 256).outcome,
            77,
            2.0,
            Some([-1.0, 0.0, 0.0]),
        );
        let miss = Ray::new(v(center - 3.0, 2.0, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
        assert_eq!(
            scene.query(miss, Path::Hierarchy, 256).outcome,
            NarrowOutcome::Miss
        );
    }
    let scene = ResearchScene::new(vec![instance(1, &sphere, Vec3::ZERO, 1.0)]);
    let near_tangent = Ray::new(v(-3.0, 1.0 + 1e-8, 0.0), v(1.0, 0.0, 0.0), 0.0, 6.0).unwrap();
    assert_eq!(
        scene.query(near_tangent, Path::Hierarchy, 256).outcome,
        NarrowOutcome::Miss
    );
    assert_eq!((1.0_f64 + 1e-8) as f32, 1.0_f32);
}

fn benchmark_scene(graph: &Arc<Graph>, count: usize, spacing: f64) -> ResearchScene {
    ResearchScene::new(
        (0..count)
            .map(|index| {
                instance(
                    index as u32 + 1,
                    graph,
                    v(
                        (index % 16) as f64 * spacing,
                        (index / 16) as f64 * spacing,
                        0.0,
                    ),
                    1.0,
                )
            })
            .collect(),
    )
}

fn benchmark_method(
    scene: &ResearchScene,
    queries: &[Ray],
    path: Path,
    label: &str,
) -> serde_json::Value {
    let mut full = Vec::new();
    let mut broad = Vec::new();
    let mut narrow = Vec::new();
    let mut candidates = 0;
    let mut node_tests = 0;
    let mut leaf_tests = 0;
    let mut shape_tests = 0;
    let mut fallback = 0;
    let mut indeterminate = 0;
    let mut hits = 0;
    let mut ambiguous_hits = 0;
    for repetition in 0..7 {
        let started = Instant::now();
        let mut broad_ns = 0;
        let mut narrow_ns = 0;
        for &ray in queries {
            let record = scene.query(ray, path, 512);
            broad_ns += record.broad_ns;
            narrow_ns += record.narrow_ns;
            if repetition == 0 {
                candidates += record.candidate_count;
                node_tests += record.node_tests;
                leaf_tests += record.leaf_tests;
                shape_tests += record.shape_tests;
                fallback += usize::from(record.fallback);
                indeterminate +=
                    usize::from(matches!(record.outcome, NarrowOutcome::Indeterminate(_)));
                hits += usize::from(matches!(record.outcome, NarrowOutcome::Hit(_)));
                ambiguous_hits +=
                    usize::from(matches!(record.outcome, NarrowOutcome::HitAmbiguous { .. }));
            }
            black_box(record.outcome);
        }
        full.push(started.elapsed().as_secs_f64() * 1000.0);
        broad.push(broad_ns as f64 / 1_000_000.0);
        narrow.push(narrow_ns as f64 / 1_000_000.0);
    }
    full.sort_by(f64::total_cmp);
    broad.sort_by(f64::total_cmp);
    narrow.sort_by(f64::total_cmp);
    serde_json::json!({
        "method":label,"full_median_ms":full[3],"broad_median_ms":broad[3],"narrow_median_ms":narrow[3],
        "candidates":candidates,"node_tests":node_tests,"leaf_tests":leaf_tests,"shape_tests":shape_tests,
        "field_evaluations":0,"fallback":fallback,"indeterminate":indeterminate,"hits":hits,
        "ambiguous_hits":ambiguous_hits
    })
}

#[test]
#[ignore = "GR-3 release-profile complete CPU query benchmark"]
fn benchmark_full_queries() {
    let graph = Arc::new(compile_json(SPHERE).unwrap());
    for (count, spacing, dense, updated) in [
        (1, 3.0, false, false),
        (8, 3.0, false, false),
        (32, 3.0, false, false),
        (128, 3.0, false, false),
        (32, 1.0, false, false),
        (128, 1.0, false, false),
        (32, 0.0, false, false),
        (128, 0.0, false, false),
        (32, 0.0, true, false),
        (128, 0.0, true, false),
        (32, 3.0, false, true),
    ] {
        let queries = if dense { dense_rays() } else { rays() };
        let mut scene = benchmark_scene(&graph, count, spacing);
        let mut build = Vec::new();
        for _ in 0..7 {
            let started = Instant::now();
            scene.rebuild();
            build.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let mut update_ms = None;
        let mut stale = None;
        if updated {
            let started = Instant::now();
            scene.replace(0, instance(1, &graph, v(40.0, 0.0, 0.0), 1.0));
            let replace_ms = started.elapsed().as_secs_f64() * 1000.0;
            stale = Some(benchmark_method(
                &scene,
                &queries,
                Path::Hierarchy,
                "stale_fallback",
            ));
            let started = Instant::now();
            scene.rebuild();
            update_ms = Some(replace_ms + started.elapsed().as_secs_f64() * 1000.0);
        }
        for &ray in &queries {
            assert_eq!(
                scene.query(ray, Path::Flat, 512).outcome,
                scene.query(ray, Path::Hierarchy, 512).outcome
            );
        }
        let flat = benchmark_method(&scene, &queries, Path::Flat, "flat");
        let hierarchy = benchmark_method(&scene, &queries, Path::Hierarchy, "hierarchy");
        build.sort_by(f64::total_cmp);
        let term_count: usize = scene
            .shapes
            .iter()
            .filter_map(Option::as_ref)
            .map(Vec::len)
            .sum();
        println!(
            "GR3_BENCH {}",
            serde_json::json!({
                "instances":count,"spacing":spacing,"dense":dense,"updated":updated,"rays":queries.len(),
                "repetitions":7,"index_build_median_ms":build[3],"update_and_rebuild_ms":update_ms,
                "node_count":scene.index.nodes.len(),"node_bytes":scene.index.nodes.len()*size_of::<Node>(),
                "leaf_bytes":scene.broad.leaves.len()*size_of::<Leaf>(),"term_bytes":term_count*size_of::<Term>(),
                "results":[flat,hierarchy],"stale_fallback":stale
            })
        );
    }
}
