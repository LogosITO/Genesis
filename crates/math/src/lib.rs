//! Finite, right-handed 3D coordinates. Units are project-defined world units.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Failure to construct or evaluate finite spatial data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathError {
    /// A component or result was NaN or infinite.
    NonFinite,
    /// A dimension or scale was not strictly positive.
    NonPositive,
    /// Interval endpoints were reversed.
    ReversedInterval,
}

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MathError {}

/// A finite vector in world units.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    /// Constructs a vector, rejecting NaN and infinity.
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, MathError> {
        if [x, y, z].iter().all(|v| v.is_finite()) {
            Ok(Self { x, y, z })
        } else {
            Err(MathError::NonFinite)
        }
    }

    /// The coordinate origin.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// X coordinate.
    pub fn x(self) -> f64 {
        self.x
    }
    /// Y coordinate.
    pub fn y(self) -> f64 {
        self.y
    }
    /// Z coordinate.
    pub fn z(self) -> f64 {
        self.z
    }

    /// Checked vector subtraction.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, MathError> {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    /// Checked vector addition.
    pub fn checked_add(self, rhs: Self) -> Result<Self, MathError> {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    /// Checked multiplication by a scalar.
    pub fn checked_scale(self, factor: f64) -> Result<Self, MathError> {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }

    /// Additive inverse.
    pub fn negated(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Checked Euclidean length. `hypot` avoids overflow in squaring.
    pub fn length(self) -> Result<f64, MathError> {
        let length = self.x.hypot(self.y).hypot(self.z);
        if length.is_finite() {
            Ok(length)
        } else {
            Err(MathError::NonFinite)
        }
    }

    /// Unit vector; the zero vector has no direction.
    pub fn normalized(self) -> Result<Self, MathError> {
        let largest = self.x.abs().max(self.y.abs()).max(self.z.abs());
        if largest == 0.0 {
            return Err(MathError::NonPositive);
        }
        let scaled = Self::new(self.x / largest, self.y / largest, self.z / largest)?;
        let length = scaled.length()?;
        Self::new(scaled.x / length, scaled.y / length, scaled.z / length)
    }

    /// Component-wise absolute value.
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
        }
    }
}

/// A finite closed interval `[min, max]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    min: f64,
    max: f64,
}

impl Interval {
    /// Constructs an interval with ordered finite endpoints.
    pub fn new(min: f64, max: f64) -> Result<Self, MathError> {
        if !min.is_finite() || !max.is_finite() {
            return Err(MathError::NonFinite);
        }
        if min > max {
            return Err(MathError::ReversedInterval);
        }
        Ok(Self { min, max })
    }

    /// Lower endpoint.
    pub fn min(self) -> f64 {
        self.min
    }
    /// Upper endpoint.
    pub fn max(self) -> f64 {
        self.max
    }
    /// Whether a finite value lies in the closed interval.
    pub fn contains(self, value: f64) -> bool {
        value.is_finite() && self.min <= value && value <= self.max
    }
}

/// Translation and positive uniform scale. Rotation is intentionally absent in 0.1.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    translation: Vec3,
    scale: f64,
}

impl Transform {
    /// Constructs a spatial transform.
    pub fn new(translation: Vec3, scale: f64) -> Result<Self, MathError> {
        Vec3::new(translation.x, translation.y, translation.z)?;
        if !scale.is_finite() {
            return Err(MathError::NonFinite);
        }
        if scale <= 0.0 {
            return Err(MathError::NonPositive);
        }
        Ok(Self { translation, scale })
    }

    /// Identity transform.
    pub fn identity() -> Self {
        Self {
            translation: Vec3::ZERO,
            scale: 1.0,
        }
    }
    /// Translation in world units.
    pub fn translation(self) -> Vec3 {
        self.translation
    }
    /// Uniform scale.
    pub fn scale(self) -> f64 {
        self.scale
    }
    /// Maps a world point into local coordinates.
    pub fn to_local(self, point: Vec3) -> Result<Vec3, MathError> {
        let delta = point.checked_sub(self.translation)?;
        Vec3::new(
            delta.x / self.scale,
            delta.y / self.scale,
            delta.z / self.scale,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_contracts() {
        assert_eq!(Vec3::new(f64::NAN, 0.0, 0.0), Err(MathError::NonFinite));
        assert_eq!(Interval::new(2.0, 1.0), Err(MathError::ReversedInterval));
        assert_eq!(Interval::new(0.0, f64::INFINITY), Err(MathError::NonFinite));
        let interval = Interval::new(-1.0, 1.0).unwrap();
        assert!(interval.contains(-1.0) && interval.contains(1.0));
        assert!(!interval.contains(f64::NAN));
        assert_eq!(
            Transform::new(Vec3::ZERO, -1.0),
            Err(MathError::NonPositive)
        );
        let t = Transform::new(Vec3::new(2.0, 0.0, 0.0).unwrap(), 2.0).unwrap();
        assert_eq!(
            t.to_local(Vec3::new(4.0, 0.0, 0.0).unwrap()).unwrap(),
            Vec3::new(1.0, 0.0, 0.0).unwrap()
        );
        assert_eq!(Vec3::ZERO.normalized(), Err(MathError::NonPositive));
        let huge = Vec3::new(f64::MAX, f64::MAX, 0.0)
            .unwrap()
            .normalized()
            .unwrap();
        assert!((huge.length().unwrap() - 1.0).abs() < 1e-15);
        assert_eq!(
            Vec3::new(f64::from_bits(1), 0.0, 0.0).unwrap().normalized(),
            Vec3::new(1.0, 0.0, 0.0)
        );
        assert_eq!(
            Vec3::new(f64::MAX, 0.0, 0.0)
                .unwrap()
                .checked_add(Vec3::new(f64::MAX, 0.0, 0.0).unwrap()),
            Err(MathError::NonFinite)
        );
    }
}
