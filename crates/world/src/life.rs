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
/// Authoritative ordinary sphere capacity, independent of GPU snapshot capacity.
pub const MAX_SPHERES: usize = 512;
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

/// Fixed, validated rule parameters.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthParameters {
    node_radius: f64,
    segment_length: f64,
    threshold: f64,
    uptake: f64,
    #[serde(default = "default_max_children")]
    max_children: u8,
}
fn default_max_children() -> u8 {
    2
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
            max_children: 2,
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
    /// Sets the maximum children per structural node (one or two).
    pub fn with_max_children(mut self, count: u8) -> Result<Self, WorldError> {
        if !(1..=2).contains(&count) {
            return Err(WorldError::OutOfRange);
        }
        self.max_children = count;
        Ok(self)
    }
    /// Maximum children per structural node.
    pub fn max_children(self) -> u8 {
        self.max_children
    }
}

/// Bounded rooted tree with stable local IDs and a shared resource budget.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Organism {
    id: EntityId,
    root: Vec3,
    parameters: GrowthParameters,
    nodes: Vec<GrowthNode>,
    #[serde(default)]
    budget: f64,
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
        )?
        .with_max_children(parameters.max_children)?;
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
            budget: 0.0,
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
    /// Shared budget in model energy units.
    pub fn budget(&self) -> f64 {
        self.budget
    }
    /// Number of bifurcation nodes with two children.
    pub fn branch_count(&self) -> usize {
        let mut children = vec![0u8; self.nodes.len()];
        for node in &self.nodes[1..] {
            children[node.parent.unwrap() as usize] += 1;
        }
        children.into_iter().filter(|count| *count == 2).count()
    }
    /// Nodes still allowed to create a child.
    pub fn active_count(&self) -> usize {
        let mut children = vec![0u8; self.nodes.len()];
        for node in &self.nodes[1..] {
            children[node.parent.unwrap() as usize] += 1;
        }
        children
            .into_iter()
            .filter(|count| *count < self.parameters.max_children)
            .count()
    }
    /// Applies one tick of samples in existing node-ID order. Each entry is uptake and a
    /// proposed local direction; saturated nodes (two children) receive zero uptake.
    /// Shared budget gains only the largest local uptake per tick; mature requests consume
    /// one threshold in ID order, so more tips cannot multiply the same environmental supply.
    pub fn grow(&mut self, samples: &[(f64, Vec3)]) -> Result<usize, WorldError> {
        self.validate()?;
        let mut proposed = self.clone();
        let added = proposed.grow_in_place(samples)?;
        *self = proposed;
        Ok(added)
    }
    fn grow_in_place(&mut self, samples: &[(f64, Vec3)]) -> Result<usize, WorldError> {
        if samples.len() != self.nodes.len() {
            return Err(WorldError::LengthMismatch);
        }
        let mut children = vec![0u8; self.nodes.len()];
        for node in &self.nodes[1..] {
            children[node.parent.ok_or(WorldError::InvalidStructure)? as usize] += 1;
        }
        let mut directions = Vec::with_capacity(samples.len());
        let mut gained_total: f64 = 0.0;
        for (index, &(gained, direction)) in samples.iter().enumerate() {
            if !gained.is_finite() {
                return Err(MathError::NonFinite.into());
            }
            if gained < 0.0 {
                return Err(MathError::NonPositive.into());
            }
            if children[index] >= self.parameters.max_children && gained != 0.0 {
                return Err(WorldError::InvalidStructure);
            }
            let direction = direction.normalized()?;
            if direction.y() <= 0.0 {
                return Err(WorldError::InvalidStructure);
            }
            directions.push(direction);
            gained_total = gained_total.max(gained);
        }
        let cap = self.parameters.threshold * MAX_NODES as f64;
        if !gained_total.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        self.budget = (self.budget + gained_total).min(cap);
        let old_len = self.nodes.len();
        for (index, &(gained, _)) in samples.iter().enumerate() {
            if children[index] >= self.parameters.max_children {
                continue;
            }
            let at_capacity = self.nodes.len() == MAX_NODES;
            let node = &mut self.nodes[index];
            node.energy = (node.energy + gained).min(self.parameters.threshold);
            if node.energy < self.parameters.threshold
                || self.budget < self.parameters.threshold
                || at_capacity
            {
                continue;
            }
            let position = node
                .position
                .checked_add(directions[index].checked_scale(self.parameters.segment_length)?)?;
            if !bounded_position(position) {
                return Err(WorldError::OutOfRange);
            }
            let parent = node.id;
            node.energy = 0.0;
            self.budget -= self.parameters.threshold;
            self.nodes.push(GrowthNode {
                id: self.nodes.len() as u32,
                parent: Some(parent),
                position,
                energy: 0.0,
            });
            children[index] += 1;
        }
        Ok(self.nodes.len() - old_len)
    }
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        GrowthParameters::new(
            self.parameters.node_radius,
            self.parameters.segment_length,
            self.parameters.threshold,
            self.parameters.uptake,
        )?
        .with_max_children(self.parameters.max_children)?;
        if !bounded_position(self.root) || self.nodes.is_empty() || self.nodes.len() > MAX_NODES {
            return Err(WorldError::InvalidStructure);
        }
        let mut children = vec![0u8; self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            if node.id != i as u32
                || (i == 0 && node.parent.is_some())
                || (i > 0 && node.parent.is_none_or(|parent| parent as usize >= i))
                || !bounded_position(node.position)
                || !node.energy.is_finite()
                || !(0.0..=self.parameters.threshold).contains(&node.energy)
            {
                return Err(WorldError::InvalidStructure);
            }
            if let Some(parent) = node.parent {
                children[parent as usize] += 1;
            }
        }
        for node in &self.nodes[1..] {
            let delta = node
                .position
                .checked_sub(self.nodes[node.parent.unwrap() as usize].position)?;
            if delta.y() <= 0.0
                || (delta.length()? - self.parameters.segment_length).abs()
                    > self.parameters.segment_length * 1e-12
            {
                return Err(WorldError::InvalidStructure);
            }
        }
        if self.nodes[0].position != self.root
            || children.iter().enumerate().any(|(i, &count)| {
                count > self.parameters.max_children
                    || (count == self.parameters.max_children && self.nodes[i].energy != 0.0)
            })
            || !self.budget.is_finite()
            || !(0.0..=self.parameters.threshold * MAX_NODES as f64).contains(&self.budget)
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
        let up = Vec3::new(0.0, 1.0, 0.0).unwrap();
        assert_eq!(world.organisms_mut()[0].grow(&[(0.5, up)]), Ok(0));
        assert_eq!(world.organisms_mut()[0].grow(&[(0.5, up)]), Ok(1));
        assert_eq!(world.organisms()[0].nodes()[1].parent(), Some(0));
        assert_eq!(world.organisms()[0].nodes()[1].id(), 1);
        assert_eq!(
            world.organisms_mut()[0].grow(&[(1.0, up), (1.0, up)]),
            Ok(1)
        );
        assert_eq!(world.organisms()[0].nodes()[2].parent(), Some(0));
        assert_eq!(world.organisms()[0].nodes()[1].energy(), 1.0);
        assert_eq!(
            world.organisms_mut()[0].grow(&[(0.0, up), (1.0, up), (0.0, up)]),
            Ok(1)
        );
        assert_eq!(world.organisms()[0].nodes()[3].parent(), Some(1));
        assert_eq!(world.organisms()[0].budget(), 0.0);
        assert_eq!(world.organisms()[0].branch_count(), 1);
        assert_eq!(world.organisms()[0].active_count(), 3);
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

    #[test]
    fn capacity_and_invalid_growth_are_atomic() {
        let up = Vec3::new(0.0, 1.0, 0.0).unwrap();
        let mut world = WorldState::new(DeterministicSeed(2));
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.5, 1.0, 1.0)
                    .unwrap()
                    .with_max_children(1)
                    .unwrap(),
            )
            .unwrap();
        while world.organisms()[0].nodes().len() < MAX_NODES {
            let len = world.organisms()[0].nodes().len();
            let samples = (0..len)
                .map(|i| (if i + 1 == len { 1.0 } else { 0.0 }, up))
                .collect::<Vec<_>>();
            assert_eq!(world.organisms_mut()[0].grow(&samples), Ok(1));
        }
        let len = world.organisms()[0].nodes().len();
        let samples = (0..len)
            .map(|i| (if i + 1 == len { 1.0 } else { 0.0 }, up))
            .collect::<Vec<_>>();
        assert_eq!(world.organisms_mut()[0].grow(&samples), Ok(0));
        assert_eq!(world.organisms()[0].nodes().last().unwrap().energy(), 1.0);
        world.validate().unwrap();

        let mut edge = WorldState::new(DeterministicSeed(3));
        edge.spawn_organism(
            Vec3::new(0.0, 999.9, 0.0).unwrap(),
            GrowthParameters::new(0.1, 1.0, 1.0, 1.0).unwrap(),
        )
        .unwrap();
        let before = edge.clone();
        assert_eq!(
            edge.organisms_mut()[0].grow(&[(1.0, up)]),
            Err(WorldError::OutOfRange)
        );
        assert_eq!(edge, before);
        assert_eq!(
            GrowthParameters::new(0.1, 1.0, 1.0, 1.0)
                .unwrap()
                .with_max_children(3),
            Err(WorldError::OutOfRange)
        );
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
