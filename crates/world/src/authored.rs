//! Immutable authored definitions and their mutable, world-owned occurrences.

use crate::{EntityId, WorldError};
use serde::{Deserialize, Serialize};
use spatial_math::{Transform, Vec3};
use std::sync::Arc;
use world_authoring::{
    COMPILER_SEMANTICS_VERSION, DefinitionRevision, SegmentReference, Structure,
    compile_json_expected,
};

/// Maximum number of distinct source revisions retained in one world.
pub const MAX_AUTHORED_DEFINITIONS: usize = 4;
/// Maximum number of authored occurrences in one world.
pub const MAX_AUTHORED_INSTANCES: usize = 8;
/// Maximum generated capsules across all authored occurrences.
pub const MAX_AUTHORED_SEGMENTS: usize = 4_096;
const MAX_COORDINATE: f64 = 9_000.0;

/// Self-contained immutable source, shared by occurrences of the same revision.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredDefinition {
    revision: DefinitionRevision,
    compiler_semantics_version: u32,
    source: Vec<u8>,
    #[serde(skip)]
    compiled: Option<Arc<Structure>>,
}
impl PartialEq for AuthoredDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
            && self.compiler_semantics_version == other.compiler_semantics_version
            && self.source == other.source
    }
}
impl AuthoredDefinition {
    /// Exact byte revision of the definition.
    pub fn revision(&self) -> DefinitionRevision {
        self.revision
    }
    /// Original source bytes, independent of the external file after insertion.
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    /// Validated, derived structure; absent only while loading an unvalidated save.
    pub fn structure(&self) -> Option<&Structure> {
        self.compiled.as_deref()
    }
    pub(crate) fn from_source(source: &[u8]) -> Result<Self, WorldError> {
        let revision = DefinitionRevision::of(source);
        let compiled =
            compile_json_expected(source, revision).map_err(|_| WorldError::InvalidStructure)?;
        Ok(Self {
            revision,
            compiler_semantics_version: COMPILER_SEMANTICS_VERSION,
            source: source.to_vec(),
            compiled: Some(Arc::new(compiled)),
        })
    }
    pub(crate) fn rebuild(&mut self) -> Result<(), WorldError> {
        if self.compiler_semantics_version != COMPILER_SEMANTICS_VERSION {
            return Err(WorldError::InvalidStructure);
        }
        self.compiled = Some(Arc::new(
            compile_json_expected(&self.source, self.revision)
                .map_err(|_| WorldError::InvalidStructure)?,
        ));
        Ok(())
    }
}

/// A stable occurrence of one exact definition revision.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredInstance {
    id: EntityId,
    revision: DefinitionRevision,
    compiler_semantics_version: u32,
    transform: Transform,
    enabled: bool,
    solid: bool,
    generation: u64,
}
impl AuthoredInstance {
    /// Stable world identity, distinct from definition identity.
    pub fn id(&self) -> EntityId {
        self.id
    }
    /// Pinned exact definition revision.
    pub fn revision(&self) -> DefinitionRevision {
        self.revision
    }
    /// Compiler contract used to interpret the pinned revision.
    pub fn compiler_semantics_version(&self) -> u32 {
        self.compiler_semantics_version
    }
    /// Translation and positive uniform scale; rotation is unsupported.
    pub fn transform(&self) -> Transform {
        self.transform
    }
    /// Disabled instances are absent from rendering, picking and contact.
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// Whether enabled capsules participate in analytic contact.
    pub fn solid(&self) -> bool {
        self.solid
    }
    /// Increments when geometry or participation changes, invalidating old selections.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub(crate) fn set_enabled(&mut self, enabled: bool) -> Result<(), WorldError> {
        if self.enabled != enabled {
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or(WorldError::IdExhausted)?;
            self.enabled = enabled;
        }
        Ok(())
    }
    pub(crate) fn set_transform(&mut self, transform: Transform) -> Result<(), WorldError> {
        if self.transform != transform {
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or(WorldError::IdExhausted)?;
            self.transform = transform;
        }
        Ok(())
    }
    pub(crate) fn set_revision(&mut self, revision: DefinitionRevision) -> Result<(), WorldError> {
        if self.revision != revision {
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or(WorldError::IdExhausted)?;
            self.revision = revision;
        }
        Ok(())
    }
}

/// Identity of a segment within one world occurrence and exact definition revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredSegmentRef {
    /// Stable occurrence ID.
    pub instance: EntityId,
    /// Occurrence generation when this segment was selected.
    pub generation: u64,
    /// Revision and structural provenance path.
    pub segment: SegmentReference,
}

/// World-space analytic capsule derived from an authored occurrence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoredCapsule {
    /// Stable semantic identity.
    pub reference: AuthoredSegmentRef,
    /// First endpoint.
    pub start: Vec3,
    /// Second endpoint.
    pub end: Vec3,
    /// Radius in world units.
    pub radius: f64,
}

pub(crate) fn transformed(
    instance: &AuthoredInstance,
    structure: &Structure,
) -> Result<Vec<AuthoredCapsule>, WorldError> {
    let mut result = Vec::with_capacity(structure.segments().len());
    for segment in structure.segments() {
        let map = |point: Vec3| -> Result<Vec3, WorldError> {
            let position = point
                .checked_scale(instance.transform.scale())?
                .checked_add(instance.transform.translation())?;
            if [position.x(), position.y(), position.z()]
                .iter()
                .any(|x| x.abs() > MAX_COORDINATE)
            {
                return Err(WorldError::OutOfRange);
            }
            Ok(position)
        };
        let radius = segment.radius * instance.transform.scale();
        if !radius.is_finite() || !(0.0001..=MAX_COORDINATE).contains(&radius) {
            return Err(WorldError::OutOfRange);
        }
        result.push(AuthoredCapsule {
            reference: AuthoredSegmentRef {
                instance: instance.id,
                generation: instance.generation,
                segment: SegmentReference {
                    revision: instance.revision,
                    compiler_semantics_version: COMPILER_SEMANTICS_VERSION,
                    identity: segment.identity,
                },
            },
            start: map(segment.start)?,
            end: map(segment.end)?,
            radius,
        });
    }
    Ok(result)
}

pub(crate) fn new_instance(
    id: EntityId,
    revision: DefinitionRevision,
    transform: Transform,
    solid: bool,
) -> AuthoredInstance {
    AuthoredInstance {
        id,
        revision,
        compiler_semantics_version: COMPILER_SEMANTICS_VERSION,
        transform,
        enabled: true,
        solid,
        generation: 0,
    }
}
