//! Spatial research queries for the closed solid represented by the supported graph vocabulary.
//! Field samples remain approximate. The separate bounds exclusion uses outward-rounded `f64`.

use super::{Graph, TraceOptions};
use spatial_math::{MathError, Vec3};

/// Observed sign of a finite CPU sample. This is not certified occupied-set membership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleSign {
    /// The computed sample is below the negative tolerance.
    Negative,
    /// The computed sample is above the positive tolerance.
    Positive,
    /// The computed sample is close to zero; it may be an internal zero artifact.
    BoundaryCandidate,
    /// Evaluation failed for finite input.
    Indeterminate,
}

/// Classifies a computed `f64` sample without claiming a topological-boundary proof.
pub fn classify_sample(
    graph: &Graph,
    point: Vec3,
    tolerance: f64,
) -> Result<SampleSign, MathError> {
    if !tolerance.is_finite() {
        return Err(MathError::NonFinite);
    }
    if !(0.0..=0.1).contains(&tolerance) {
        return Err(MathError::OutOfRange);
    }
    Ok(match graph.sample(point) {
        Ok(value) if value < -tolerance => SampleSign::Negative,
        Ok(value) if value > tolerance => SampleSign::Positive,
        Ok(_) => SampleSign::BoundaryCandidate,
        Err(_) => SampleSign::Indeterminate,
    })
}

/// Finite axis-aligned query region in supported world coordinates.
#[derive(Clone, Copy, Debug)]
pub struct RegionAabb {
    min: Vec3,
    max: Vec3,
}
impl RegionAabb {
    /// Rejects reversed axes and coordinates outside ±10,000 world units.
    pub fn new(min: Vec3, max: Vec3) -> Result<Self, MathError> {
        for (lo, hi) in [(min.x(), max.x()), (min.y(), max.y()), (min.z(), max.z())] {
            if lo > hi {
                return Err(MathError::ReversedInterval);
            }
            if lo.abs() > 10_000.0 || hi.abs() > 10_000.0 {
                return Err(MathError::OutOfRange);
            }
        }
        Ok(Self { min, max })
    }
    /// Lower corner.
    pub fn min(self) -> Vec3 {
        self.min
    }
    /// Upper corner.
    pub fn max(self) -> Vec3 {
        self.max
    }
}

/// Occupancy exclusion using only outward-rounded analytic graph bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionExclusion {
    /// The closed query region cannot intersect the ideal occupied set, under IEEE-754 `f64` rounding.
    ConfirmedOutside,
    /// Bounds overlap; occupancy and boundary status remain unknown.
    Unknown,
}

/// Safely excludes a region when one world axis is strictly separated from graph bounds.
/// It makes no claim about zeros or occupancy within overlapping bounds.
pub fn exclude_region(graph: &Graph, region: RegionAabb) -> RegionExclusion {
    let bounds = graph.bounds();
    let min = region.min();
    let max = region.max();
    if [
        (min.x(), max.x(), bounds.min.x(), bounds.max.x()),
        (min.y(), max.y(), bounds.min.y(), bounds.max.y()),
        (min.z(), max.z(), bounds.min.z(), bounds.max.z()),
    ]
    .into_iter()
    .any(|(lo, hi, bound_lo, bound_hi)| hi < bound_lo || lo > bound_hi)
    {
        RegionExclusion::ConfirmedOutside
    } else {
        RegionExclusion::Unknown
    }
}

fn rounded_ray_axis(origin: f64, direction: f64, parameter: f64) -> Option<(f64, f64)> {
    let product = direction * parameter;
    if !product.is_finite() {
        return None;
    }
    let lower_sum = origin + product.next_down();
    let upper_sum = origin + product.next_up();
    if !lower_sum.is_finite() || !upper_sum.is_finite() {
        return None;
    }
    let lo = lower_sum.next_down();
    let hi = upper_sum.next_up();
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

fn ray_segment_excluded(graph: &Graph, origin: Vec3, direction: Vec3, near: f64, far: f64) -> bool {
    let bounds = graph.bounds();
    [
        (origin.x(), direction.x(), bounds.min.x(), bounds.max.x()),
        (origin.y(), direction.y(), bounds.min.y(), bounds.max.y()),
        (origin.z(), direction.z(), bounds.min.z(), bounds.max.z()),
    ]
    .into_iter()
    .any(|(o, d, bound_lo, bound_hi)| {
        match (rounded_ray_axis(o, d, near), rounded_ray_axis(o, d, far)) {
            (Some(a), Some(b)) => a.1.max(b.1) < bound_lo || a.0.min(b.0) > bound_hi,
            _ => false,
        }
    })
}

/// Computed `F(center) ± radius` using the graph's ideal 1-Lipschitz model.
/// No variant certifies the sign of the ideal expression or encloses floating-point values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IdealRegionEstimate {
    /// Computed lower estimate is positive; ideal arithmetic would exclude occupancy and zero.
    Outside {
        /// Computed ideal-model lower estimate in world units.
        lower: f64,
    },
    /// Computed upper estimate is negative; ideal arithmetic would imply strict occupancy.
    Inside {
        /// Computed ideal-model upper estimate in world units.
        upper: f64,
    },
    /// The bound crosses zero or evaluation was numerically indeterminate.
    Unknown,
}

/// Estimate a region using its enclosing-ball radius and the graph's ideal Lipschitz bound.
/// `Outside` and `Inside` must never drive authoritative pruning without certified arithmetic.
pub fn estimate_region(graph: &Graph, region: RegionAabb) -> IdealRegionEstimate {
    let lo = region.min();
    let hi = region.max();
    let center = match Vec3::new(
        lo.x() / 2.0 + hi.x() / 2.0,
        lo.y() / 2.0 + hi.y() / 2.0,
        lo.z() / 2.0 + hi.z() / 2.0,
    ) {
        Ok(v) => v,
        Err(_) => return IdealRegionEstimate::Unknown,
    };
    let radius = ((hi.x() - lo.x()) / 2.0)
        .hypot((hi.y() - lo.y()) / 2.0)
        .hypot((hi.z() - lo.z()) / 2.0);
    let value = match graph.sample(center) {
        Ok(v) => v,
        Err(_) => return IdealRegionEstimate::Unknown,
    };
    let spread = radius * graph.ideal_lipschitz_bound();
    let (lower, upper) = (value - spread, value + spread);
    if !lower.is_finite() || !upper.is_finite() {
        return IdealRegionEstimate::Unknown;
    }
    if lower > 0.0 {
        IdealRegionEstimate::Outside { lower }
    } else if upper < 0.0 {
        IdealRegionEstimate::Inside { upper }
    } else {
        IdealRegionEstimate::Unknown
    }
}

/// Bounded exterior-entry policy. `max_work` counts expanded node visits, not shader instructions.
#[derive(Clone, Copy, Debug)]
pub struct SolidRayOptions {
    /// Existing near/far, tolerance, safety, and iteration limits.
    pub ray: TraceOptions,
    /// Positive maximum expanded node visits, including the start classification.
    pub max_work: usize,
}

/// Reason a physical entry cannot be selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolidQueryIssue {
    /// Start sample is negative: exiting an occupied region is unsupported.
    StartsInside,
    /// Start lies within hit tolerance of the zero set; exterior status is unverified.
    StartNearZero,
    /// Sample or ray arithmetic was invalid or could not advance.
    Numeric,
    /// Expanded graph-work budget was reached.
    WorkBudget,
    /// An approximate Lipschitz step passed `far`, without a certified segment exclusion.
    UncertifiedAdvance,
}

/// Restricted entry of a finite ray into the supported closed union of primitives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SolidEntryOutcome {
    /// Approximate first external-boundary entry, conditional on ideal arithmetic and an exterior start.
    ApproximateEntry {
        /// Approximate ray parameter in world units.
        distance: f64,
    },
    /// Outward-rounded ray segment bounds do not overlap outward-rounded occupied bounds.
    BoundsMiss,
    /// Unsupported start or numerical/work uncertainty.
    Indeterminate(SolidQueryIssue),
    /// Sample iteration cap was reached; never a miss.
    Exhausted,
}

/// Query outcome with exact sample-call count and charged worst-case graph visits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolidQueryReport {
    /// The same classification returned by `trace_exterior_entry`.
    pub outcome: SolidEntryOutcome,
    /// Number of calls to the graph's CPU sampler.
    pub samples: u32,
    /// Sum of validated worst-case visits charged for those samples.
    pub charged_visits: usize,
}

fn report(outcome: SolidEntryOutcome, samples: u32, charged_visits: usize) -> SolidQueryReport {
    SolidQueryReport {
        outcome,
        samples,
        charged_visits,
    }
}

/// Trace only the first entry from a sampled exterior start. The supported graph grammar
/// makes a finite union of nondegenerate closed primitives; an interior shared-face zero
/// cannot be encountered before the external boundary along an exterior ray in ideal arithmetic.
/// This `f64` result is not a numerically certified collision or picking answer.
pub fn trace_exterior_entry(
    graph: &Graph,
    origin: Vec3,
    direction: Vec3,
    options: SolidRayOptions,
) -> Result<SolidEntryOutcome, MathError> {
    Ok(trace_exterior_entry_report(graph, origin, direction, options)?.outcome)
}

/// The same bounded entry query with reproducible work diagnostics.
pub fn trace_exterior_entry_report(
    graph: &Graph,
    origin: Vec3,
    direction: Vec3,
    options: SolidRayOptions,
) -> Result<SolidQueryReport, MathError> {
    let o = options.ray;
    if !o.near.is_finite()
        || !o.far.is_finite()
        || !o.tolerance.is_finite()
        || !o.safety.is_finite()
    {
        return Err(MathError::NonFinite);
    }
    if o.near < 0.0
        || o.far <= o.near
        || !(0.00001..=0.1).contains(&o.tolerance)
        || !(0.0..=1.0).contains(&o.safety)
        || o.safety == 0.0
        || o.iterations == 0
        || o.iterations > 512
        || options.max_work == 0
        || options.max_work > 1_000_000
    {
        return Err(MathError::OutOfRange);
    }
    let direction = direction.normalized()?;
    let mut work = 0usize;
    let mut samples = 0u32;
    let mut distance = o.near;
    for step in 0..o.iterations {
        if work.saturating_add(graph.visits()) > options.max_work {
            return Ok(report(
                SolidEntryOutcome::Indeterminate(SolidQueryIssue::WorkBudget),
                samples,
                work,
            ));
        }
        let point = match direction
            .checked_scale(distance)
            .and_then(|step| origin.checked_add(step))
        {
            Ok(v) => v,
            Err(_) => {
                return Ok(report(
                    SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric),
                    samples,
                    work,
                ));
            }
        };
        work += graph.visits();
        samples += 1;
        let value = match graph.sample(point) {
            Ok(v) => v,
            Err(_) => {
                return Ok(report(
                    SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric),
                    samples,
                    work,
                ));
            }
        };
        if step == 0 {
            if value < 0.0 {
                return Ok(report(
                    SolidEntryOutcome::Indeterminate(SolidQueryIssue::StartsInside),
                    samples,
                    work,
                ));
            }
            if value <= o.tolerance {
                return Ok(report(
                    SolidEntryOutcome::Indeterminate(SolidQueryIssue::StartNearZero),
                    samples,
                    work,
                ));
            }
            if ray_segment_excluded(graph, origin, direction, o.near, o.far) {
                return Ok(report(SolidEntryOutcome::BoundsMiss, samples, work));
            }
        } else {
            if value < 0.0 {
                return Ok(report(
                    SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric),
                    samples,
                    work,
                ));
            }
            if value <= o.tolerance {
                return Ok(report(
                    SolidEntryOutcome::ApproximateEntry { distance },
                    samples,
                    work,
                ));
            }
        }
        let next = distance + value * o.safety;
        if !next.is_finite() || next <= distance {
            return Ok(report(
                SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric),
                samples,
                work,
            ));
        }
        if next > o.far {
            return Ok(report(
                SolidEntryOutcome::Indeterminate(SolidQueryIssue::UncertifiedAdvance),
                samples,
                work,
            ));
        }
        distance = next;
    }
    Ok(report(SolidEntryOutcome::Exhausted, samples, work))
}
