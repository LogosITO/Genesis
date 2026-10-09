//! Spatial research queries for the closed solid represented by the supported graph vocabulary.
//! Every exclusion here is conditional on ideal arithmetic; `f64` is not outward rounded.

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
}

/// Restricted entry of a finite ray into the supported closed union of primitives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SolidEntryOutcome {
    /// Approximate first external-boundary entry, conditional on ideal arithmetic and an exterior start.
    ApproximateEntry {
        /// Approximate ray parameter in world units.
        distance: f64,
    },
    /// The computed step passed `far`; absence of entry is conditional on ideal arithmetic.
    IdealModelMiss,
    /// Unsupported start or numerical/work uncertainty.
    Indeterminate(SolidQueryIssue),
    /// Sample iteration cap was reached; never a miss.
    Exhausted,
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
    let mut distance = o.near;
    for step in 0..o.iterations {
        if work.saturating_add(graph.visits()) > options.max_work {
            return Ok(SolidEntryOutcome::Indeterminate(
                SolidQueryIssue::WorkBudget,
            ));
        }
        work += graph.visits();
        let point = match origin.checked_add(direction.checked_scale(distance)?) {
            Ok(v) => v,
            Err(_) => return Ok(SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric)),
        };
        let value = match graph.sample(point) {
            Ok(v) => v,
            Err(_) => return Ok(SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric)),
        };
        if step == 0 {
            if value < 0.0 {
                return Ok(SolidEntryOutcome::Indeterminate(
                    SolidQueryIssue::StartsInside,
                ));
            }
            if value <= o.tolerance {
                return Ok(SolidEntryOutcome::Indeterminate(
                    SolidQueryIssue::StartNearZero,
                ));
            }
        } else {
            if value < 0.0 {
                return Ok(SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric));
            }
            if value <= o.tolerance {
                return Ok(SolidEntryOutcome::ApproximateEntry { distance });
            }
        }
        let next = distance + value * o.safety;
        if !next.is_finite() || next <= distance {
            return Ok(SolidEntryOutcome::Indeterminate(SolidQueryIssue::Numeric));
        }
        if next > o.far {
            return Ok(SolidEntryOutcome::IdealModelMiss);
        }
        distance = next;
    }
    Ok(SolidEntryOutcome::Exhausted)
}
