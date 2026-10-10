//! World-owned visual field occurrences; compiled graphs are derived from exact source bytes.

use crate::{EntityId, WorldError};
use serde::{Deserialize, Serialize};
use spatial_math::{Transform, Vec3};
use std::sync::Arc;
use world_authoring::{
    DefinitionRevision,
    field_graph::{Bounds, Graph, compile_json},
};

/// Maximum distinct visual graph revisions retained by one world.
pub const MAX_FIELD_DEFINITIONS: usize = 4;
/// Maximum visual graph occurrences in one world.
pub const MAX_FIELD_INSTANCES: usize = 8;
/// Interpretation version of the persisted graph source.
pub const FIELD_GRAPH_SEMANTICS_VERSION: u32 = 1;

/// Resource-coupled uniform growth of one visual field occurrence.
/// The graph source stays immutable; its world-owned transform scale is the shape parameter.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldGrowth {
    uptake: f64,
    scale_per_unit: f64,
    initial_scale: f64,
    max_scale: f64,
    total_allocated: f64,
    last_allocated: f64,
}
impl FieldGrowth {
    /// Validates a bounded conversion from resource units to uniform field scale.
    pub fn new(
        uptake: f64,
        scale_per_unit: f64,
        initial_scale: f64,
        max_scale: f64,
    ) -> Result<Self, WorldError> {
        if [uptake, scale_per_unit, initial_scale, max_scale]
            .iter()
            .any(|value| !value.is_finite())
            || !(0.0..=100.0).contains(&uptake)
            || !(0.001..=10.0).contains(&scale_per_unit)
            || !(0.01..=100.0).contains(&initial_scale)
            || !(initial_scale..=100.0).contains(&max_scale)
        {
            return Err(WorldError::OutOfRange);
        }
        Ok(Self {
            uptake,
            scale_per_unit,
            initial_scale,
            max_scale,
            total_allocated: 0.0,
            last_allocated: 0.0,
        })
    }
    /// Resource demand multiplier for local source influence per second.
    pub fn uptake(&self) -> f64 {
        self.uptake
    }
    /// Uniform scale gained per acquired resource unit.
    pub fn scale_per_unit(&self) -> f64 {
        self.scale_per_unit
    }
    /// Saturation limit for the world-owned scale.
    pub fn max_scale(&self) -> f64 {
        self.max_scale
    }
    /// Units acquired over all completed ticks.
    pub fn total_allocated(&self) -> f64 {
        self.total_allocated
    }
    /// Units acquired during the most recently completed tick.
    pub fn last_allocated(&self) -> f64 {
        self.last_allocated
    }
    pub(crate) fn validate(&self, scale: f64) -> Result<(), WorldError> {
        Self::new(
            self.uptake,
            self.scale_per_unit,
            self.initial_scale,
            self.max_scale,
        )?;
        let expected =
            (self.initial_scale + self.total_allocated * self.scale_per_unit).min(self.max_scale);
        if !self.total_allocated.is_finite()
            || self.total_allocated < 0.0
            || !self.last_allocated.is_finite()
            || !(0.0..=self.total_allocated).contains(&self.last_allocated)
            || !expected.is_finite()
            || (expected - scale).abs() > 1e-10 * self.max_scale
        {
            return Err(WorldError::InvalidStructure);
        }
        Ok(())
    }
}

/// Exact authored bytes and a disposable compiled graph shared by occurrences.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    revision: DefinitionRevision,
    semantics_version: u32,
    source: Vec<u8>,
    #[serde(skip)]
    graph: Option<Arc<Graph>>,
}
impl PartialEq for FieldDefinition {
    fn eq(&self, rhs: &Self) -> bool {
        self.revision == rhs.revision
            && self.semantics_version == rhs.semantics_version
            && self.source == rhs.source
    }
}
impl FieldDefinition {
    /// SHA-256 of the exact source bytes.
    pub fn revision(&self) -> DefinitionRevision {
        self.revision
    }
    /// Exact bytes retained for save/load and provenance.
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    /// Validated graph, present after successful creation or save loading.
    pub fn graph(&self) -> Option<&Graph> {
        self.graph.as_deref()
    }
    pub(crate) fn from_source(source: &[u8]) -> Result<Self, WorldError> {
        let graph = compile_json(source).map_err(|_| WorldError::InvalidStructure)?;
        Ok(Self {
            revision: graph.revision(),
            semantics_version: FIELD_GRAPH_SEMANTICS_VERSION,
            source: source.to_vec(),
            graph: Some(Arc::new(graph)),
        })
    }
    pub(crate) fn rebuild(&mut self) -> Result<(), WorldError> {
        if self.semantics_version != FIELD_GRAPH_SEMANTICS_VERSION
            || self.revision != DefinitionRevision::of(&self.source)
        {
            return Err(WorldError::InvalidStructure);
        }
        let graph = compile_json(&self.source).map_err(|_| WorldError::InvalidStructure)?;
        if graph.revision() != self.revision {
            return Err(WorldError::InvalidStructure);
        }
        self.graph = Some(Arc::new(graph));
        Ok(())
    }
}

/// Stable visual occurrence. It has no gameplay collision capability.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldInstance {
    id: EntityId,
    revision: DefinitionRevision,
    transform: Transform,
    enabled: bool,
    generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    growth: Option<FieldGrowth>,
}
impl FieldInstance {
    /// World-stable ID, never reused after removal.
    pub fn id(&self) -> EntityId {
        self.id
    }
    /// Exact source revision shared by other occurrences.
    pub fn revision(&self) -> DefinitionRevision {
        self.revision
    }
    /// Translation and positive uniform scale.
    pub fn transform(&self) -> Transform {
        self.transform
    }
    /// Whether this occurrence is visible to visual snapshot consumers.
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// Changes on revision, transform or visibility edits.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Optional resource-coupled growth; ordinary visual fields have none.
    pub fn growth(&self) -> Option<&FieldGrowth> {
        self.growth.as_ref()
    }
    pub(crate) fn new(id: EntityId, revision: DefinitionRevision, transform: Transform) -> Self {
        Self {
            id,
            revision,
            transform,
            enabled: true,
            generation: 0,
            growth: None,
        }
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
    pub(crate) fn enable_growth(&mut self, growth: FieldGrowth) -> Result<(), WorldError> {
        if self.growth.is_some() || growth.initial_scale != self.transform.scale() {
            return Err(WorldError::InvalidStructure);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(WorldError::IdExhausted)?;
        self.growth = Some(growth);
        Ok(())
    }
    pub(crate) fn grow_allocated(&mut self, amount: f64) -> Result<(), WorldError> {
        let growth = self.growth.as_ref().ok_or(WorldError::InvalidStructure)?;
        let remaining = (growth.max_scale - self.transform.scale()) / growth.scale_per_unit;
        if !amount.is_finite() || amount < 0.0 || amount > remaining + 1e-12 {
            return Err(WorldError::OutOfRange);
        }
        let total = growth.total_allocated + amount;
        let scale = (growth.initial_scale + total * growth.scale_per_unit).min(growth.max_scale);
        if !total.is_finite() || !scale.is_finite() {
            return Err(WorldError::OutOfRange);
        }
        let transform = Transform::new(self.transform.translation(), scale)?;
        self.set_transform(transform)?;
        let growth = self.growth.as_mut().ok_or(WorldError::InvalidStructure)?;
        growth.total_allocated = total;
        growth.last_allocated = amount;
        growth.validate(self.transform.scale())
    }
}

/// Outward-rounded world bounds for a visual instance, within the GPU preview domain.
pub(crate) fn transformed_bounds(
    graph: &Graph,
    transform: Transform,
) -> Result<Bounds, WorldError> {
    let scale = transform.scale();
    if !(0.01..=100.0).contains(&scale) {
        return Err(WorldError::OutOfRange);
    }
    let b = graph.bounds();
    let t = transform.translation();
    let map = |v: Vec3, upper: bool| -> Result<Vec3, WorldError> {
        let mut result = [0.0; 3];
        for (index, (coordinate, offset)) in [(v.x(), t.x()), (v.y(), t.y()), (v.z(), t.z())]
            .into_iter()
            .enumerate()
        {
            let product = coordinate * scale;
            let sum = if upper {
                product.next_up() + offset
            } else {
                product.next_down() + offset
            };
            let value = if upper {
                sum.next_up()
            } else {
                sum.next_down()
            };
            if !value.is_finite() || value.abs() > 10_000.0 {
                return Err(WorldError::OutOfRange);
            }
            result[index] = value;
        }
        Ok(Vec3::new(result[0], result[1], result[2])?)
    };
    Ok(Bounds {
        min: map(b.min, false)?,
        max: map(b.max, true)?,
    })
}
