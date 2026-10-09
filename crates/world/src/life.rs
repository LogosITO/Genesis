//! Bounded analytic growth state and a continuous, non-conserved resource field.

use crate::{EntityId, WorldError};
use serde::{Deserialize, Serialize};
use spatial_math::{MathError, Vec3};

/// Maximum organisms in one world.
pub const MAX_ORGANISMS: usize = 4;
/// Maximum nodes in one organism.
pub const MAX_NODES: usize = 48;
/// Maximum lifetime node allocations per organism; pruned IDs are not reused.
pub const MAX_NODE_IDS: u32 = 1024;
/// Connection capsule radius as a fraction of its endpoint node radius.
pub const CONNECTION_RADIUS_RATIO: f64 = 0.55;
/// Maximum resource sources in one world.
pub const MAX_SOURCES: usize = 4;
/// Authoritative ordinary sphere capacity, independent of GPU snapshot capacity.
pub const MAX_SPHERES: usize = 512;
/// Spatial support for growth and resource sources, in world units.
pub const MAX_POSITION: f64 = 1000.0;

/// Structural lifecycle derived from active topology and lifetime ID capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum OrganismLifecycle {
    /// At least one active growth site can still allocate a child.
    Active,
    /// Current topology has no room for another child; pruning may reactivate it.
    Saturated,
    /// Lifetime node IDs are exhausted, even if pruning frees active capacity.
    Exhausted,
}

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
    /// Stable local allocation ID; pruning can leave gaps.
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
    /// Local maturity signal; not an independently conserved resource balance.
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
    #[serde(default)]
    next_node_id: u32,
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
            next_node_id: 1,
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
    /// The next local ID, including allocations whose nodes were later pruned.
    pub fn next_node_id(&self) -> u32 {
        self.next_node_id
    }
    /// Finds an active node by stable local ID.
    pub fn node(&self, id: u32) -> Option<&GrowthNode> {
        self.nodes.iter().find(|node| node.id == id)
    }
    fn node_index(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|node| node.id == id)
    }
    /// Returns whether `ancestor` is the node itself or an ancestor of `descendant`.
    pub fn contains_branch(&self, ancestor: u32, mut descendant: u32) -> bool {
        while let Some(node) = self.node(descendant) {
            if node.id == ancestor {
                return true;
            }
            match node.parent {
                Some(parent) => descendant = parent,
                None => break,
            }
        }
        false
    }
    /// Removes a non-root node and all its descendants. Unspent energy in removed nodes is lost;
    /// the shared budget is retained. Surviving IDs and the lifetime allocator are unchanged.
    pub fn prune_branch(&mut self, child: u32) -> Result<usize, WorldError> {
        self.validate()?;
        if child == 0 {
            return Err(WorldError::RootPrune);
        }
        if self.node(child).is_none() {
            return Err(WorldError::UnknownNode);
        }
        let mut proposed = self.clone();
        let mut removed = vec![child];
        for node in &proposed.nodes {
            if node.parent.is_some_and(|parent| removed.contains(&parent)) {
                removed.push(node.id);
            }
        }
        proposed.nodes.retain(|node| !removed.contains(&node.id));
        proposed.validate()?;
        *self = proposed;
        Ok(removed.len())
    }
    /// Migrates the dense ID allocator used by version-1 and version-2 saves.
    pub fn migrate_legacy_node_ids(&mut self) {
        self.next_node_id = self.nodes.len() as u32;
    }
    /// Shared spendable budget in model units; finite sources deposit exact allocations.
    pub fn budget(&self) -> f64 {
        self.budget
    }
    /// Number of bifurcation nodes with two children.
    pub fn branch_count(&self) -> usize {
        let mut children = vec![0u8; self.nodes.len()];
        for node in &self.nodes[1..] {
            children[self
                .node_index(node.parent.unwrap())
                .expect("validated parent")] += 1;
        }
        children.into_iter().filter(|count| *count == 2).count()
    }
    /// Nodes still allowed to create a child.
    pub fn active_count(&self) -> usize {
        let mut children = vec![0u8; self.nodes.len()];
        for node in &self.nodes[1..] {
            children[self
                .node_index(node.parent.unwrap())
                .expect("validated parent")] += 1;
        }
        children
            .into_iter()
            .filter(|count| *count < self.parameters.max_children)
            .count()
    }
    /// Current structural lifecycle; resource starvation does not erase viable tips.
    pub fn lifecycle_state(&self) -> OrganismLifecycle {
        if self.next_node_id >= MAX_NODE_IDS {
            OrganismLifecycle::Exhausted
        } else if self.nodes.len() >= MAX_NODES || self.active_count() == 0 {
            OrganismLifecycle::Saturated
        } else {
            OrganismLifecycle::Active
        }
    }
    /// Applies one tick of samples in existing node-ID order. Each entry is uptake and a
    /// proposed local direction; saturated nodes (two children) receive zero uptake.
    /// Shared budget gains only the largest local uptake per tick; mature requests consume
    /// one threshold in ID order, so more tips cannot multiply the same environmental supply.
    pub fn grow(&mut self, samples: &[(f64, Vec3)]) -> Result<usize, WorldError> {
        self.validate()?;
        let mut proposed = self.clone();
        let added = proposed.grow_in_place(samples, None)?;
        *self = proposed;
        Ok(added)
    }
    /// Applies finite-source maturity signals and deposits the exact allocated units.
    pub fn grow_allocated(
        &mut self,
        samples: &[(f64, Vec3)],
        allocated: f64,
    ) -> Result<usize, WorldError> {
        self.validate()?;
        let mut proposed = self.clone();
        let added = proposed.grow_in_place(samples, Some(allocated))?;
        *self = proposed;
        Ok(added)
    }
    fn grow_in_place(
        &mut self,
        samples: &[(f64, Vec3)],
        allocated: Option<f64>,
    ) -> Result<usize, WorldError> {
        if samples.len() != self.nodes.len() {
            return Err(WorldError::LengthMismatch);
        }
        let mut children = vec![0u8; self.nodes.len()];
        for node in &self.nodes[1..] {
            children[self
                .node_index(node.parent.ok_or(WorldError::InvalidStructure)?)
                .ok_or(WorldError::InvalidStructure)?] += 1;
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
        let deposit = allocated.unwrap_or(gained_total);
        if !deposit.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        if deposit < 0.0 {
            return Err(MathError::NonPositive.into());
        }
        if allocated.is_some() && deposit > cap - self.budget + 1e-12 {
            return Err(WorldError::OutOfRange);
        }
        self.budget = (self.budget + deposit).min(cap);
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
            if self.next_node_id >= MAX_NODE_IDS {
                return Err(WorldError::IdExhausted);
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
                id: self.next_node_id,
                parent: Some(parent),
                position,
                energy: 0.0,
            });
            self.next_node_id += 1;
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
        if self.nodes[0].id != 0 || self.next_node_id == 0 || self.next_node_id > MAX_NODE_IDS {
            return Err(WorldError::InvalidStructure);
        }
        let mut children = vec![0u8; self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            if node.id >= self.next_node_id
                || (i > 0 && node.id <= self.nodes[i - 1].id)
                || (i == 0 && node.parent.is_some())
                || (i > 0
                    && node.parent.is_none_or(|parent| {
                        parent >= node.id
                            || self
                                .node_index(parent)
                                .is_none_or(|parent_index| parent_index >= i)
                    }))
                || !bounded_position(node.position)
                || !node.energy.is_finite()
                || !(0.0..=self.parameters.threshold).contains(&node.energy)
            {
                return Err(WorldError::InvalidStructure);
            }
            if let Some(parent) = node.parent {
                children[self
                    .node_index(parent)
                    .ok_or(WorldError::InvalidStructure)?] += 1;
            }
        }
        for node in &self.nodes[1..] {
            let delta = node.position.checked_sub(
                self.node(node.parent.unwrap())
                    .ok_or(WorldError::InvalidStructure)?
                    .position,
            )?;
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
    fn finite_stock_replenishes_and_cannot_overallocate() {
        let mut world = WorldState::new(DeterministicSeed(1));
        assert!(
            world
                .spawn_finite_source(Vec3::ZERO, 1.0, 1.0, f64::NAN, 1.0, 0.1)
                .is_err()
        );
        assert!(
            world
                .spawn_finite_source(Vec3::ZERO, 1.0, 1.0, 2.0, 1.0, 0.1)
                .is_err()
        );
        let id = world
            .spawn_finite_source(Vec3::ZERO, 1.0, 1.0, 0.7, 1.0, 0.5)
            .unwrap();
        assert_eq!(
            world.spawn_source(Vec3::ZERO, 1.0, 1.0),
            Err(WorldError::InvalidStructure)
        );
        let source = world.source_mut(id).unwrap();
        source.begin_tick().unwrap();
        assert_eq!(
            source.reservoir().unwrap().last_replenished(),
            0.30000000000000004
        );
        assert!(source.allocate(1.1).is_err());
        source.allocate(0.4).unwrap();
        assert_eq!(source.reservoir().unwrap().stored(), 0.6);
        source.set_active(false);
        source.begin_tick().unwrap();
        assert_eq!(source.reservoir().unwrap().stored(), 1.0);
        assert_eq!(source.reservoir().unwrap().last_allocated(), 0.0);
        assert_eq!(source.reservoir().unwrap().total_allocated(), 0.4);
        world.validate().unwrap();
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

    #[test]
    fn pruning_preserves_survivors_and_never_reuses_ids() {
        let mut world = WorldState::new(DeterministicSeed(3));
        let organism = world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.5, 1.0, 1.0).unwrap(),
            )
            .unwrap();
        let up = Vec3::new(0.0, 1.0, 0.0).unwrap();
        let tree = &mut world.organisms_mut()[0];
        tree.grow(&[(1.0, up)]).unwrap();
        tree.grow(&[(1.0, up), (0.0, up)]).unwrap();
        tree.grow(&[(0.0, up), (1.0, up), (0.0, up)]).unwrap();
        tree.grow(&[(0.0, up), (0.0, up), (0.0, up), (1.0, up)])
            .unwrap();
        assert_eq!(
            tree.nodes().iter().map(GrowthNode::id).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        let before = tree.clone();
        assert_eq!(tree.prune_branch(4), Ok(1));
        assert_eq!(tree.next_node_id(), 5);
        *tree = before;
        let budget = tree.budget();
        assert_eq!(tree.prune_branch(1), Ok(3));
        assert_eq!(
            tree.nodes().iter().map(GrowthNode::id).collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(tree.budget(), budget);
        assert_eq!(tree.next_node_id(), 5);
        let pruned = tree.clone();
        assert_eq!(tree.prune_branch(0), Err(WorldError::RootPrune));
        assert_eq!(tree.prune_branch(1), Err(WorldError::UnknownNode));
        assert_eq!(*tree, pruned);
        tree.grow(&[(1.0, up), (0.0, up)]).unwrap();
        assert_eq!(
            tree.nodes().iter().map(GrowthNode::id).collect::<Vec<_>>(),
            [0, 2, 5]
        );
        assert_eq!(tree.node(5).unwrap().parent(), Some(0));
        world.validate().unwrap();
        assert_eq!(world.prune_branch(organism, 2), Ok(1));
    }

    #[test]
    fn lifetime_node_id_exhaustion_is_atomic() {
        let mut world = WorldState::new(DeterministicSeed(11));
        let id = world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.5, 1.0, 1.0)
                    .unwrap()
                    .with_max_children(1)
                    .unwrap(),
            )
            .unwrap();
        let up = Vec3::new(0.0, 1.0, 0.0).unwrap();
        for expected in 1..MAX_NODE_IDS {
            world.organisms_mut()[0].grow(&[(1.0, up)]).unwrap();
            assert_eq!(world.organisms()[0].nodes()[1].id(), expected);
            world.prune_branch(id, expected).unwrap();
        }
        let before = world.clone();
        assert_eq!(
            world.organisms_mut()[0].grow(&[(1.0, up)]),
            Err(WorldError::IdExhausted)
        );
        assert_eq!(world, before);
        world.validate().unwrap();
    }
}

/// A bounded stock of model resource units behind an analytic source influence field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteReservoir {
    stored: f64,
    initial_stored: f64,
    capacity: f64,
    replenish_per_tick: f64,
    last_replenished: f64,
    total_replenished: f64,
    last_allocated: f64,
    total_allocated: f64,
}
impl FiniteReservoir {
    /// Resource units currently available after the last completed tick.
    pub fn stored(&self) -> f64 {
        self.stored
    }
    /// Units present when the source was created.
    pub fn initial_stored(&self) -> f64 {
        self.initial_stored
    }
    /// Maximum stored units.
    pub fn capacity(&self) -> f64 {
        self.capacity
    }
    /// Units offered for replenishment at the beginning of each tick.
    pub fn replenish_per_tick(&self) -> f64 {
        self.replenish_per_tick
    }
    /// Units actually generated on the last tick, after capacity clipping.
    pub fn last_replenished(&self) -> f64 {
        self.last_replenished
    }
    /// Cumulative units actually generated, excluding capacity-clipped offers.
    pub fn total_replenished(&self) -> f64 {
        self.total_replenished
    }
    /// Units withdrawn for organisms on the last tick.
    pub fn last_allocated(&self) -> f64 {
        self.last_allocated
    }
    /// Cumulative units withdrawn since source creation.
    pub fn total_allocated(&self) -> f64 {
        self.total_allocated
    }
    fn new(stored: f64, capacity: f64, replenish_per_tick: f64) -> Result<Self, WorldError> {
        if [stored, capacity, replenish_per_tick]
            .iter()
            .any(|x| !x.is_finite())
        {
            return Err(MathError::NonFinite.into());
        }
        if capacity <= 0.0
            || capacity > 1_000_000.0
            || stored < 0.0
            || stored > capacity
            || replenish_per_tick < 0.0
            || replenish_per_tick > capacity
        {
            return Err(WorldError::OutOfRange);
        }
        Ok(Self {
            stored,
            initial_stored: stored,
            capacity,
            replenish_per_tick,
            last_replenished: 0.0,
            total_replenished: 0.0,
            last_allocated: 0.0,
            total_allocated: 0.0,
        })
    }
    fn validate(&self) -> Result<(), WorldError> {
        Self::new(self.stored, self.capacity, self.replenish_per_tick)?;
        if !self.initial_stored.is_finite()
            || !(0.0..=self.capacity).contains(&self.initial_stored)
            || !self.total_replenished.is_finite()
            || self.total_replenished < self.last_replenished
            || !self.last_replenished.is_finite()
            || !self.last_allocated.is_finite()
            || !self.total_allocated.is_finite()
            || !(0.0..=self.replenish_per_tick).contains(&self.last_replenished)
            || self.last_allocated < 0.0
            || self.last_allocated > self.capacity * (1.0 + 16.0 * f64::EPSILON)
            || self.total_allocated < self.last_allocated
        {
            return Err(WorldError::InvalidStructure);
        }
        let balance = self.initial_stored + self.total_replenished - self.total_allocated;
        let tolerance = 1e-10 * (1.0 + self.total_replenished + self.total_allocated);
        if !balance.is_finite() || (balance - self.stored).abs() > tolerance {
            return Err(WorldError::InvalidStructure);
        }
        Ok(())
    }
}

/// Analytic influence field with either legacy unlimited supply or a finite reservoir.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSource {
    id: EntityId,
    position: Vec3,
    radius: f64,
    strength: f64,
    active: bool,
    #[serde(default)]
    reservoir: Option<FiniteReservoir>,
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
            reservoir: None,
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
    /// Finite stock, or `None` for the pre-ecology unlimited source model.
    pub fn reservoir(&self) -> Option<&FiniteReservoir> {
        self.reservoir.as_ref()
    }
    pub(crate) fn with_reservoir(
        mut self,
        stored: f64,
        capacity: f64,
        refill: f64,
    ) -> Result<Self, WorldError> {
        self.reservoir = Some(FiniteReservoir::new(stored, capacity, refill)?);
        Ok(self)
    }
    /// Replenishes a finite source once, even while disabled; old sources are unchanged.
    pub fn begin_tick(&mut self) -> Result<(), WorldError> {
        if let Some(stock) = &mut self.reservoir {
            let next = (stock.stored + stock.replenish_per_tick).min(stock.capacity);
            let generated = next - stock.stored;
            let total = stock.total_replenished + generated;
            if !total.is_finite() {
                return Err(WorldError::OutOfRange);
            }
            stock.last_replenished = generated;
            stock.total_replenished = total;
            stock.stored = next;
            stock.last_allocated = 0.0;
        }
        Ok(())
    }
    /// Withdraws a validated finite allocation. Zero is allowed.
    pub fn allocate(&mut self, amount: f64) -> Result<(), WorldError> {
        let stock = self
            .reservoir
            .as_mut()
            .ok_or(WorldError::InvalidStructure)?;
        if !amount.is_finite() {
            return Err(MathError::NonFinite.into());
        }
        if amount < 0.0 || amount > stock.stored {
            return Err(WorldError::OutOfRange);
        }
        let total = stock.total_allocated + amount;
        if !total.is_finite() {
            return Err(WorldError::OutOfRange);
        }
        stock.stored -= amount;
        stock.last_allocated += amount;
        stock.total_allocated = total;
        Ok(())
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
        Self::validate_values(self.position, self.radius, self.strength)?;
        if let Some(stock) = &self.reservoir {
            stock.validate()?;
        }
        Ok(())
    }
}
