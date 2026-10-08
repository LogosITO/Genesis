//! Analytic fields with explicit guarantees. CSG is a sign field, not an exact SDF.

use spatial_math::{MathError, Transform, Vec3};

/// A scalar sample with no inside/outside semantics.
pub trait ScalarField {
    /// Samples a finite point; numeric overflow is an error.
    fn sample(&self, point: Vec3) -> Result<f64, MathError>;
}

/// A sign convention: negative interior, zero set, positive exterior.
/// A zero sample alone does not prove an occupied solid or physical surface.
pub trait SignedField: ScalarField {
    /// Tests for a non-positive sample, including zero sets without solid interior.
    fn contains(&self, point: Vec3) -> Result<bool, MathError> {
        Ok(self.sample(point)? <= 0.0)
    }
}

/// Floating point evaluation of a formula that bounds zero-set distance in real arithmetic.
/// The stored `f64` is not a rounding-certified lower bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdealDistanceBound(f64);
impl IdealDistanceBound {
    /// Nonnegative bound value in the field's length units.
    pub fn value(self) -> f64 {
        self.0
    }
}

/// A signed field with a global positive Euclidean Lipschitz bound in exact arithmetic.
pub trait LipschitzField: SignedField {
    /// Finite positive upper bound on the Lipschitz constant.
    fn lipschitz_bound(&self) -> f64;

    /// Evaluates `abs(f(point)) / L`, a lower bound to the zero set in exact arithmetic.
    /// Floating point sampling and rounding do not make this a certified numeric bound.
    fn ideal_zero_set_bound(&self, point: Vec3) -> Result<IdealDistanceBound, MathError> {
        let bound = self.lipschitz_bound();
        if !bound.is_finite() {
            return Err(MathError::NonFinite);
        }
        if bound <= 0.0 {
            return Err(MathError::NonPositive);
        }
        let value = self.sample(point)?.abs() / bound;
        if value.is_finite() {
            Ok(IdealDistanceBound(value))
        } else {
            Err(MathError::NonFinite)
        }
    }
}

/// A field whose ideal sample is exact signed Euclidean distance to its primitive boundary.
pub trait ExactSdf: LipschitzField {}

/// Analytic gradient, or an explicit reason no unique derivative is returned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gradient {
    /// Unique analytic derivative.
    Defined(Vec3),
    /// No unique derivative is available at this point.
    Undefined(GradientIssue),
}

/// Reasons for an undefined analytic gradient.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientIssue {
    /// The sphere centre is singular.
    SphereCenter,
    /// An AABB edge, corner, or medial axis is non-smooth.
    BoxNonsmooth,
    /// CSG branches have equal sampled values.
    BranchTie,
}

/// A field with an analytic gradient where it exists.
pub trait AnalyticGradient: ScalarField {
    /// Returns a derivative or an explicit undefined status.
    fn gradient(&self, point: Vec3) -> Result<Gradient, MathError>;
}

impl Gradient {
    fn negated(self) -> Self {
        match self {
            Self::Defined(v) => Self::Defined(v.negated()),
            other => other,
        }
    }
}

/// Sphere centred at the local origin.
///
/// ```
/// use analytic_field::{ScalarField, Sphere};
/// use spatial_math::Vec3;
/// let sphere = Sphere::new(2.0).unwrap();
/// assert_eq!(sphere.sample(Vec3::ZERO).unwrap(), -2.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    radius: f64,
}

impl Sphere {
    /// Radius must be finite and strictly positive.
    pub fn new(radius: f64) -> Result<Self, MathError> {
        if !radius.is_finite() {
            return Err(MathError::NonFinite);
        }
        if radius <= 0.0 {
            return Err(MathError::NonPositive);
        }
        Ok(Self { radius })
    }

    /// Sphere radius in local units.
    pub fn radius(self) -> f64 {
        self.radius
    }

    /// Samples a translated, positively uniformly scaled sphere in world units.
    /// This avoids a needless division by very small scales.
    pub fn sample_transformed(self, transform: Transform, point: Vec3) -> Result<f64, MathError> {
        let world_radius = self.radius * transform.scale();
        if !world_radius.is_finite() {
            return Err(MathError::NonFinite);
        }
        let value = point.checked_sub(transform.translation())?.length()? - world_radius;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(MathError::NonFinite)
        }
    }
}

impl ScalarField for Sphere {
    fn sample(&self, point: Vec3) -> Result<f64, MathError> {
        let value = point.length()? - self.radius;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(MathError::NonFinite)
        }
    }
}
impl SignedField for Sphere {}
impl LipschitzField for Sphere {
    fn lipschitz_bound(&self) -> f64 {
        1.0
    }
}
impl ExactSdf for Sphere {}
impl AnalyticGradient for Sphere {
    fn gradient(&self, point: Vec3) -> Result<Gradient, MathError> {
        if point == Vec3::ZERO {
            return Ok(Gradient::Undefined(GradientIssue::SphereCenter));
        }
        Ok(Gradient::Defined(point.normalized()?))
    }
}

/// Axis-aligned box centred at the local origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisAlignedBox {
    half_extent: Vec3,
}

impl AxisAlignedBox {
    /// Each half extent must be strictly positive.
    pub fn new(half_extent: Vec3) -> Result<Self, MathError> {
        if [half_extent.x(), half_extent.y(), half_extent.z()]
            .iter()
            .any(|v| *v <= 0.0)
        {
            return Err(MathError::NonPositive);
        }
        Ok(Self { half_extent })
    }
}

impl ScalarField for AxisAlignedBox {
    fn sample(&self, point: Vec3) -> Result<f64, MathError> {
        let q = point.abs().checked_sub(self.half_extent)?;
        let outside = q.x().max(0.0).hypot(q.y().max(0.0)).hypot(q.z().max(0.0));
        let value = outside + q.x().max(q.y()).max(q.z()).min(0.0);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(MathError::NonFinite)
        }
    }
}
impl SignedField for AxisAlignedBox {}
impl LipschitzField for AxisAlignedBox {
    fn lipschitz_bound(&self) -> f64 {
        1.0
    }
}
impl ExactSdf for AxisAlignedBox {}
impl AnalyticGradient for AxisAlignedBox {
    fn gradient(&self, point: Vec3) -> Result<Gradient, MathError> {
        let q = point.abs().checked_sub(self.half_extent)?;
        let outside = Vec3::new(q.x().max(0.0), q.y().max(0.0), q.z().max(0.0))?;
        let length = outside.length()?;
        if length > 0.0 {
            return Ok(Gradient::Defined(Vec3::new(
                outside.x() / length * point.x().signum(),
                outside.y() / length * point.y().signum(),
                outside.z() / length * point.z().signum(),
            )?));
        }
        let max = q.x().max(q.y()).max(q.z());
        let active = [q.x() == max, q.y() == max, q.z() == max];
        if active.iter().filter(|&&v| v).count() != 1 {
            return Ok(Gradient::Undefined(GradientIssue::BoxNonsmooth));
        }
        let axis = active.iter().position(|&v| v).expect("one active axis");
        let coordinate = [point.x(), point.y(), point.z()][axis];
        if coordinate == 0.0 {
            return Ok(Gradient::Undefined(GradientIssue::BoxNonsmooth));
        }
        let sign = coordinate.signum();
        Ok(Gradient::Defined(Vec3::new(
            if axis == 0 { sign } else { 0.0 },
            if axis == 1 { sign } else { 0.0 },
            if axis == 2 { sign } else { 0.0 },
        )?))
    }
}

/// Signed CSG expression. Zero sets can exist without an occupied solid.
// shortcut: recursive evaluation can exhaust the stack; compile deep authored graphs iteratively when needed.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// Analytic sphere.
    Sphere(Sphere),
    /// Analytic axis-aligned box.
    Box(AxisAlignedBox),
    /// Union: `min(a, b)`.
    Union(Box<Field>, Box<Field>),
    /// Intersection: `max(a, b)`.
    Intersection(Box<Field>, Box<Field>),
    /// Difference: `max(a, -b)`.
    Difference(Box<Field>, Box<Field>),
    /// Applies translation and positive uniform scale to a child expression.
    Transformed(Transform, Box<Field>),
}

impl Field {
    /// Combines signed fields by union.
    pub fn union(a: Self, b: Self) -> Self {
        Self::Union(Box::new(a), Box::new(b))
    }
    /// Combines signed fields by intersection.
    pub fn intersection(a: Self, b: Self) -> Self {
        Self::Intersection(Box::new(a), Box::new(b))
    }
    /// Difference sign expression `max(a, -b)`; zero is not necessarily a solid boundary.
    pub fn difference(a: Self, b: Self) -> Self {
        Self::Difference(Box::new(a), Box::new(b))
    }
    /// Transforms a field without changing its ideal Lipschitz constant.
    pub fn transformed(child: Self, transform: Transform) -> Self {
        Self::Transformed(transform, Box::new(child))
    }
}

impl ScalarField for Field {
    fn sample(&self, point: Vec3) -> Result<f64, MathError> {
        match self {
            Self::Sphere(s) => s.sample(point),
            Self::Box(b) => b.sample(point),
            Self::Union(a, b) => Ok(a.sample(point)?.min(b.sample(point)?)),
            Self::Intersection(a, b) => Ok(a.sample(point)?.max(b.sample(point)?)),
            Self::Difference(a, b) => Ok(a.sample(point)?.max(-b.sample(point)?)),
            Self::Transformed(transform, child) => match child.as_ref() {
                Self::Sphere(sphere) => sphere.sample_transformed(*transform, point),
                _ => {
                    let value = child.sample(transform.to_local(point)?)? * transform.scale();
                    if value.is_finite() {
                        Ok(value)
                    } else {
                        Err(MathError::NonFinite)
                    }
                }
            },
        }
    }
}
impl SignedField for Field {}
impl LipschitzField for Field {
    fn lipschitz_bound(&self) -> f64 {
        1.0
    }
}
impl AnalyticGradient for Field {
    fn gradient(&self, point: Vec3) -> Result<Gradient, MathError> {
        match self {
            Self::Sphere(s) => s.gradient(point),
            Self::Box(b) => b.gradient(point),
            Self::Union(a, b) => {
                let (av, bv) = (a.sample(point)?, b.sample(point)?);
                if av < bv {
                    a.gradient(point)
                } else if bv < av {
                    b.gradient(point)
                } else {
                    Ok(Gradient::Undefined(GradientIssue::BranchTie))
                }
            }
            Self::Intersection(a, b) => {
                let (av, bv) = (a.sample(point)?, b.sample(point)?);
                if av > bv {
                    a.gradient(point)
                } else if bv > av {
                    b.gradient(point)
                } else {
                    Ok(Gradient::Undefined(GradientIssue::BranchTie))
                }
            }
            Self::Difference(a, b) => {
                let (av, bv) = (a.sample(point)?, -b.sample(point)?);
                if av > bv {
                    a.gradient(point)
                } else if bv > av {
                    Ok(b.gradient(point)?.negated())
                } else {
                    Ok(Gradient::Undefined(GradientIssue::BranchTie))
                }
            }
            Self::Transformed(transform, child) => match child.as_ref() {
                Self::Sphere(sphere) => {
                    sphere.gradient(point.checked_sub(transform.translation())?)
                }
                _ => child.gradient(transform.to_local(point)?),
            },
        }
    }
}

mod ray;
pub use ray::{Ray, RayIssue, RayOptions, RayOutcome, trace};

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }

    #[test]
    fn sphere_samples_and_invalid_parameters() {
        let s = Sphere::new(2.0).unwrap();
        assert_eq!(s.sample(p(0.0, 0.0, 0.0)), Ok(-2.0));
        assert_eq!(s.sample(p(2.0, 0.0, 0.0)), Ok(0.0));
        assert_eq!(s.sample(p(3.0, 0.0, 0.0)), Ok(1.0));
        assert!(s.contains(p(2.0, 0.0, 0.0)).unwrap());
        assert_eq!(Sphere::new(-1.0), Err(MathError::NonPositive));
        assert_eq!(Sphere::new(f64::INFINITY), Err(MathError::NonFinite));
        assert_eq!(
            s.sample(p(f64::MAX, f64::MAX, 0.0)),
            Err(MathError::NonFinite)
        );
    }

    #[test]
    fn box_samples() {
        let b = AxisAlignedBox::new(p(1.0, 2.0, 3.0)).unwrap();
        assert_eq!(b.sample(p(0.0, 0.0, 0.0)), Ok(-1.0));
        assert_eq!(b.sample(p(1.0, 0.0, 0.0)), Ok(0.0));
        assert_eq!(b.sample(p(2.0, 0.0, 0.0)), Ok(1.0));
        assert_eq!(b.sample(p(2.0, 3.0, 0.0)), Ok(2.0_f64.sqrt()));
        assert_eq!(
            AxisAlignedBox::new(p(1.0, 0.0, 1.0)),
            Err(MathError::NonPositive)
        );
    }

    #[test]
    fn csg_signs() {
        let a = Field::Sphere(Sphere::new(2.0).unwrap());
        let b = Field::Box(AxisAlignedBox::new(p(1.0, 1.0, 1.0)).unwrap());
        assert_eq!(
            Field::union(a.clone(), b.clone()).sample(p(0.0, 0.0, 0.0)),
            Ok(-2.0)
        );
        assert_eq!(
            Field::intersection(a.clone(), b.clone()).sample(p(0.0, 0.0, 0.0)),
            Ok(-1.0)
        );
        assert_eq!(Field::difference(a, b).sample(p(0.0, 0.0, 0.0)), Ok(1.0));
        let cut = Field::difference(
            Field::Sphere(Sphere::new(2.0).unwrap()),
            Field::Box(AxisAlignedBox::new(p(1.0, 1.0, 1.0)).unwrap()),
        );
        assert_eq!(cut.sample(p(1.0, 0.0, 0.0)), Ok(0.0));
        assert!(cut.contains(p(1.0, 0.0, 0.0)).unwrap());
    }

    #[test]
    fn gradients_report_singularities_and_active_branches() {
        let sphere = Sphere::new(1.0).unwrap();
        assert_eq!(
            sphere.gradient(Vec3::ZERO),
            Ok(Gradient::Undefined(GradientIssue::SphereCenter))
        );
        assert_eq!(
            sphere.gradient(p(3.0, 4.0, 0.0)),
            Ok(Gradient::Defined(p(0.6, 0.8, 0.0)))
        );
        let cube = AxisAlignedBox::new(p(1.0, 1.0, 1.0)).unwrap();
        assert_eq!(
            cube.gradient(Vec3::ZERO),
            Ok(Gradient::Undefined(GradientIssue::BoxNonsmooth))
        );
        assert_eq!(
            cube.gradient(p(1.0, 1.0, 0.0)),
            Ok(Gradient::Undefined(GradientIssue::BoxNonsmooth))
        );
        assert_eq!(
            cube.gradient(p(1.0, 0.0, 0.0)),
            Ok(Gradient::Defined(p(1.0, 0.0, 0.0)))
        );
        let a = Field::Sphere(Sphere::new(2.0).unwrap());
        let b = Field::Sphere(Sphere::new(1.0).unwrap());
        assert_eq!(
            Field::difference(a, b).gradient(p(1.0, 0.0, 0.0)),
            Ok(Gradient::Defined(p(-1.0, 0.0, 0.0)))
        );
        let same = Field::union(Field::Sphere(sphere), Field::Sphere(sphere));
        assert_eq!(
            same.gradient(p(2.0, 0.0, 0.0)),
            Ok(Gradient::Undefined(GradientIssue::BranchTie))
        );
        let transform = Transform::new(p(1.0, 0.0, 0.0), 2.0).unwrap();
        let moved = Field::transformed(Field::Sphere(sphere), transform);
        assert_eq!(moved.sample(p(3.0, 0.0, 0.0)), Ok(0.0));
        assert_eq!(
            moved.gradient(p(3.0, 0.0, 0.0)),
            Ok(Gradient::Defined(p(1.0, 0.0, 0.0)))
        );
        let tiny = Field::transformed(
            Field::Sphere(sphere),
            Transform::new(Vec3::ZERO, 1e-308).unwrap(),
        );
        assert_eq!(
            tiny.gradient(p(1.0, 0.0, 0.0)),
            Ok(Gradient::Defined(p(1.0, 0.0, 0.0)))
        );
    }

    #[test]
    fn csg_zero_sets_are_not_solid_boundaries() {
        let a = Field::Sphere(Sphere::new(1.0).unwrap());
        let same_union = Field::union(a.clone(), a.clone());
        let same_intersection = Field::intersection(a.clone(), a.clone());
        let same_difference = Field::difference(a.clone(), a.clone());
        for point in [Vec3::ZERO, p(1.0, 0.0, 0.0), p(2.0, 0.0, 0.0)] {
            assert_eq!(same_union.sample(point), a.sample(point));
            assert_eq!(same_intersection.sample(point), a.sample(point));
            assert!(same_difference.sample(point).unwrap() >= 0.0);
        }
        assert_eq!(same_difference.sample(p(1.0, 0.0, 0.0)), Ok(0.0));
        assert!(same_difference.contains(p(1.0, 0.0, 0.0)).unwrap());
        let shift = Transform::new(p(2.0, 0.0, 0.0), 1.0).unwrap();
        let touching = Field::intersection(a.clone(), Field::transformed(a.clone(), shift));
        assert_eq!(touching.sample(p(1.0, 0.0, 0.0)), Ok(0.0));
        assert!(touching.sample(p(0.9, 0.0, 0.0)).unwrap() > 0.0);
        assert!(touching.sample(p(1.1, 0.0, 0.0)).unwrap() > 0.0);
        let overlap = Field::intersection(
            a.clone(),
            Field::transformed(a.clone(), Transform::new(p(1.0, 0.0, 0.0), 1.0).unwrap()),
        );
        assert!(overlap.sample(p(0.5, 0.0, 0.0)).unwrap() < 0.0);
        let outer = Field::Sphere(Sphere::new(2.0).unwrap());
        assert!(
            Field::difference(outer.clone(), a.clone())
                .sample(p(1.5, 0.0, 0.0))
                .unwrap()
                < 0.0
        );
        assert!(Field::difference(a, outer).sample(Vec3::ZERO).unwrap() > 0.0);
    }

    #[test]
    fn ideal_bounds_are_to_zero_sets_only() {
        let sphere = Sphere::new(1.0).unwrap();
        assert_eq!(
            sphere
                .ideal_zero_set_bound(p(3.0, 0.0, 0.0))
                .unwrap()
                .value(),
            2.0
        );
        assert_eq!(
            sphere.ideal_zero_set_bound(Vec3::ZERO).unwrap().value(),
            1.0
        );
        let difference = Field::difference(Field::Sphere(sphere), Field::Sphere(sphere));
        assert_eq!(
            difference
                .ideal_zero_set_bound(p(1.0, 0.0, 0.0))
                .unwrap()
                .value(),
            0.0
        );
    }
}
