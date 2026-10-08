//! Bounded analytic growth state and a continuous, non-conserved resource field.

use crate::{EntityId, WorldError};
use serde::{Deserialize, Serialize};
use spatial_math::{MathError, Vec3};

/// Maximum organisms in one world.
pub const MAX_ORGANISMS: usize = 4;
/// Maximum nodes in one organism.
pub const MAX_NODES: usize = 48;
/// Maximum resource sources in one world.
pub const MAX_SOURCES: usize = 4;
/// Maximum ordinary sphere entities, leaving room for all life primitives on the GPU.
pub const MAX_SPHERES: usize = 32;
/// Spatial support for growth and resource sources, in world units.
pub const MAX_POSITION: f64 = 1000.0;

fn bounded_position(position: Vec3) -> bool {
    [position.x(), position.y(), position.z()]
        .into_iter()
        .all(|x| x.abs() <= MAX_POSITION)
}

/// One node of a small rooted growth graph. IDs are never reused within an organism.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthNode {
    id: u32,
    parent: Option<u32>,
    position: Vec3,
    energy: f64,
}
impl GrowthNode {
    /// Stable local ID, equal to its allocation index.
    pub fn id(&self) -> u32 {
        self.id
    }
    /// Parent ID; the root has no parent.
    pub fn parent(&self) -> Option<u32> {
        self.parent
    }
    /// Node centre in world coordinates.
    pub fn position(&self) -> Vec3 {
        self.position
    }
    /// Accumulated resource budget, in model energy units.
    pub fn energy(&self) -> f64 {
        self.energy
    }
}

/// Fixed, validated rule parameters. Growth adds at most one child per organism per tick.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthParameters {
    node_radius: f64,
    segment_length: f64,
    threshold: f64,
    uptake: f64,
}
impl GrowthParameters {
    /// Dimensions are world units; threshold is energy; uptake scales energy per second.
    pub fn new(
        node_radius: f64,
        segment_length: f64,
        threshold: f64,
        uptake: f64,
    ) -> Result<Self, WorldError> {
        let values = [node_radius, segment_length, threshold, uptake];
        if values.iter().any(|x| !x.is_finite()) {
            return Err(MathError::NonFinite.into());
        }
        if values.iter().any(|x| *x <= 0.0) {
            return Err(MathError::NonPositive.into());
        }
        if node_radius > 10.0 || segment_length > 10.0 || threshold > 1_000.0 || uptake > 1_000.0 {
            return Err(WorldError::OutOfRange);
        }
        Ok(Self {
            node_radius,
            segment_length,
            threshold,
            uptake,
        })
    }
    /// Analytic radius used to display each growth node.
    pub fn node_radius(self) -> f64 {
        self.node_radius
    }
    /// Parent-to-child distance.
    pub fn segment_length(self) -> f64 {
        self.segment_length
    }
    /// Resource budget required for one new child.
    pub fn threshold(self) -> f64 {
        self.threshold
    }
    /// Multiplier for environmental resource uptake.
    pub fn uptake(self) -> f64 {
        self.uptake
    }
}

/// A rooted chain, represented as a graph with stable parent links and node IDs.
// shortcut: one active tip permits only a chain; add bounded tip selection when branching is justified.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Organism {
    id: EntityId,
    root: Vec3,
    parameters: GrowthParameters,
    nodes: Vec<GrowthNode>,
}
impl Organism {
    pub(crate) fn new(
        id: EntityId,
        root: Vec3,
        parameters: GrowthParameters,
    ) -> Result<Self, WorldError> {
        GrowthParameters::new(
            parameters.node_radius,
            parameters.segment_length,
            parameters.threshold,
            parameters.uptake,
        )?;
        if !bounded_position(root) {
            return Err(WorldError::OutOfRange);
        }
        Ok(Self {
            id,
            root,
            parameters,
            nodes: vec![GrowthNode {
                id: 0,
                parent: None,
                position: root,
                energy: 0.0,
            }],
        })
    }
    /// Stable world identity.
    pub fn id(&self) -> EntityId {
        self.id
    }
    /// Fixed initial root position.
    pub fn root(&self) -> Vec3 {
        self.root
    }
    /// Growth rule parameters.
    pub fn parameters(&self) -> GrowthParameters {
        self.parameters
    }
    /// Nodes in allocation order.
    pub fn nodes(&self) -> &[GrowthNode] {
        &self.nodes
    }
    /// Accumulates resource on the current tip and creates at most one child.
    /// Returns true if topology changed. At capacity, energy saturates at the threshold.
    pub fn grow(&mut self, gained: f64, direction: Vec3) -> Result<bool, WorldError> {
        if !gained.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        if gained < 0.0 {
            return Err(MathError::NonPositive.into());
        }
        let direction = direction.normalized()?;
        if direction.y() <= 0.0 {
            return Err(WorldError::InvalidStructure);
        }
        let at_capacity = self.nodes.len() == MAX_NODES;
        let tip = self.nodes.last_mut().ok_or(WorldError::InvalidStructure)?;
        let total = tip.energy + gained;
        if !total.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        let energy = total.min(self.parameters.threshold);
        if energy < self.parameters.threshold || at_capacity {
            tip.energy = energy;
            return Ok(false);
        }
        let parent = tip.id;
        let position = tip
            .position
            .checked_add(direction.checked_scale(self.parameters.segment_length)?)?;
        if !bounded_position(position) {
            return Err(WorldError::OutOfRange);
        }
        let actual = position.checked_sub(tip.position)?;
        if actual.y() <= 0.0
            || (actual.length()? - self.parameters.segment_length).abs()
                > self.parameters.segment_length * 1e-12
        {
            return Err(WorldError::InvalidStructure);
        }
        tip.energy = 0.0;
        self.nodes.push(GrowthNode {
            id: self.nodes.len() as u32,
            parent: Some(parent),
            position,
            energy: 0.0,
        });
        Ok(true)
    }
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        GrowthParameters::new(
            self.parameters.node_radius,
            self.parameters.segment_length,
            self.parameters.threshold,
            self.parameters.uptake,
        )?;
        if !bounded_position(self.root) || self.nodes.is_empty() || self.nodes.len() > MAX_NODES {
            return Err(WorldError::InvalidStructure);
        }
        for (i, node) in self.nodes.iter().enumerate() {
            if node.id != i as u32
                || node.parent != (if i == 0 { None } else { Some((i - 1) as u32) })
                || !bounded_position(node.position)
                || !node.energy.is_finite()
                || !(0.0..=self.parameters.threshold).contains(&node.energy)
            {
                return Err(WorldError::InvalidStructure);
            }
        }
        for pair in self.nodes.windows(2) {
            let delta = pair[1].position.checked_sub(pair[0].position)?;
            if delta.y() <= 0.0
                || (delta.length()? - self.parameters.segment_length).abs()
                    > self.parameters.segment_length * 1e-12
            {
                return Err(WorldError::InvalidStructure);
            }
        }
        if self.nodes[0].position != self.root
            || self.nodes[..self.nodes.len() - 1]
                .iter()
                .any(|n| n.energy != 0.0)
        {
            return Err(WorldError::InvalidStructure);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeterministicSeed, WorldState};

    #[test]
    fn source_field_and_growth_limits() {
        let mut world = WorldState::new(DeterministicSeed(1));
        let root = Vec3::ZERO;
        let parameters = GrowthParameters::new(0.1, 0.5, 1.0, 1.0).unwrap();
        let id = world.spawn_organism(root, parameters).unwrap();
        let source = world
            .spawn_source(Vec3::new(0.0, 2.0, 0.0).unwrap(), 4.0, 2.0)
            .unwrap();
        assert_eq!((id.value(), source.value()), (0, 1));
        assert_eq!(world.sources()[0].sample(root).unwrap(), 1.0);
        assert_eq!(
            world.sources()[0]
                .sample(Vec3::new(4.0, 2.0, 0.0).unwrap())
                .unwrap(),
            0.0
        );
        assert_eq!(
            world.organisms_mut()[0].grow(0.5, Vec3::new(0.0, 1.0, 0.0).unwrap()),
            Ok(false)
        );
        assert_eq!(
            world.organisms_mut()[0].grow(0.5, Vec3::new(0.0, 1.0, 0.0).unwrap()),
            Ok(true)
        );
        assert_eq!(world.organisms()[0].nodes()[1].parent(), Some(0));
        assert_eq!(world.organisms()[0].nodes()[1].id(), 1);
        for _ in 2..MAX_NODES {
            assert!(
                world.organisms_mut()[0]
                    .grow(1.0, Vec3::new(0.0, 1.0, 0.0).unwrap())
                    .unwrap()
            );
        }
        assert!(
            !world.organisms_mut()[0]
                .grow(1.0, Vec3::new(0.0, 1.0, 0.0).unwrap())
                .unwrap()
        );
        assert_eq!(world.organisms()[0].nodes().len(), MAX_NODES);
        assert_eq!(world.organisms()[0].nodes().last().unwrap().energy(), 1.0);
        world.validate().unwrap();
        assert_eq!(
            GrowthParameters::new(f64::NAN, 0.5, 1.0, 1.0),
            Err(WorldError::Math(MathError::NonFinite))
        );
        assert_eq!(
            world.spawn_source(root, f64::INFINITY, 1.0),
            Err(WorldError::Math(MathError::NonFinite))
        );
    }

    #[test]
    fn organism_and_source_capacities_reject_without_allocating_ids() {
        let mut world = WorldState::new(DeterministicSeed(1));
        let parameters = GrowthParameters::new(0.1, 0.5, 1.0, 1.0).unwrap();
        for _ in 0..MAX_ORGANISMS {
            world.spawn_organism(Vec3::ZERO, parameters).unwrap();
        }
        assert_eq!(
            world.spawn_organism(Vec3::ZERO, parameters),
            Err(WorldError::Capacity)
        );
        for _ in 0..MAX_SOURCES {
            world.spawn_source(Vec3::ZERO, 1.0, 1.0).unwrap();
        }
        assert_eq!(
            world.spawn_source(Vec3::ZERO, 1.0, 1.0),
            Err(WorldError::Capacity)
        );
        assert_eq!(
            world.sources()[MAX_SOURCES - 1].id().value(),
            (MAX_ORGANISMS + MAX_SOURCES - 1) as u64
        );
        world.validate().unwrap();
    }
}

/// A continuous, stationary source until changed by a tick event. This is concentration,
/// not a finite supply: sampling does not consume it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSource {
    id: EntityId,
    position: Vec3,
    radius: f64,
    strength: f64,
    active: bool,
}
impl ResourceSource {
    pub(crate) fn new(
        id: EntityId,
        position: Vec3,
        radius: f64,
        strength: f64,
    ) -> Result<Self, WorldError> {
        Self::validate_values(position, radius, strength)?;
        Ok(Self {
            id,
            position,
            radius,
            strength,
            active: true,
        })
    }
    fn validate_values(position: Vec3, radius: f64, strength: f64) -> Result<(), WorldError> {
        if !radius.is_finite() || !strength.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        if radius <= 0.0 || strength < 0.0 {
            return Err(MathError::NonPositive.into());
        }
        if !bounded_position(position) || radius > 100.0 || strength > 100.0 {
            return Err(WorldError::OutOfRange);
        }
        Ok(())
    }
    /// Stable world identity.
    pub fn id(&self) -> EntityId {
        self.id
    }
    /// Source centre.
    pub fn position(&self) -> Vec3 {
        self.position
    }
    /// Radius of nonzero influence.
    pub fn radius(&self) -> f64 {
        self.radius
    }
    /// Peak concentration.
    pub fn strength(&self) -> f64 {
        self.strength
    }
    /// Whether this source contributes concentration.
    pub fn active(&self) -> bool {
        self.active
    }
    /// Continuous compact-support concentration: `strength * max(0, 1 - distance/radius)`.
    pub fn sample(&self, point: Vec3) -> Result<f64, WorldError> {
        if !self.active {
            return Ok(0.0);
        }
        let distance = point.checked_sub(self.position)?.length()?;
        Ok(self.strength * (1.0 - distance / self.radius).max(0.0))
    }
    /// Moves a source to a valid bounded world position.
    pub fn move_to(&mut self, position: Vec3) -> Result<(), WorldError> {
        if !bounded_position(position) {
            return Err(WorldError::OutOfRange);
        }
        self.position = position;
        Ok(())
    }
    /// Enables or disables resource influence.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        Self::validate_values(self.position, self.radius, self.strength)
    }
}
