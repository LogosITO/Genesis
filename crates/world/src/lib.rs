//! Minimal mutable world: stable IDs and analytic spheres, with no renderer dependency.

use analytic_field::Sphere;
use serde::{Deserialize, Serialize};
use spatial_math::{MathError, Transform, Vec3};
use std::fmt;

mod authored;
mod body;
mod life;
pub use authored::{
    AuthoredCapsule, AuthoredDefinition, AuthoredInstance, AuthoredSegmentRef,
    MAX_AUTHORED_DEFINITIONS, MAX_AUTHORED_INSTANCES, MAX_AUTHORED_SEGMENTS,
};
use authored::{new_instance, transformed};
pub use body::{BodyContact, ColliderId, KinematicBody, MAX_BODY_SPEED};
pub use life::{
    CONNECTION_RADIUS_RATIO, FiniteReservoir, GrowthNode, GrowthParameters, MAX_NODE_IDS,
    MAX_NODES, MAX_ORGANISMS, MAX_SOURCES, MAX_SPHERES, Organism, OrganismLifecycle,
    ResourceSource,
};
use world_authoring::DefinitionRevision;

/// A stable identifier for the lifetime of a world. IDs are never reused.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct EntityId(u64);
impl EntityId {
    /// Numeric identifier for logs or serialization.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Seed recorded with world state for future seeded rules. The 0.1 growth rule is deterministic without randomness.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeterministicSeed(pub u64);

/// Errors in world construction, queries, and mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    /// Invalid or overflowing floating point value.
    Math(MathError),
    /// No entity with this ID exists.
    UnknownEntity,
    /// No active growth node has this stable local ID.
    UnknownNode,
    /// The root cannot be pruned.
    RootPrune,
    /// Entity ID space is exhausted.
    IdExhausted,
    /// Radius update length did not match the number of entities.
    LengthMismatch,
    /// A fixed world, source, or organism capacity was reached.
    Capacity,
    /// A value exceeds the model's documented support.
    OutOfRange,
    /// Invalid saved identity or growth topology.
    InvalidStructure,
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldState {
    seed: DeterministicSeed,
    entities: Vec<Entity>,
    organisms: Vec<Organism>,
    sources: Vec<ResourceSource>,
    #[serde(default)]
    body: Option<KinematicBody>,
    #[serde(default)]
    authored_definitions: Vec<AuthoredDefinition>,
    #[serde(default)]
    authored_instances: Vec<AuthoredInstance>,
    next_id: u64,
}
impl WorldState {
    /// Constructs an empty world.
    pub fn new(seed: DeterministicSeed) -> Self {
        Self {
            seed,
            entities: Vec::new(),
            organisms: Vec::new(),
            sources: Vec::new(),
            body: None,
            authored_definitions: Vec::new(),
            authored_instances: Vec::new(),
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
    /// Organisms in stable allocation order.
    pub fn organisms(&self) -> &[Organism] {
        &self.organisms
    }
    /// Resource sources in stable allocation order.
    pub fn sources(&self) -> &[ResourceSource] {
        &self.sources
    }
    /// Mutable sources; changes remain subject to validated source methods.
    pub fn sources_mut(&mut self) -> &mut [ResourceSource] {
        &mut self.sources
    }
    /// The sole kinematic body, if present.
    pub fn body(&self) -> Option<&KinematicBody> {
        self.body.as_ref()
    }
    /// Unique source revisions retained by this world.
    pub fn authored_definitions(&self) -> &[AuthoredDefinition] {
        &self.authored_definitions
    }
    /// Authored occurrences in allocation order.
    pub fn authored_instances(&self) -> &[AuthoredInstance] {
        &self.authored_instances
    }
    /// Finds an authored occurrence by stable ID.
    pub fn authored_instance(&self, id: EntityId) -> Option<&AuthoredInstance> {
        self.authored_instances
            .iter()
            .find(|instance| instance.id() == id)
    }
    /// Loads exact source bytes and atomically inserts a validated authored occurrence.
    pub fn spawn_authored(
        &mut self,
        source: &[u8],
        transform: Transform,
        solid: bool,
    ) -> Result<EntityId, WorldError> {
        let revision = DefinitionRevision::of(source);
        let definition = if let Some(existing) = self
            .authored_definitions
            .iter()
            .find(|item| item.revision() == revision)
        {
            if existing.source() != source {
                return Err(WorldError::InvalidStructure);
            }
            existing.clone()
        } else {
            if self.authored_definitions.len() == MAX_AUTHORED_DEFINITIONS {
                return Err(WorldError::Capacity);
            }
            AuthoredDefinition::from_source(source)?
        };
        if self.authored_instances.len() == MAX_AUTHORED_INSTANCES {
            return Err(WorldError::Capacity);
        }
        let next = self.next_id.checked_add(1).ok_or(WorldError::IdExhausted)?;
        let structure = definition.structure().ok_or(WorldError::InvalidStructure)?;
        let total = self.authored_instances.iter().try_fold(
            structure.segments().len(),
            |sum, instance| {
                let existing = self
                    .authored_definitions
                    .iter()
                    .find(|item| item.revision() == instance.revision())
                    .and_then(AuthoredDefinition::structure)
                    .ok_or(WorldError::InvalidStructure)?;
                sum.checked_add(existing.segments().len())
                    .ok_or(WorldError::Capacity)
            },
        )?;
        if total > MAX_AUTHORED_SEGMENTS {
            return Err(WorldError::Capacity);
        }
        let id = EntityId(self.next_id);
        let instance = new_instance(id, revision, transform, solid);
        transformed(&instance, structure)?;
        if !self
            .authored_definitions
            .iter()
            .any(|item| item.revision() == revision)
        {
            self.authored_definitions.push(definition);
        }
        self.authored_instances.push(instance);
        self.next_id = next;
        Ok(id)
    }
    /// Removes an occurrence; its ID is never reused.
    pub fn remove_authored(&mut self, id: EntityId) -> Result<(), WorldError> {
        let index = self
            .authored_instances
            .iter()
            .position(|item| item.id() == id)
            .ok_or(WorldError::UnknownEntity)?;
        self.authored_instances.remove(index);
        self.clear_authored_contact(id);
        Ok(())
    }
    /// Atomically pins an occurrence to a validated source revision. Segment references from the old revision expire.
    pub fn replace_authored(&mut self, id: EntityId, source: &[u8]) -> Result<(), WorldError> {
        let index = self
            .authored_instances
            .iter()
            .position(|item| item.id() == id)
            .ok_or(WorldError::UnknownEntity)?;
        let revision = DefinitionRevision::of(source);
        let exists = self
            .authored_definitions
            .iter()
            .find(|item| item.revision() == revision);
        let definition = if let Some(existing) = exists {
            if existing.source() != source {
                return Err(WorldError::InvalidStructure);
            }
            existing.clone()
        } else {
            if self.authored_definitions.len() == MAX_AUTHORED_DEFINITIONS {
                return Err(WorldError::Capacity);
            }
            AuthoredDefinition::from_source(source)?
        };
        if self.authored_instances[index].revision() == revision {
            return Ok(());
        }
        let mut proposed = self.authored_instances[index].clone();
        proposed.set_revision(revision)?;
        let structure = definition.structure().ok_or(WorldError::InvalidStructure)?;
        transformed(&proposed, structure)?;
        let total =
            self.authored_instances
                .iter()
                .enumerate()
                .try_fold(0usize, |sum, (i, instance)| {
                    let count = if i == index {
                        structure.segments().len()
                    } else {
                        self.authored_definitions
                            .iter()
                            .find(|item| item.revision() == instance.revision())
                            .and_then(AuthoredDefinition::structure)
                            .ok_or(WorldError::InvalidStructure)?
                            .segments()
                            .len()
                    };
                    sum.checked_add(count).ok_or(WorldError::Capacity)
                })?;
        if total > MAX_AUTHORED_SEGMENTS {
            return Err(WorldError::Capacity);
        }
        if exists.is_none() {
            self.authored_definitions.push(definition);
        }
        self.authored_instances[index] = proposed;
        self.clear_authored_contact(id);
        Ok(())
    }
    fn clear_authored_contact(&mut self, id: EntityId) {
        if self.body.as_ref().and_then(KinematicBody::contact).is_some_and(|contact| matches!(contact.collider, ColliderId::Authored(reference) if reference.instance == id))
            && let Some(body) = &mut self.body {
            body.clear_contact();
        }
    }
    /// Applies a validated transform without changing identity or source revision.
    pub fn set_authored_transform(
        &mut self,
        id: EntityId,
        transform: Transform,
    ) -> Result<(), WorldError> {
        let index = self
            .authored_instances
            .iter()
            .position(|item| item.id() == id)
            .ok_or(WorldError::UnknownEntity)?;
        let mut proposed = self.authored_instances[index].clone();
        proposed.set_transform(Transform::new(transform.translation(), transform.scale())?)?;
        if proposed == self.authored_instances[index] {
            return Ok(());
        }
        let structure = self
            .authored_definitions
            .iter()
            .find(|item| item.revision() == proposed.revision())
            .and_then(AuthoredDefinition::structure)
            .ok_or(WorldError::InvalidStructure)?;
        transformed(&proposed, structure)?;
        self.authored_instances[index] = proposed;
        self.clear_authored_contact(id);
        Ok(())
    }
    /// Enables or disables rendering, picking and contact for an occurrence.
    pub fn set_authored_enabled(&mut self, id: EntityId, enabled: bool) -> Result<(), WorldError> {
        let instance = self
            .authored_instances
            .iter_mut()
            .find(|item| item.id() == id)
            .ok_or(WorldError::UnknownEntity)?;
        instance.set_enabled(enabled)?;
        if !enabled {
            self.clear_authored_contact(id);
        }
        Ok(())
    }
    /// Resolves a current segment reference; disabled, removed and replaced references are stale.
    pub fn authored_segment(&self, reference: AuthoredSegmentRef) -> Option<AuthoredCapsule> {
        let instance = self.authored_instance(reference.instance).filter(|item| {
            item.enabled()
                && item.generation() == reference.generation
                && item.revision() == reference.segment.revision
                && item.compiler_semantics_version() == reference.segment.compiler_semantics_version
        })?;
        let structure = self
            .authored_definitions
            .iter()
            .find(|item| item.revision() == instance.revision())?
            .structure()?;
        let segment = structure.segment(reference.segment)?;
        transformed(instance, structure)
            .ok()?
            .into_iter()
            .find(|item| item.reference.segment.identity == segment.identity)
    }
    /// Derives world-space capsules from cached definitions for all enabled occurrences.
    pub fn authored_capsules(&self) -> Result<Vec<(AuthoredCapsule, bool)>, WorldError> {
        let mut capsules = Vec::new();
        for instance in self.authored_instances.iter().filter(|item| item.enabled()) {
            let structure = self
                .authored_definitions
                .iter()
                .find(|item| item.revision() == instance.revision())
                .and_then(AuthoredDefinition::structure)
                .ok_or(WorldError::InvalidStructure)?;
            capsules.extend(
                transformed(instance, structure)?
                    .into_iter()
                    .map(|capsule| (capsule, instance.solid())),
            );
        }
        Ok(capsules)
    }
    /// Rebuilds disposable compiled definitions after parsing a save, before validation.
    pub fn rebuild_authored(&mut self) -> Result<(), WorldError> {
        for definition in &mut self.authored_definitions {
            definition.rebuild()?;
        }
        Ok(())
    }
    /// Creates one kinematic body with a world-stable ID.
    pub fn spawn_body(&mut self, position: Vec3, radius: f64) -> Result<EntityId, WorldError> {
        if self.body.is_some() {
            return Err(WorldError::Capacity);
        }
        let next = self.next_id.checked_add(1).ok_or(WorldError::IdExhausted)?;
        let id = EntityId(self.next_id);
        self.body = Some(KinematicBody::new(id, position, radius)?);
        self.next_id = next;
        Ok(id)
    }
    /// Finds the body for a validated movement event.
    pub fn body_mut(&mut self, id: EntityId) -> Result<&mut KinematicBody, WorldError> {
        self.body
            .as_mut()
            .filter(|body| body.id() == id)
            .ok_or(WorldError::UnknownEntity)
    }
    /// Mutable organisms; mutation is limited to validated growth methods.
    pub fn organisms_mut(&mut self) -> &mut [Organism] {
        &mut self.organisms
    }
    /// Prunes an authoritative non-root branch of one organism.
    pub fn prune_branch(&mut self, organism: EntityId, child: u32) -> Result<usize, WorldError> {
        self.organisms
            .iter_mut()
            .find(|entry| entry.id() == organism)
            .ok_or(WorldError::UnknownEntity)?
            .prune_branch(child)
    }
    /// Migrates the dense node allocators in supported legacy save formats.
    pub fn migrate_legacy_node_ids(&mut self) {
        for organism in &mut self.organisms {
            organism.migrate_legacy_node_ids();
        }
    }
    /// Finds a source for a typed environmental event.
    pub fn source_mut(&mut self, id: EntityId) -> Result<&mut ResourceSource, WorldError> {
        self.sources
            .iter_mut()
            .find(|source| source.id() == id)
            .ok_or(WorldError::UnknownEntity)
    }
    /// Adds a rooted organism with one stable root node.
    pub fn spawn_organism(
        &mut self,
        root: Vec3,
        parameters: GrowthParameters,
    ) -> Result<EntityId, WorldError> {
        if self.organisms.len() == MAX_ORGANISMS {
            return Err(WorldError::Capacity);
        }
        let next = self.next_id.checked_add(1).ok_or(WorldError::IdExhausted)?;
        let id = EntityId(self.next_id);
        let organism = Organism::new(id, root, parameters)?;
        self.organisms.push(organism);
        self.next_id = next;
        Ok(id)
    }
    /// Adds a continuous resource source.
    pub fn spawn_source(
        &mut self,
        position: Vec3,
        radius: f64,
        strength: f64,
    ) -> Result<EntityId, WorldError> {
        if self
            .sources
            .iter()
            .any(|source| source.reservoir().is_some())
        {
            return Err(WorldError::InvalidStructure);
        }
        if self.sources.len() == MAX_SOURCES {
            return Err(WorldError::Capacity);
        }
        let next = self.next_id.checked_add(1).ok_or(WorldError::IdExhausted)?;
        let id = EntityId(self.next_id);
        let source = ResourceSource::new(id, position, radius, strength)?;
        self.sources.push(source);
        self.next_id = next;
        Ok(id)
    }
    /// Adds a finite resource source; a world cannot mix finite and legacy unlimited sources.
    pub fn spawn_finite_source(
        &mut self,
        position: Vec3,
        radius: f64,
        strength: f64,
        stored: f64,
        capacity: f64,
        replenish_per_tick: f64,
    ) -> Result<EntityId, WorldError> {
        if self
            .sources
            .iter()
            .any(|source| source.reservoir().is_none())
        {
            return Err(WorldError::InvalidStructure);
        }
        if self.sources.len() == MAX_SOURCES {
            return Err(WorldError::Capacity);
        }
        let next = self.next_id.checked_add(1).ok_or(WorldError::IdExhausted)?;
        let id = EntityId(self.next_id);
        let source = ResourceSource::new(id, position, radius, strength)?.with_reservoir(
            stored,
            capacity,
            replenish_per_tick,
        )?;
        self.sources.push(source);
        self.next_id = next;
        Ok(id)
    }
    /// Adds a growing sphere. Growth must be finite and nonnegative.
    pub fn spawn_sphere(
        &mut self,
        sphere: Sphere,
        transform: Transform,
        growth_per_second: f64,
    ) -> Result<EntityId, WorldError> {
        if self.entities.len() == MAX_SPHERES {
            return Err(WorldError::Capacity);
        }
        Sphere::new(sphere.radius())?;
        Transform::new(transform.translation(), transform.scale())?;
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
        for sphere in radii {
            Sphere::new(sphere.radius())?;
        }
        for (entity, sphere) in self.entities.iter_mut().zip(radii) {
            entity.sphere = *sphere;
        }
        Ok(())
    }
    /// Validates every invariant after deserialization, before a loaded world is accepted.
    pub fn validate(&self) -> Result<(), WorldError> {
        if self.entities.len() > MAX_SPHERES
            || self.organisms.len() > MAX_ORGANISMS
            || self.sources.len() > MAX_SOURCES
            || self.authored_definitions.len() > MAX_AUTHORED_DEFINITIONS
            || self.authored_instances.len() > MAX_AUTHORED_INSTANCES
        {
            return Err(WorldError::Capacity);
        }
        let count = self.entities.len()
            + self.organisms.len()
            + self.sources.len()
            + usize::from(self.body.is_some())
            + self.authored_instances.len();
        if self.next_id < count as u64 {
            return Err(WorldError::InvalidStructure);
        }
        let mut seen = std::collections::BTreeSet::new();
        for id in self
            .entities
            .iter()
            .map(|e| e.id)
            .chain(self.organisms.iter().map(Organism::id))
            .chain(self.sources.iter().map(ResourceSource::id))
            .chain(self.body.iter().map(KinematicBody::id))
            .chain(self.authored_instances.iter().map(AuthoredInstance::id))
        {
            if id.value() >= self.next_id || !seen.insert(id.value()) {
                return Err(WorldError::InvalidStructure);
            }
        }
        let mut revisions = std::collections::HashSet::new();
        for definition in &self.authored_definitions {
            if !revisions.insert(definition.revision())
                || definition.structure().is_none()
                || DefinitionRevision::of(definition.source()) != definition.revision()
            {
                return Err(WorldError::InvalidStructure);
            }
        }
        let mut segment_count = 0usize;
        for instance in &self.authored_instances {
            if instance.compiler_semantics_version() != world_authoring::COMPILER_SEMANTICS_VERSION
            {
                return Err(WorldError::InvalidStructure);
            }
            let definition = self
                .authored_definitions
                .iter()
                .find(|item| item.revision() == instance.revision())
                .ok_or(WorldError::InvalidStructure)?;
            let structure = definition.structure().ok_or(WorldError::InvalidStructure)?;
            Transform::new(
                instance.transform().translation(),
                instance.transform().scale(),
            )?;
            transformed(instance, structure)?;
            segment_count = segment_count
                .checked_add(structure.segments().len())
                .ok_or(WorldError::Capacity)?;
        }
        if segment_count > MAX_AUTHORED_SEGMENTS {
            return Err(WorldError::Capacity);
        }
        for entity in &self.entities {
            Sphere::new(entity.sphere.radius())?;
            Transform::new(entity.transform.translation(), entity.transform.scale())?;
            if !entity.growth_per_second.is_finite() {
                return Err(MathError::NonFinite.into());
            }
            if entity.growth_per_second < 0.0 {
                return Err(MathError::NonPositive.into());
            }
        }
        for organism in &self.organisms {
            organism.validate()?;
        }
        for source in &self.sources {
            source.validate()?;
        }
        if self
            .sources
            .iter()
            .any(|source| source.reservoir().is_some())
            && self
                .sources
                .iter()
                .any(|source| source.reservoir().is_none())
        {
            return Err(WorldError::InvalidStructure);
        }
        if let Some(body) = &self.body {
            body.validate()?;
            if let Some(contact) = body.contact() {
                let valid = match contact.collider {
                    ColliderId::Sphere(id) => self.entity(id).is_some(),
                    ColliderId::Authored(reference) => {
                        self.authored_segment(reference).is_some()
                            && self
                                .authored_instance(reference.instance)
                                .is_some_and(AuthoredInstance::solid)
                    }
                    ColliderId::Node { organism, node }
                    | ColliderId::Connection {
                        organism,
                        child: node,
                    } => self
                        .organisms
                        .iter()
                        .find(|tree| tree.id() == organism)
                        .is_some_and(|tree| {
                            node < tree.next_node_id()
                                && (matches!(contact.collider, ColliderId::Node { .. })
                                    || node != 0)
                        }),
                };
                if !valid {
                    return Err(WorldError::InvalidStructure);
                }
            }
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
