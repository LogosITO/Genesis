//! One bounded kinematic sphere, stored with the authoritative world.

use crate::{EntityId, WorldError};
use serde::{Deserialize, Serialize};
use spatial_math::{MathError, Vec3};

/// Maximum speed in world units per simulated second.
pub const MAX_BODY_SPEED: f64 = 100.0;

/// Stable identity of an analytic solid contacted by the body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ColliderId {
    /// Ordinary world sphere.
    Sphere(EntityId),
    /// Growth-node sphere.
    Node {
        /// Stable organism ID.
        organism: EntityId,
        /// Stable local node ID.
        node: u32,
    },
    /// Parent-child capsule, identified by its child node.
    Connection {
        /// Stable organism ID.
        organism: EntityId,
        /// Stable child node ID.
        child: u32,
    },
}

/// Contact recorded after a fixed step. `fraction` is in `[0, 1]` along that step's movement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyContact {
    /// Solid that constrained the body.
    pub collider: ColliderId,
    /// First contact fraction; zero includes initial overlap.
    pub fraction: f64,
    /// Outward normal, absent when an overlap has no unique normal.
    pub normal: Option<Vec3>,
    /// True when geometry already occupied the body at the start of movement.
    pub initial_overlap: bool,
}

/// A single controllable analytic sphere. Inputs set velocity; only fixed ticks move it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KinematicBody {
    id: EntityId,
    position: Vec3,
    radius: f64,
    desired_velocity: Vec3,
    last_displacement: Vec3,
    contact: Option<BodyContact>,
}

impl KinematicBody {
    pub(crate) fn new(id: EntityId, position: Vec3, radius: f64) -> Result<Self, WorldError> {
        if !radius.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        if !(0.001..=10.0).contains(&radius) || !bounded(position) {
            return Err(WorldError::OutOfRange);
        }
        Ok(Self {
            id,
            position,
            radius,
            desired_velocity: Vec3::ZERO,
            last_displacement: Vec3::ZERO,
            contact: None,
        })
    }
    /// Stable world ID.
    pub fn id(&self) -> EntityId {
        self.id
    }
    /// Sphere centre in world coordinates.
    pub fn position(&self) -> Vec3 {
        self.position
    }
    /// Sphere radius in world units.
    pub fn radius(&self) -> f64 {
        self.radius
    }
    /// Requested velocity, held until another tick-indexed input changes it.
    pub fn desired_velocity(&self) -> Vec3 {
        self.desired_velocity
    }
    /// Actual displacement in the most recently completed tick.
    pub fn last_displacement(&self) -> Vec3 {
        self.last_displacement
    }
    /// Last contact, if any.
    pub fn contact(&self) -> Option<BodyContact> {
        self.contact
    }
    /// Validates and changes the desired velocity; movement waits for the next fixed step.
    pub fn set_desired_velocity(&mut self, velocity: Vec3) -> Result<(), WorldError> {
        if velocity.length()? > MAX_BODY_SPEED {
            return Err(WorldError::OutOfRange);
        }
        self.desired_velocity = velocity;
        Ok(())
    }
    /// Commits a collision-constrained fixed-step result.
    pub fn apply_motion(
        &mut self,
        position: Vec3,
        contact: Option<BodyContact>,
    ) -> Result<(), WorldError> {
        if !bounded(position) {
            return Err(WorldError::OutOfRange);
        }
        self.last_displacement = position.checked_sub(self.position)?;
        self.position = position;
        self.contact = contact;
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        Self::new(self.id, self.position, self.radius)?;
        if self.desired_velocity.length()? > MAX_BODY_SPEED
            || self.last_displacement.length()? > 2000.0
            || self.contact.is_some_and(|contact| {
                !contact.fraction.is_finite()
                    || !(0.0..=1.0).contains(&contact.fraction)
                    || (contact.initial_overlap && contact.fraction != 0.0)
                    || (contact.normal.is_none() && !contact.initial_overlap)
                    || contact.normal.is_some_and(|normal| {
                        normal
                            .length()
                            .map_or(true, |length| (length - 1.0).abs() > 1e-9)
                    })
            })
        {
            return Err(WorldError::InvalidStructure);
        }
        Ok(())
    }
}

fn bounded(position: Vec3) -> bool {
    [position.x(), position.y(), position.z()]
        .into_iter()
        .all(|coordinate| coordinate.abs() <= 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeterministicSeed, WorldState};

    #[test]
    fn body_capacity_and_historical_contact_ids_are_validated() {
        let mut world = WorldState::new(DeterministicSeed(4));
        assert_eq!(
            world.spawn_body(Vec3::ZERO, -1.0),
            Err(WorldError::OutOfRange)
        );
        let id = world.spawn_body(Vec3::ZERO, 0.1).unwrap();
        assert_eq!(id.value(), 0);
        assert_eq!(world.spawn_body(Vec3::ZERO, 0.1), Err(WorldError::Capacity));
        world
            .body_mut(id)
            .unwrap()
            .apply_motion(
                Vec3::ZERO,
                Some(BodyContact {
                    collider: ColliderId::Sphere(id),
                    fraction: 0.0,
                    normal: None,
                    initial_overlap: true,
                }),
            )
            .unwrap();
        assert_eq!(world.validate(), Err(WorldError::InvalidStructure));
    }
}
