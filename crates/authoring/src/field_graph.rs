//! Bounded signed implicit field graphs. Union samples are not exact signed distances.

use crate::{AuthoringError, DefinitionRevision};
use analytic_field::{
    AnalyticGradient, AxisAlignedBox, Gradient, GradientIssue, ScalarField, Sphere,
};
use serde::Deserialize;
use spatial_math::{MathError, Vec3};
use std::collections::BTreeMap;

mod spatial;
pub use spatial::{
    IdealRegionEstimate, RegionAabb, RegionExclusion, SampleSign, SolidEntryOutcome,
    SolidQueryIssue, SolidQueryReport, SolidRayOptions, SolidSweepOptions, classify_sample,
    estimate_region, exclude_region, sweep_sphere_exterior, trace_exterior_entry,
    trace_exterior_entry_report,
};

/// Maximum accepted source size for the research preview.
pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
/// Maximum distinct nodes. Evaluation may visit shared children more than once.
pub const MAX_NODES: usize = 32;
/// Maximum root-to-leaf depth, including both endpoints.
pub const MAX_DEPTH: usize = 12;
/// Maximum node visits for one sample, after shared references are expanded.
pub const MAX_VISITS: usize = 64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    format_version: u32,
    id: String,
    root: String,
    nodes: Vec<NodeSource>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum NodeSource {
    Sphere {
        id: String,
        radius: f64,
    },
    Box {
        id: String,
        half_extent: [f64; 3],
    },
    Translate {
        id: String,
        child: String,
        offset: [f64; 3],
    },
    Scale {
        id: String,
        child: String,
        factor: f64,
    },
    Union {
        id: String,
        left: String,
        right: String,
    },
    SmoothUnion {
        id: String,
        left: String,
        right: String,
        width: f64,
    },
}

impl NodeSource {
    fn id(&self) -> &str {
        match self {
            Self::Sphere { id, .. }
            | Self::Box { id, .. }
            | Self::Translate { id, .. }
            | Self::Scale { id, .. }
            | Self::Union { id, .. }
            | Self::SmoothUnion { id, .. } => id,
        }
    }
}

/// Validated operation. Child indices refer to the same immutable graph.
#[derive(Clone, Debug)]
pub enum Node {
    /// Local sphere with exact primitive SDF.
    Sphere(Sphere),
    /// Local axis-aligned box with exact primitive SDF.
    Box(AxisAlignedBox),
    /// Sample the child at `point - offset`.
    Translate {
        /// Referenced child node.
        child: u32,
        /// World-unit translation.
        offset: Vec3,
    },
    /// Sample the child at `point / factor`, then multiply its value by `factor`.
    Scale {
        /// Referenced child node.
        child: u32,
        /// Positive uniform scale.
        factor: f64,
    },
    /// Pointwise minimum, with sign-union semantics.
    Union {
        /// First branch.
        left: u32,
        /// Second branch.
        right: u32,
    },
    /// Polynomial smooth minimum of two exact-SDF children; signed and unit-Lipschitz, not exact SDF.
    SmoothUnion {
        /// First exact-SDF branch.
        left: u32,
        /// Second exact-SDF branch.
        right: u32,
        /// Positive smoothing width in world units.
        width: f64,
    },
}

/// Outward-rounded world-axis bounds of the ideal occupied set under IEEE-754 `f64` arithmetic.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// Componentwise lower corner.
    pub min: Vec3,
    /// Componentwise upper corner.
    pub max: Vec3,
}

/// Compiled immutable graph. It is independent of `WorldState` and has no collision authority.
#[derive(Clone, Debug)]
pub struct Graph {
    id: String,
    revision: DefinitionRevision,
    nodes: Vec<Node>,
    root: u32,
    visits: usize,
    bounds: Bounds,
    exact_sdf: bool,
}

/// Strongest supported ideal-arithmetic distance contract; neither variant certifies rounding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DistanceSemantics {
    /// One primitive under translation and positive uniform scale.
    ExactSdf,
    /// A union sign field with known global Lipschitz bound one, without exact SDF status.
    SignedUnitLipschitz,
}

impl Graph {
    /// Logical author-chosen identity, not a filesystem path.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// SHA-256 of the exact JSON bytes.
    pub fn revision(&self) -> DefinitionRevision {
        self.revision
    }
    /// Validated operations in source order.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    /// Root index in `nodes`.
    pub fn root(&self) -> u32 {
        self.root
    }
    /// Worst-case operation visits for one sample, including shared children.
    pub fn visits(&self) -> usize {
        self.visits
    }
    /// Strongest supported ideal-arithmetic distance contract.
    pub fn distance_semantics(&self) -> DistanceSemantics {
        if self.exact_sdf {
            DistanceSemantics::ExactSdf
        } else {
            DistanceSemantics::SignedUnitLipschitz
        }
    }
    /// Conservative bounds in world units, using ideal arithmetic.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }
    /// Global Euclidean Lipschitz upper bound in ideal arithmetic.
    pub fn ideal_lipschitz_bound(&self) -> f64 {
        1.0
    }

    /// CPU reference sample. Negative means inside; zero means on the mathematical zero set.
    pub fn sample(&self, point: Vec3) -> Result<f64, MathError> {
        self.sample_node(self.root as usize, point)
    }

    fn sample_node(&self, index: usize, point: Vec3) -> Result<f64, MathError> {
        let value = match self.nodes[index] {
            Node::Sphere(s) => s.sample(point)?,
            Node::Box(b) => b.sample(point)?,
            Node::Translate { child, offset } => {
                self.sample_node(child as usize, point.checked_sub(offset)?)?
            }
            Node::Scale { child, factor } => {
                self.sample_node(
                    child as usize,
                    Vec3::new(point.x() / factor, point.y() / factor, point.z() / factor)?,
                )? * factor
            }
            Node::Union { left, right } => self
                .sample_node(left as usize, point)?
                .min(self.sample_node(right as usize, point)?),
            Node::SmoothUnion { left, right, width } => {
                let a = self.sample_node(left as usize, point)?;
                let b = self.sample_node(right as usize, point)?;
                smooth_min(a, b, width)
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(MathError::NonFinite)
        }
    }

    /// Analytic gradient or a declared singularity/tie. No finite differencing is hidden here.
    pub fn gradient(&self, point: Vec3) -> Result<Gradient, MathError> {
        self.gradient_node(self.root as usize, point)
    }

    /// Derives numerical parameters for the bounded two-sphere union schema.
    /// The exact source revision and operation topology are retained.
    pub fn with_two_lobe_radii(&self, radii: [f64; 2]) -> Result<Self, AuthoringError> {
        if radii
            .iter()
            .any(|r| !r.is_finite() || !(0.1..=2.0).contains(r))
            || self.two_lobe_radii().is_none()
        {
            return Err(AuthoringError::Invalid("two-lobe parameter schema"));
        }
        let mut graph = self.clone();
        graph.nodes[0] = Node::Sphere(
            Sphere::new(radii[0]).map_err(|_| AuthoringError::Invalid("left radius"))?,
        );
        graph.nodes[2] = Node::Sphere(
            Sphere::new(radii[1]).map_err(|_| AuthoringError::Invalid("right radius"))?,
        );
        let [
            Node::Translate { offset: left, .. },
            Node::Translate { offset: right, .. },
        ] = [&graph.nodes[1], &graph.nodes[3]]
        else {
            unreachable!("schema checked above")
        };
        let l = Vec3::new(radii[0], radii[0], radii[0]).unwrap();
        let r = Vec3::new(radii[1], radii[1], radii[1]).unwrap();
        graph.bounds = Bounds {
            min: vec_min(
                outward_vec(l.negated(), *left, false, false)?,
                outward_vec(r.negated(), *right, false, false)?,
            )?,
            max: vec_max(
                outward_vec(l, *left, false, true)?,
                outward_vec(r, *right, false, true)?,
            )?,
        };
        Ok(graph)
    }

    /// Radii for the fixed two-sphere union schema, if the graph matches it.
    pub fn two_lobe_radii(&self) -> Option<[f64; 2]> {
        match self.nodes.as_slice() {
            [
                Node::Sphere(a),
                Node::Translate { child: 0, .. },
                Node::Sphere(b),
                Node::Translate { child: 2, .. },
                Node::Union { left: 1, right: 3 },
            ] if self.root == 4 => Some([a.radius(), b.radius()]),
            _ => None,
        }
    }
    /// Local point at the outward x-axis of a lobe; a bounded sampling site,
    /// not a certified closest point on an arbitrary implicit surface.
    pub fn two_lobe_outer_site(&self, lobe: usize, radius: f64) -> Result<Vec3, AuthoringError> {
        if self.two_lobe_radii().is_none()
            || lobe > 1
            || !radius.is_finite()
            || !(0.1..=2.0).contains(&radius)
        {
            return Err(AuthoringError::Invalid("two-lobe sample site"));
        }
        let Node::Translate { offset, .. } = self.nodes[1 + 2 * lobe] else {
            unreachable!("schema checked")
        };
        Vec3::new(
            offset.x() + if lobe == 0 { -radius } else { radius },
            offset.y(),
            offset.z(),
        )
        .map_err(|_| AuthoringError::Geometry("two-lobe sample site"))
    }

    fn gradient_node(&self, index: usize, point: Vec3) -> Result<Gradient, MathError> {
        match self.nodes[index] {
            Node::Sphere(s) => s.gradient(point),
            Node::Box(b) => b.gradient(point),
            Node::Translate { child, offset } => {
                self.gradient_node(child as usize, point.checked_sub(offset)?)
            }
            Node::Scale { child, factor } => self.gradient_node(
                child as usize,
                Vec3::new(point.x() / factor, point.y() / factor, point.z() / factor)?,
            ),
            Node::Union { left, right } => {
                let a = self.sample_node(left as usize, point)?;
                let b = self.sample_node(right as usize, point)?;
                if a < b {
                    self.gradient_node(left as usize, point)
                } else if b < a {
                    self.gradient_node(right as usize, point)
                } else {
                    Ok(Gradient::Undefined(GradientIssue::BranchTie))
                }
            }
            Node::SmoothUnion { left, right, width } => {
                let a = self.sample_node(left as usize, point)?;
                let b = self.sample_node(right as usize, point)?;
                let difference = a - b;
                if difference.abs() >= width {
                    return self.gradient_node(if a < b { left } else { right } as usize, point);
                }
                let left_weight = 0.5 - difference / (2.0 * width);
                let right_weight = 1.0 - left_weight;
                match (
                    self.gradient_node(left as usize, point)?,
                    self.gradient_node(right as usize, point)?,
                ) {
                    (Gradient::Defined(a), Gradient::Defined(b)) => Ok(Gradient::Defined(
                        a.checked_scale(left_weight)?
                            .checked_add(b.checked_scale(right_weight)?)?,
                    )),
                    (Gradient::Undefined(issue), _) | (_, Gradient::Undefined(issue)) => {
                        Ok(Gradient::Undefined(issue))
                    }
                }
            }
        }
    }
}

fn smooth_min(a: f64, b: f64, width: f64) -> f64 {
    let blend = (width - (a - b).abs()).max(0.0) / width;
    a.min(b) - width * blend * blend / 4.0
}

fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn coordinate(v: [f64; 3]) -> Result<Vec3, AuthoringError> {
    if v.iter().any(|x| !x.is_finite() || x.abs() > 1000.0) {
        return Err(AuthoringError::Invalid("field coordinate"));
    }
    Vec3::new(v[0], v[1], v[2]).map_err(|_| AuthoringError::Invalid("field coordinate"))
}
fn positive(value: f64) -> Result<f64, AuthoringError> {
    if !value.is_finite() || !(0.01..=100.0).contains(&value) {
        return Err(AuthoringError::Invalid("field size or scale"));
    }
    Ok(value)
}
fn vec_min(a: Vec3, b: Vec3) -> Result<Vec3, AuthoringError> {
    Vec3::new(a.x().min(b.x()), a.y().min(b.y()), a.z().min(b.z()))
        .map_err(|_| AuthoringError::Geometry("field bounds"))
}
fn vec_max(a: Vec3, b: Vec3) -> Result<Vec3, AuthoringError> {
    Vec3::new(a.x().max(b.x()), a.y().max(b.y()), a.z().max(b.z()))
        .map_err(|_| AuthoringError::Geometry("field bounds"))
}
fn outward_vec(a: Vec3, b: Vec3, multiply: bool, upper: bool) -> Result<Vec3, AuthoringError> {
    let values = [(a.x(), b.x()), (a.y(), b.y()), (a.z(), b.z())];
    let mut rounded = [0.0; 3];
    for (index, (left, right)) in values.into_iter().enumerate() {
        let value = if multiply { left * right } else { left + right };
        if !value.is_finite() {
            return Err(AuthoringError::Geometry("field bounds"));
        }
        rounded[index] = if upper {
            value.next_up()
        } else {
            value.next_down()
        };
    }
    Vec3::new(rounded[0], rounded[1], rounded[2])
        .map_err(|_| AuthoringError::Geometry("field bounds"))
}

/// Parse and validate a version-one graph. Rejection returns no partially compiled graph.
pub fn compile_json(bytes: &[u8]) -> Result<Graph, AuthoringError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(AuthoringError::InputTooLarge);
    }
    let source: Source = serde_json::from_slice(bytes).map_err(AuthoringError::Json)?;
    if source.format_version != 1 {
        return Err(AuthoringError::UnsupportedVersion(source.format_version));
    }
    if !name(&source.id) || !name(&source.root) {
        return Err(AuthoringError::Invalid("field identity"));
    }
    if source.nodes.is_empty() || source.nodes.len() > MAX_NODES {
        return Err(AuthoringError::Budget("field nodes"));
    }
    let mut ids = BTreeMap::new();
    for (index, node) in source.nodes.iter().enumerate() {
        if !name(node.id()) || ids.insert(node.id(), index).is_some() {
            return Err(AuthoringError::Invalid("field node id"));
        }
    }
    let index = |id: &str| -> Result<u32, AuthoringError> {
        ids.get(id)
            .copied()
            .map(|i| i as u32)
            .ok_or(AuthoringError::Invalid("field connection"))
    };
    let root = index(&source.root)?;
    let mut nodes = Vec::with_capacity(source.nodes.len());
    for node in &source.nodes {
        nodes.push(match node {
            NodeSource::Sphere { radius, .. } => Node::Sphere(
                Sphere::new(positive(*radius)?)
                    .map_err(|_| AuthoringError::Invalid("sphere radius"))?,
            ),
            NodeSource::Box { half_extent, .. } => {
                let e = coordinate(*half_extent)?;
                if [e.x(), e.y(), e.z()].iter().any(|x| *x < 0.01) {
                    return Err(AuthoringError::Invalid("box half extent"));
                }
                Node::Box(
                    AxisAlignedBox::new(e)
                        .map_err(|_| AuthoringError::Invalid("box half extent"))?,
                )
            }
            NodeSource::Translate { child, offset, .. } => Node::Translate {
                child: index(child)?,
                offset: coordinate(*offset)?,
            },
            NodeSource::Scale { child, factor, .. } => Node::Scale {
                child: index(child)?,
                factor: positive(*factor)?,
            },
            NodeSource::Union { left, right, .. } => Node::Union {
                left: index(left)?,
                right: index(right)?,
            },
            NodeSource::SmoothUnion {
                left, right, width, ..
            } => Node::SmoothUnion {
                left: index(left)?,
                right: index(right)?,
                width: positive(*width)?,
            },
        });
    }
    let mut state = vec![0u8; nodes.len()];
    let mut visits = 0;
    fn walk(
        nodes: &[Node],
        index: usize,
        depth: usize,
        state: &mut [u8],
        visits: &mut usize,
    ) -> Result<(), AuthoringError> {
        if depth > MAX_DEPTH {
            return Err(AuthoringError::Budget("field depth"));
        }
        *visits += 1;
        if *visits > MAX_VISITS {
            return Err(AuthoringError::Budget("field evaluation"));
        }
        if state[index] == 1 {
            return Err(AuthoringError::Invalid("field cycle"));
        }
        state[index] = 1;
        match nodes[index] {
            Node::Translate { child, .. } | Node::Scale { child, .. } => {
                walk(nodes, child as usize, depth + 1, state, visits)?
            }
            Node::Union { left, right } | Node::SmoothUnion { left, right, .. } => {
                walk(nodes, left as usize, depth + 1, state, visits)?;
                walk(nodes, right as usize, depth + 1, state, visits)?;
            }
            _ => {}
        }
        state[index] = 2;
        Ok(())
    }
    walk(&nodes, root as usize, 1, &mut state, &mut visits)?;
    if state.contains(&0) {
        return Err(AuthoringError::Invalid("unreachable field node"));
    }
    fn properties(nodes: &[Node], index: usize) -> Result<(Bounds, bool), AuthoringError> {
        Ok(match nodes[index] {
            Node::Sphere(s) => {
                let r = s.radius();
                (
                    Bounds {
                        min: Vec3::new(-r, -r, -r).unwrap(),
                        max: Vec3::new(r, r, r).unwrap(),
                    },
                    true,
                )
            }
            Node::Box(b) => (
                Bounds {
                    min: b.half_extent().negated(),
                    max: b.half_extent(),
                },
                true,
            ),
            Node::Translate { child, offset } => {
                let (b, exact) = properties(nodes, child as usize)?;
                (
                    Bounds {
                        min: outward_vec(b.min, offset, false, false)?,
                        max: outward_vec(b.max, offset, false, true)?,
                    },
                    exact,
                )
            }
            Node::Scale { child, factor } => {
                let (b, exact) = properties(nodes, child as usize)?;
                let factor = Vec3::new(factor, factor, factor).unwrap();
                (
                    Bounds {
                        min: outward_vec(b.min, factor, true, false)?,
                        max: outward_vec(b.max, factor, true, true)?,
                    },
                    exact,
                )
            }
            Node::Union { left, right } => {
                let (a, _) = properties(nodes, left as usize)?;
                let (b, _) = properties(nodes, right as usize)?;
                (
                    Bounds {
                        min: vec_min(a.min, b.min)?,
                        max: vec_max(a.max, b.max)?,
                    },
                    false,
                )
            }
            Node::SmoothUnion { left, right, width } => {
                let (a, a_exact) = properties(nodes, left as usize)?;
                let (b, b_exact) = properties(nodes, right as usize)?;
                if !a_exact || !b_exact {
                    return Err(AuthoringError::Invalid(
                        "smooth union requires exact-SDF children",
                    ));
                }
                let margin = (width / 4.0).next_up();
                let lower = Vec3::new(-margin, -margin, -margin).unwrap();
                let upper = Vec3::new(margin, margin, margin).unwrap();
                (
                    Bounds {
                        min: outward_vec(vec_min(a.min, b.min)?, lower, false, false)?,
                        max: outward_vec(vec_max(a.max, b.max)?, upper, false, true)?,
                    },
                    false,
                )
            }
        })
    }
    let (bounds, exact_sdf) = properties(&nodes, root as usize)?;
    // Keep the old geometric ceiling usable when outward rounding adds a few ulps.
    let mut admission_limit = 10_000.0_f64;
    for _ in 0..(2 * MAX_DEPTH) {
        admission_limit = admission_limit.next_up();
    }
    if [
        bounds.min.x(),
        bounds.min.y(),
        bounds.min.z(),
        bounds.max.x(),
        bounds.max.y(),
        bounds.max.z(),
    ]
    .iter()
    .any(|x| x.abs() > admission_limit)
    {
        return Err(AuthoringError::Budget("field world bounds"));
    }
    Ok(Graph {
        id: source.id,
        revision: DefinitionRevision::of(bytes),
        nodes,
        root,
        visits,
        bounds,
        exact_sdf,
    })
}

/// Ray query policy for an exterior start, in world units.
#[derive(Clone, Copy, Debug)]
pub struct TraceOptions {
    /// Inclusive near distance.
    pub near: f64,
    /// Inclusive far distance.
    pub far: f64,
    /// Positive sample threshold for an approximate hit.
    pub tolerance: f64,
    /// Maximum field samples.
    pub iterations: u32,
    /// Fraction of the ideal `f/L` step; must be in `(0, 1]`.
    pub safety: f64,
}

/// Explicit outcomes of the bounded numerical query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TraceOutcome {
    /// Positive sample within tolerance of the zero set under ideal arithmetic.
    Hit {
        /// Approximate distance from ray origin.
        distance: f64,
    },
    /// Ideal step passes the far limit while still outside.
    Miss,
    /// Floating point or interior-start ambiguity.
    Uncertain(TraceIssue),
    /// Iteration budget reached before a decision.
    Exhausted,
}

/// Reason a bounded CPU query could not classify the ray.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceIssue {
    /// The initial or later sample was in the negative region; only exterior entry is traced.
    Inside,
    /// Field evaluation failed or produced a nonfinite result.
    Numeric,
    /// The next representable ray distance did not advance.
    NoProgress,
}

/// Sphere tracing with an ideal 1-Lipschitz step; no certified floating-point guarantee.
pub fn trace(
    graph: &Graph,
    origin: Vec3,
    direction: Vec3,
    options: TraceOptions,
) -> Result<TraceOutcome, MathError> {
    if !options.near.is_finite()
        || !options.far.is_finite()
        || !options.tolerance.is_finite()
        || !options.safety.is_finite()
    {
        return Err(MathError::NonFinite);
    }
    if options.near < 0.0
        || options.far <= options.near
        || !(0.00001..=0.1).contains(&options.tolerance)
        || !(0.0..=1.0).contains(&options.safety)
        || options.safety == 0.0
        || options.iterations == 0
        || options.iterations > 512
    {
        return Err(MathError::OutOfRange);
    }
    let direction = direction.normalized()?;
    let mut distance = options.near;
    for _ in 0..options.iterations {
        let point = origin.checked_add(direction.checked_scale(distance)?)?;
        let value = match graph.sample(point) {
            Ok(v) => v,
            Err(_) => return Ok(TraceOutcome::Uncertain(TraceIssue::Numeric)),
        };
        if value < 0.0 {
            return Ok(TraceOutcome::Uncertain(TraceIssue::Inside));
        }
        if value <= options.tolerance {
            return Ok(TraceOutcome::Hit { distance });
        }
        let next = distance + value * options.safety;
        if !next.is_finite() || next <= distance {
            return Ok(TraceOutcome::Uncertain(TraceIssue::NoProgress));
        }
        if next > options.far {
            return Ok(TraceOutcome::Miss);
        }
        distance = next;
    }
    Ok(TraceOutcome::Exhausted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_lobe_parameters_preserve_revision_and_expand_bounds() {
        let source = include_bytes!("../../../examples/field-graph/definitions/twin.json");
        let graph = compile_json(source).unwrap();
        // The legacy graph shares one sphere node, so it is not this schema.
        assert!(graph.with_two_lobe_radii([0.5, 0.8]).is_err());
        let source = br#"{"format_version":1,"id":"morph","root":"shape","nodes":[{"op":"sphere","id":"a","radius":0.5},{"op":"translate","id":"left","child":"a","offset":[-0.65,0,0]},{"op":"sphere","id":"b","radius":0.5},{"op":"translate","id":"right","child":"b","offset":[0.65,0,0]},{"op":"union","id":"shape","left":"left","right":"right"}]}"#;
        let graph = compile_json(source).unwrap();
        let changed = graph.with_two_lobe_radii([0.7, 1.1]).unwrap();
        assert_eq!(changed.revision(), graph.revision());
        assert!(changed.bounds().min.x() < graph.bounds().min.x());
        assert!(changed.bounds().max.x() > graph.bounds().max.x());
        assert!(changed.sample(Vec3::new(1.6, 0.0, 0.0).unwrap()).unwrap() < 0.2);
        assert!(graph.with_two_lobe_radii([f64::NAN, 1.0]).is_err());
    }
    use analytic_field::Field;
    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }
    const SOURCE: &str = r#"{"format_version":1,"id":"test","root":"both","nodes":[{"op":"sphere","id":"a","radius":1.0},{"op":"translate","id":"left","child":"a","offset":[-0.5,0,0]},{"op":"box","id":"b","half_extent":[0.5,0.5,0.5]},{"op":"union","id":"both","left":"left","right":"b"}]}"#;
    #[test]
    fn evaluated_graph_contract() {
        let g = compile_json(SOURCE.as_bytes()).unwrap();
        assert_eq!(g.nodes().len(), 4);
        assert_eq!(
            g.distance_semantics(),
            DistanceSemantics::SignedUnitLipschitz
        );
        assert_eq!(g.ideal_lipschitz_bound(), 1.0);
        assert_eq!(g.sample(v(-0.5, 0.0, 0.0)).unwrap(), -1.0);
        assert_eq!(g.sample(v(2.0, 0.0, 0.0)).unwrap(), 1.5);
        assert!(g.bounds().min.x() <= -1.5);
        assert!(g.bounds().min.y() <= -1.0);
        assert!(g.bounds().min.z() <= -1.0);
        assert_eq!(
            g.gradient(v(-0.5, 0.0, 0.0)).unwrap(),
            Gradient::Undefined(GradientIssue::SphereCenter)
        );
        assert_eq!(
            g.gradient(v(0.5, 0.0, 0.0)).unwrap(),
            Gradient::Undefined(GradientIssue::BranchTie)
        );
        let options = TraceOptions {
            near: 0.001,
            far: 10.0,
            tolerance: 0.0001,
            iterations: 128,
            safety: 0.8,
        };
        assert!(
            matches!(trace(&g,v(-0.5,0.0,-3.0),v(0.0,0.0,1.0),options).unwrap(),TraceOutcome::Hit{distance} if (distance-2.0).abs()<0.001)
        );
        assert_eq!(
            trace(&g, v(5.0, 5.0, -3.0), v(0.0, 0.0, 1.0), options).unwrap(),
            TraceOutcome::Miss
        );
        assert_eq!(
            trace(&g, v(-0.5, 0.0, 0.0), v(1.0, 0.0, 0.0), options).unwrap(),
            TraceOutcome::Uncertain(TraceIssue::Inside)
        );
        assert_eq!(
            trace(
                &g,
                v(-0.5, 0.0, -3.0),
                v(0.0, 0.0, 1.0),
                TraceOptions {
                    iterations: 1,
                    ..options
                }
            )
            .unwrap(),
            TraceOutcome::Exhausted
        );
    }
    #[test]
    fn rejects_untrusted_graphs() {
        for bad in [
            SOURCE.replace("\"radius\":1.0", "\"radius\":-1.0"),
            SOURCE.replace("\"child\":\"a\"", "\"child\":\"missing\""),
            SOURCE.replace("\"child\":\"a\"", "\"child\":\"both\""),
            SOURCE.replace("\"format_version\":1", "\"format_version\":2"),
            SOURCE.replace("\"id\":\"b\"", "\"id\":\"a\""),
            SOURCE.replace("\"radius\":1.0", "\"radius\":1e999"),
            SOURCE.replace("\"radius\":1.0", "\"radius\":1.0,\"extra\":1"),
        ] {
            assert!(compile_json(bad.as_bytes()).is_err(), "{bad}");
        }
        assert!(compile_json(&vec![b' '; MAX_SOURCE_BYTES + 1]).is_err());
    }
    #[test]
    fn transforms_and_scale() {
        let source = r#"{"format_version":1,"id":"scaled","root":"s","nodes":[{"op":"sphere","id":"ball","radius":1},{"op":"scale","id":"z","child":"ball","factor":2},{"op":"translate","id":"s","child":"z","offset":[2,0,0]}]}"#;
        let g = compile_json(source.as_bytes()).unwrap();
        assert_eq!(g.distance_semantics(), DistanceSemantics::ExactSdf);
        assert_eq!(g.sample(v(4.0, 0.0, 0.0)).unwrap(), 0.0);
        assert!(g.bounds().max.x() >= 4.0);
        assert!(g.bounds().max.y() >= 2.0);
        assert!(g.bounds().max.z() >= 2.0);
    }
    #[test]
    fn bounded_depth_visits_and_ambiguous_zero_sets() {
        let mut nodes = vec![r#"{"op":"sphere","id":"s","radius":1}"#.to_string()];
        for i in 0..MAX_DEPTH {
            let child = if i == 0 {
                "s".to_owned()
            } else {
                format!("t{}", i - 1)
            };
            nodes.push(format!(
                r#"{{"op":"translate","id":"t{i}","child":"{child}","offset":[0,0,0]}}"#
            ));
        }
        let source = format!(
            r#"{{"format_version":1,"id":"deep","root":"t{}","nodes":[{}]}}"#,
            MAX_DEPTH - 1,
            nodes.join(",")
        );
        assert!(matches!(
            compile_json(source.as_bytes()),
            Err(AuthoringError::Budget("field depth"))
        ));
        let mut nodes = vec![r#"{"op":"sphere","id":"s","radius":1}"#.to_string()];
        for i in 0..6 {
            let child = if i == 0 {
                "s".to_owned()
            } else {
                format!("u{}", i - 1)
            };
            nodes.push(format!(
                r#"{{"op":"union","id":"u{i}","left":"{child}","right":"{child}"}}"#
            ));
        }
        let source = format!(
            r#"{{"format_version":1,"id":"wide","root":"u5","nodes":[{}]}}"#,
            nodes.join(",")
        );
        assert!(matches!(
            compile_json(source.as_bytes()),
            Err(AuthoringError::Budget("field evaluation"))
        ));
        let sphere = Field::Sphere(Sphere::new(1.0).unwrap());
        let empty_shell = Field::difference(sphere.clone(), sphere);
        assert_eq!(empty_shell.sample(v(1.0, 0.0, 0.0)).unwrap(), 0.0);
        assert!(empty_shell.sample(v(0.0, 0.0, 0.0)).unwrap() > 0.0);
        assert!(empty_shell.sample(v(2.0, 0.0, 0.0)).unwrap() > 0.0);
        // This zero set has no negative interior; the graph deliberately excludes difference.
    }
    #[test]
    fn primitive_reference_and_numerical_limits() {
        for radius in [0.01, 1.0, 100.0] {
            let source = format!(
                r#"{{"format_version":1,"id":"ball","root":"s","nodes":[{{"op":"sphere","id":"s","radius":{radius}}}]}}"#
            );
            let g = compile_json(source.as_bytes()).unwrap();
            assert_eq!(g.distance_semantics(), DistanceSemantics::ExactSdf);
            assert_eq!(g.sample(v(0.0, 0.0, 0.0)).unwrap(), -radius);
            assert_eq!(g.sample(v(radius, 0.0, 0.0)).unwrap(), 0.0);
            assert_eq!(g.sample(v(radius * 2.0, 0.0, 0.0)).unwrap(), radius);
        }
        let g = compile_json(SOURCE.as_bytes()).unwrap();
        assert_eq!(
            g.sample(v(f64::MAX, f64::MAX, 0.0)),
            Err(MathError::NonFinite)
        );
        let options = TraceOptions {
            near: 0.0,
            far: 5.0,
            tolerance: 0.001,
            iterations: 8,
            safety: 0.8,
        };
        assert_eq!(
            trace(
                &g,
                v(0.0, 0.0, -3.0),
                v(0.0, 0.0, 1.0),
                TraceOptions {
                    iterations: 0,
                    ..options
                }
            ),
            Err(MathError::OutOfRange)
        );
        assert_eq!(
            trace(
                &g,
                v(0.0, 0.0, -3.0),
                v(0.0, 0.0, 1.0),
                TraceOptions {
                    safety: f64::NAN,
                    ..options
                }
            ),
            Err(MathError::NonFinite)
        );
    }
}
