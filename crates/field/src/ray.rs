//! Bounded CPU reference tracing for exact SDF primitives only.

use crate::ExactSdf;
use spatial_math::{MathError, Vec3};

/// A finite ray segment with a unit direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    origin: Vec3,
    direction: Vec3,
    max_distance: f64,
}

impl Ray {
    /// Constructs a ray from a finite origin, nonzero direction, and positive segment length.
    pub fn new(origin: Vec3, direction: Vec3, max_distance: f64) -> Result<Self, MathError> {
        if !max_distance.is_finite() {
            return Err(MathError::NonFinite);
        }
        if max_distance <= 0.0 {
            return Err(MathError::NonPositive);
        }
        Ok(Self {
            origin,
            direction: direction.normalized()?,
            max_distance,
        })
    }

    /// Maximum distance along the ray in world units.
    pub fn max_distance(self) -> f64 {
        self.max_distance
    }

    fn point_at(self, distance: f64) -> Result<Vec3, MathError> {
        self.origin
            .checked_add(self.direction.checked_scale(distance)?)
    }
}

/// Explicit numerical tolerance and iteration limit for tracing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayOptions {
    hit_tolerance: f64,
    max_iterations: usize,
}

impl RayOptions {
    /// Tolerance must be finite and positive; iteration count must be nonzero.
    pub fn new(hit_tolerance: f64, max_iterations: usize) -> Result<Self, MathError> {
        if !hit_tolerance.is_finite() {
            return Err(MathError::NonFinite);
        }
        if hit_tolerance <= 0.0 || max_iterations == 0 {
            return Err(MathError::NonPositive);
        }
        Ok(Self {
            hit_tolerance,
            max_iterations,
        })
    }
}

/// Result of a bounded approximate CPU ray query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayOutcome {
    /// Sampled a point no farther than the requested tolerance from the zero set in ideal arithmetic.
    Hit {
        /// Distance from ray origin in world units.
        distance: f64,
        /// Sampled point at that distance.
        point: Vec3,
    },
    /// The segment was exhausted using the ideal Lipschitz step argument.
    Miss,
    /// The query could not decide within its declared numerical model or budget.
    Indeterminate(RayIssue),
}

/// Reason for an indeterminate ray query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RayIssue {
    /// Sampling or ray arithmetic produced an invalid value.
    Numeric(MathError),
    /// The ray starts in the negative region; this tracer handles exterior starts only.
    StartsInside,
    /// No result before the iteration limit.
    IterationLimit,
    /// Floating point addition could not advance the ray.
    NoProgress,
}

/// Traces an exterior ray against an exact primitive SDF.
/// `Miss` and `Hit` rely on ideal real-arithmetic bounds; floating point error is not certified.
/// General CSG `Field` deliberately does not implement `ExactSdf` and cannot be passed here.
pub fn trace<F: ExactSdf>(field: &F, ray: Ray, options: RayOptions) -> RayOutcome {
    let bound = field.lipschitz_bound();
    if !bound.is_finite() {
        return RayOutcome::Indeterminate(RayIssue::Numeric(MathError::NonFinite));
    }
    if bound <= 0.0 {
        return RayOutcome::Indeterminate(RayIssue::Numeric(MathError::NonPositive));
    }
    let mut distance = 0.0;
    for _ in 0..options.max_iterations {
        let point = match ray.point_at(distance) {
            Ok(point) => point,
            Err(error) => return RayOutcome::Indeterminate(RayIssue::Numeric(error)),
        };
        let value = match field.sample(point) {
            Ok(value) => value,
            Err(error) => return RayOutcome::Indeterminate(RayIssue::Numeric(error)),
        };
        if value < 0.0 {
            return RayOutcome::Indeterminate(RayIssue::StartsInside);
        }
        if value <= options.hit_tolerance {
            return RayOutcome::Hit { distance, point };
        }
        if distance == ray.max_distance {
            return RayOutcome::Miss;
        }
        let step = value / bound;
        if !step.is_finite() {
            return RayOutcome::Indeterminate(RayIssue::Numeric(MathError::NonFinite));
        }
        let next = distance + step;
        if !next.is_finite() {
            return RayOutcome::Indeterminate(RayIssue::Numeric(MathError::NonFinite));
        }
        if next > ray.max_distance {
            return RayOutcome::Miss;
        }
        if next <= distance {
            return RayOutcome::Indeterminate(RayIssue::NoProgress);
        }
        distance = next;
    }
    RayOutcome::Indeterminate(RayIssue::IterationLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AxisAlignedBox, Sphere};

    fn p(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }

    #[test]
    fn bounded_ray_outcomes() {
        let options = RayOptions::new(1e-9, 64).unwrap();
        let sphere = Sphere::new(1.0).unwrap();
        assert_eq!(
            trace(
                &sphere,
                Ray::new(p(0.0, 0.0, -3.0), p(0.0, 0.0, 1.0), 5.0).unwrap(),
                options
            ),
            RayOutcome::Hit {
                distance: 2.0,
                point: p(0.0, 0.0, -1.0)
            }
        );
        assert_eq!(
            trace(
                &sphere,
                Ray::new(p(0.0, 0.0, -3.0), p(0.0, 0.0, 1.0), 2.0).unwrap(),
                options
            ),
            RayOutcome::Hit {
                distance: 2.0,
                point: p(0.0, 0.0, -1.0)
            }
        );
        assert_eq!(
            trace(
                &sphere,
                Ray::new(p(0.0, 0.0, -3.0), p(0.0, 1.0, 0.0), 1.0).unwrap(),
                options
            ),
            RayOutcome::Miss
        );
        assert_eq!(
            trace(
                &sphere,
                Ray::new(Vec3::ZERO, p(1.0, 0.0, 0.0), 5.0).unwrap(),
                options
            ),
            RayOutcome::Indeterminate(RayIssue::StartsInside)
        );
        assert_eq!(
            trace(
                &sphere,
                Ray::new(p(-2.0, 1.0, 0.0), p(1.0, 0.0, 0.0), 4.0).unwrap(),
                RayOptions::new(1e-12, 1).unwrap()
            ),
            RayOutcome::Indeterminate(RayIssue::IterationLimit)
        );
        let box_field = AxisAlignedBox::new(p(1.0, 1.0, 1.0)).unwrap();
        assert_eq!(
            trace(
                &box_field,
                Ray::new(p(3.0, 0.0, 0.0), p(-1.0, 0.0, 0.0), 5.0).unwrap(),
                options
            ),
            RayOutcome::Hit {
                distance: 2.0,
                point: p(1.0, 0.0, 0.0)
            }
        );
        assert_eq!(
            Ray::new(Vec3::ZERO, Vec3::ZERO, 1.0),
            Err(MathError::NonPositive)
        );
        assert_eq!(
            Ray::new(Vec3::ZERO, p(1.0, 0.0, 0.0), f64::NAN),
            Err(MathError::NonFinite)
        );
        assert_eq!(RayOptions::new(0.0, 1), Err(MathError::NonPositive));
        assert_eq!(RayOptions::new(f64::INFINITY, 1), Err(MathError::NonFinite));
        assert_eq!(RayOptions::new(1e-3, 0), Err(MathError::NonPositive));
        assert_eq!(
            trace(
                &sphere,
                Ray::new(p(f64::MAX, f64::MAX, 0.0), p(1.0, 0.0, 0.0), 1.0).unwrap(),
                options
            ),
            RayOutcome::Indeterminate(RayIssue::Numeric(MathError::NonFinite))
        );
    }
}
