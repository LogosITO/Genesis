//! Minimal mutable world: stable IDs and analytic spheres, with no renderer dependency.

use analytic_field::Sphere;
use spatial_math::{MathError, Transform, Vec3};
use std::fmt;

/// A stable identifier for the lifetime of a world. IDs are never reused.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityId(u64);
impl EntityId {
    /// Numeric identifier for logs or serialization.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Seed recorded with world state for future seeded rules. The 0.1 growth rule is deterministic without randomness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicSeed(pub u64);

/// Errors in world construction, queries, and mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    /// Invalid or overflowing floating point value.
    Math(MathError),
    /// No entity with this ID exists.
    UnknownEntity,
    /// Entity ID space is exhausted.
    IdExhausted,
    /// Radius update length did not match the number of entities.
    LengthMismatch,
}
impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorldError {}
impl From<MathError> for WorldError {
    fn from(value: MathError) -> Self {
        Self::Math(value)
    }
}

/// One mathematically defined world object.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    id: EntityId,
    transform: Transform,
    sphere: Sphere,
    growth_per_second: f64,
}
impl Entity {
    /// Stable identity.
    pub fn id(&self) -> EntityId {
        self.id
    }
    /// World transform.
    pub fn transform(&self) -> Transform {
        self.transform
    }
    /// Current analytic sphere.
    pub fn sphere(&self) -> Sphere {
        self.sphere
    }
    /// Growth rate in local radius units per simulated second.
    pub fn growth_per_second(&self) -> f64 {
        self.growth_per_second
    }
    /// Ideal exact world-space sphere SDF; floating point sampling remains approximate.
    pub fn signed_distance(&self, point: Vec3) -> Result<f64, WorldError> {
        Ok(self.sphere.sample_transformed(self.transform, point)?)
    }
}

/// Ordered entity storage with no removal in version 0.1.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldState {
    seed: DeterministicSeed,
    entities: Vec<Entity>,
    next_id: u64,
}
impl WorldState {
    /// Constructs an empty world.
    pub fn new(seed: DeterministicSeed) -> Self {
        Self {
            seed,
            entities: Vec::new(),
            next_id: 0,
        }
    }
    /// Recorded deterministic seed.
    pub fn seed(&self) -> DeterministicSeed {
        self.seed
    }
    /// Entities in stable insertion order.
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
    /// Finds an entity by stable ID.
    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }
    /// Adds a growing sphere. Growth must be finite and nonnegative.
    pub fn spawn_sphere(
        &mut self,
        sphere: Sphere,
        transform: Transform,
        growth_per_second: f64,
    ) -> Result<EntityId, WorldError> {
        if !growth_per_second.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        if growth_per_second < 0.0 {
            return Err(MathError::NonPositive.into());
        }
        let next = self.next_id.checked_add(1).ok_or(WorldError::IdExhausted)?;
        let id = EntityId(self.next_id);
        self.entities.push(Entity {
            id,
            sphere,
            transform,
            growth_per_second,
        });
        self.next_id = next;
        Ok(id)
    }
    /// Replaces radii after a complete validated simulation step.
    pub fn apply_radii(&mut self, radii: &[Sphere]) -> Result<(), WorldError> {
        if radii.len() != self.entities.len() {
            return Err(WorldError::LengthMismatch);
        }
        for (entity, sphere) in self.entities.iter_mut().zip(radii) {
            entity.sphere = *sphere;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_and_transformed_query() {
        let mut world = WorldState::new(DeterministicSeed(7));
        let t = Transform::new(Vec3::new(3.0, 0.0, 0.0).unwrap(), 2.0).unwrap();
        let a = world
            .spawn_sphere(Sphere::new(1.0).unwrap(), t, 0.0)
            .unwrap();
        let b = world
            .spawn_sphere(Sphere::new(1.0).unwrap(), Transform::identity(), 0.0)
            .unwrap();
        assert_eq!((a.value(), b.value()), (0, 1));
        assert_eq!(
            world
                .entity(a)
                .unwrap()
                .signed_distance(Vec3::new(5.0, 0.0, 0.0).unwrap()),
            Ok(0.0)
        );
        assert_eq!(
            world.spawn_sphere(Sphere::new(1.0).unwrap(), t, f64::NAN),
            Err(WorldError::Math(MathError::NonFinite))
        );
        assert_eq!(world.entities().len(), 2);
        let tiny = Transform::new(Vec3::ZERO, 1e-308).unwrap();
        let id = world
            .spawn_sphere(Sphere::new(1.0).unwrap(), tiny, 0.0)
            .unwrap();
        assert_eq!(
            world
                .entity(id)
                .unwrap()
                .signed_distance(Vec3::new(1.0, 0.0, 0.0).unwrap()),
            Ok(1.0)
        );
    }
}
