//! Small world-space procedural material model for the experimental mixed renderer.
use crate::RenderError;
use world_state::EntityId;

/// Spatial variation of surface base color. Coordinates are world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MaterialPattern {
    /// The base color is constant.
    Constant,
    /// Clamp `dot(world_position, axis) * frequency + phase + time * speed` to `[0,1]`.
    Gradient {
        /// Unit world-space direction.
        axis: [f32; 3],
        /// Cycles per world unit along the axis.
        frequency: f32,
        /// Additive phase.
        phase: f32,
        /// Phase change per simulation second.
        speed: f32,
    },
    /// Alternate colors every half cycle in world coordinates.
    Bands {
        /// Unit world-space direction.
        axis: [f32; 3],
        /// Cycles per world unit along the axis.
        frequency: f32,
        /// Additive phase.
        phase: f32,
        /// Phase change per simulation second.
        speed: f32,
    },
}

/// Surface parameters, independent of geometric occupancy and lighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    base: [f32; 3],
    alternate: [f32; 3],
    emission: [f32; 3],
    pattern: MaterialPattern,
}

/// CPU reference result before directional lighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialSample {
    /// Linear RGB reflectance, each component in `[0,1]`.
    pub base: [f32; 3],
    /// Added linear RGB emission, each component in `[0,4]`.
    pub emission: [f32; 3],
}
impl MaterialSample {
    /// Experimental linear-RGB ambient + one directional Lambert light, plus emission.
    pub fn lit(self, normal: [f32; 3]) -> Result<[f32; 3], RenderError> {
        let length = normal.iter().map(|x| x * x).sum::<f32>();
        if normal.iter().any(|x| !x.is_finite()) || (length - 1.0).abs() > 0.01 {
            return Err(RenderError::InvalidInput("invalid material normal"));
        }
        let light_length = (0.5f32 * 0.5 + 0.8 * 0.8 + 0.6 * 0.6).sqrt();
        let dot = (normal[0] * 0.5 + normal[1] * 0.8 - normal[2] * 0.6) / light_length;
        let diffuse = 0.18 + 0.82 * dot.max(0.0);
        Ok(std::array::from_fn(|i| {
            (self.base[i] * diffuse + self.emission[i]).clamp(0.0, 1.0)
        }))
    }
}

impl Material {
    /// Validate a bounded spatial material. Axis length must be approximately one.
    pub fn new(
        base: [f32; 3],
        alternate: [f32; 3],
        emission: [f32; 3],
        pattern: MaterialPattern,
    ) -> Result<Self, RenderError> {
        if base
            .into_iter()
            .chain(alternate)
            .any(|x| !x.is_finite() || !(0.0..=1.0).contains(&x))
            || emission
                .into_iter()
                .any(|x| !x.is_finite() || !(0.0..=4.0).contains(&x))
        {
            return Err(RenderError::InvalidInput("invalid material color"));
        }
        if let MaterialPattern::Gradient {
            axis,
            frequency,
            phase,
            speed,
        }
        | MaterialPattern::Bands {
            axis,
            frequency,
            phase,
            speed,
        } = pattern
        {
            let length_squared = axis.iter().map(|x| x * x).sum::<f32>();
            if axis.iter().any(|x| !x.is_finite())
                || !length_squared.is_finite()
                || (length_squared - 1.0).abs() > 0.001
                || !frequency.is_finite()
                || !(0.001..=10.0).contains(&frequency)
                || !phase.is_finite()
                || phase.abs() > 1000.0
                || !speed.is_finite()
                || speed.abs() > 10.0
            {
                return Err(RenderError::InvalidInput("invalid material pattern"));
            }
        }
        Ok(Self {
            base,
            alternate,
            emission,
            pattern,
        })
    }

    /// Evaluate base color and emission in world coordinates at deterministic simulation time.
    pub fn sample(
        self,
        world_position: [f32; 3],
        simulation_seconds: f32,
    ) -> Result<MaterialSample, RenderError> {
        if world_position
            .iter()
            .any(|x| !x.is_finite() || x.abs() > 10_000.0)
            || !simulation_seconds.is_finite()
            || !(0.0..=1_000_000.0).contains(&simulation_seconds)
        {
            return Err(RenderError::InvalidInput(
                "invalid material coordinates or time",
            ));
        }
        let base = match self.pattern {
            MaterialPattern::Constant => self.base,
            MaterialPattern::Gradient {
                axis,
                frequency,
                phase,
                speed,
            } => {
                let x = world_position[0] * axis[0]
                    + world_position[1] * axis[1]
                    + world_position[2] * axis[2];
                let t = (x * frequency + phase + simulation_seconds * speed).clamp(0.0, 1.0);
                std::array::from_fn(|i| self.base[i] * (1.0 - t) + self.alternate[i] * t)
            }
            MaterialPattern::Bands {
                axis,
                frequency,
                phase,
                speed,
            } => {
                let x = world_position[0] * axis[0]
                    + world_position[1] * axis[1]
                    + world_position[2] * axis[2];
                let cycle = (x * frequency + phase + simulation_seconds * speed).rem_euclid(1.0);
                if cycle < 0.5 {
                    self.base
                } else {
                    self.alternate
                }
            }
        };
        Ok(MaterialSample {
            base,
            emission: self.emission,
        })
    }

    pub(crate) fn gpu(self) -> [[f32; 4]; 5] {
        let (axis, frequency, phase, speed, mode) = match self.pattern {
            MaterialPattern::Constant => ([0.0; 3], 0.0, 0.0, 0.0, 0.0),
            MaterialPattern::Gradient {
                axis,
                frequency,
                phase,
                speed,
            } => (axis, frequency, phase, speed, 1.0),
            MaterialPattern::Bands {
                axis,
                frequency,
                phase,
                speed,
            } => (axis, frequency, phase, speed, 2.0),
        };
        [
            [self.base[0], self.base[1], self.base[2], 0.0],
            [self.alternate[0], self.alternate[1], self.alternate[2], 0.0],
            [self.emission[0], self.emission[1], self.emission[2], 0.0],
            [axis[0], axis[1], axis[2], speed],
            [frequency, phase, mode, 0.0],
        ]
    }
}

/// Presentation assignments keyed by stable world entity identity.
#[derive(Clone, Debug, Default)]
pub struct MaterialSet {
    spheres: Vec<(EntityId, Material)>,
    fields: Vec<(EntityId, Material)>,
}
impl MaterialSet {
    /// Assign a material to an ordinary world sphere; replaces an existing assignment.
    pub fn set_sphere(&mut self, id: EntityId, material: Material) {
        if let Some((_, current)) = self.spheres.iter_mut().find(|(key, _)| *key == id) {
            *current = material;
        } else {
            self.spheres.push((id, material));
        }
    }
    /// Assign a material to a world-owned Field Graph occurrence.
    pub fn set_field(&mut self, id: EntityId, material: Material) {
        if let Some((_, current)) = self.fields.iter_mut().find(|(key, _)| *key == id) {
            *current = material;
        } else {
            self.fields.push((id, material));
        }
    }
    pub(crate) fn sphere(&self, id: EntityId) -> Option<Material> {
        self.spheres.iter().find(|(key, _)| *key == id).map(|x| x.1)
    }
    pub(crate) fn field(&self, id: EntityId) -> Option<Material> {
        self.fields.iter().find(|(key, _)| *key == id).map(|x| x.1)
    }
    pub(crate) fn validate_world(
        &self,
        world: &world_state::WorldState,
    ) -> Result<(), RenderError> {
        if self.spheres.len() > 64 || self.fields.len() > world_state::MAX_FIELD_INSTANCES {
            return Err(RenderError::InvalidInput("too many material assignments"));
        }
        if self
            .spheres
            .iter()
            .any(|(id, _)| !world.entities().iter().any(|e| e.id() == *id))
            || self
                .fields
                .iter()
                .any(|(id, _)| world.field_instance(*id).is_none())
        {
            return Err(RenderError::InvalidInput(
                "material target missing from world",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constant_gradient_bands_and_validation() {
        let constant = Material::new(
            [0.2, 0.3, 0.4],
            [1.0; 3],
            [0.1, 0.0, 0.0],
            MaterialPattern::Constant,
        )
        .unwrap();
        assert_eq!(
            constant.sample([2.0, 0.0, 0.0], 0.0).unwrap().base,
            [0.2, 0.3, 0.4]
        );
        let gradient = Material::new(
            [0.0; 3],
            [1.0; 3],
            [0.0; 3],
            MaterialPattern::Gradient {
                axis: [1.0, 0.0, 0.0],
                frequency: 1.0,
                phase: 0.0,
                speed: 0.25,
            },
        )
        .unwrap();
        assert_eq!(
            gradient.sample([0.25, 0.0, 0.0], 1.0).unwrap().base,
            [0.5; 3]
        );
        let bands = Material::new(
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0; 3],
            MaterialPattern::Bands {
                axis: [1.0, 0.0, 0.0],
                frequency: 1.0,
                phase: 0.0,
                speed: 0.0,
            },
        )
        .unwrap();
        assert_eq!(
            bands.sample([0.1, 0.0, 0.0], 0.0).unwrap().base,
            [1.0, 0.0, 0.0]
        );
        assert_eq!(
            bands.sample([0.6, 0.0, 0.0], 0.0).unwrap().base,
            [0.0, 1.0, 0.0]
        );
        assert!(
            Material::new([f32::NAN; 3], [0.0; 3], [0.0; 3], MaterialPattern::Constant).is_err()
        );
        assert!(
            Material::new(
                [0.0; 3],
                [0.0; 3],
                [0.0; 3],
                MaterialPattern::Bands {
                    axis: [0.0; 3],
                    frequency: 1.0,
                    phase: 0.0,
                    speed: 0.0
                }
            )
            .is_err()
        );
        assert!(constant.sample([0.0; 3], f32::INFINITY).is_err());
    }
}
