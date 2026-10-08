//! Analytic fields with explicit guarantees. CSG is a sign field, not an exact SDF.

use spatial_math::{MathError, Vec3};

/// A scalar sample with no inside/outside semantics.
pub trait ScalarField {
    /// Samples a finite point; numeric overflow is an error.
    fn sample(&self, point: Vec3) -> Result<f64, MathError>;
}

/// A scalar field where negative is inside, zero is boundary, positive is outside.
pub trait SignedField: ScalarField {
    /// Classifies the point; the boundary counts as inside.
    fn contains(&self, point: Vec3) -> Result<bool, MathError> {
        Ok(self.sample(point)? <= 0.0)
    }
}

/// A field whose sample is the exact signed Euclidean distance for its primitive.
pub trait ExactSdf: SignedField {}

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
impl ExactSdf for Sphere {}

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
impl ExactSdf for AxisAlignedBox {}

/// Signed CSG expression. Its magnitude does not promise exact surface distance.
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
    /// Regularized closed-set difference. The cut surface has value zero and counts as boundary.
    pub fn difference(a: Self, b: Self) -> Self {
        Self::Difference(Box::new(a), Box::new(b))
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
        }
    }
}
impl SignedField for Field {}

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
}
