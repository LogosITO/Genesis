//! World-owned visual field occurrences; compiled graphs are derived from exact source bytes.

use crate::{EntityId, WorldError};
use serde::{Deserialize, Serialize};
use spatial_math::{Transform, Vec3};
use std::sync::Arc;
use world_authoring::{
    DefinitionRevision,
    field_graph::{Bounds, Graph, MAX_PARAMETER_CHANNELS, ParameterBinding, compile_json},
};

/// Maximum distinct visual graph revisions retained by one world.
pub const MAX_FIELD_DEFINITIONS: usize = 4;
/// Maximum visual graph occurrences in one world.
pub const MAX_FIELD_INSTANCES: usize = 8;
/// Interpretation version of the persisted graph source.
pub const FIELD_GRAPH_SEMANTICS_VERSION: u32 = 1;
/// Version of the bounded runtime parameter and resource rule schema.
pub const FIELD_PARAMETER_SCHEMA_VERSION: u32 = 1;

/// One declarative resource-driven parameter rule on an authored primitive.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterRule {
    /// Stable node ID and primitive component.
    pub binding: ParameterBinding,
    /// Source sample multiplier per second.
    pub uptake: f64,
    /// Parameter gain per unit allocated from the finite ledger.
    pub gain_per_unit: f64,
    /// Saturating parameter value.
    pub maximum: f64,
    /// Positive or negative primitive-axis surface feature.
    pub positive_site: bool,
    /// When false, the sample site remains at the authored initial geometry.
    pub feedback: bool,
}

/// World-owned state of one resource-driven parameter channel.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterChannel {
    rule: ParameterRule,
    initial: f64,
    current: f64,
    total_allocated: f64,
    last_allocated: f64,
}
impl ParameterChannel {
    /// Declarative validated rule.
    pub fn rule(&self) -> &ParameterRule {
        &self.rule
    }
    /// Current authoritative geometric value.
    pub fn current(&self) -> f64 {
        self.current
    }
    /// Total units withdrawn for this channel.
    pub fn total_allocated(&self) -> f64 {
        self.total_allocated
    }
    /// Units withdrawn in the last completed tick.
    pub fn last_allocated(&self) -> f64 {
        self.last_allocated
    }
    /// Remaining resource units before saturation.
    pub fn remaining_units(&self) -> f64 {
        ((self.rule.maximum - self.current) / self.rule.gain_per_unit).max(0.0)
    }
}

/// Bounded reusable runtime values, separate from immutable authored bytes.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldParameters {
    schema_version: u32,
    revision: u64,
    channels: Vec<ParameterChannel>,
}
impl FieldParameters {
    pub(crate) fn new(source: &Graph, rules: Vec<ParameterRule>) -> Result<Self, WorldError> {
        if rules.is_empty() || rules.len() > MAX_PARAMETER_CHANNELS {
            return Err(WorldError::Capacity);
        }
        let channels = rules
            .into_iter()
            .map(|rule| {
                let initial = source
                    .parameter_value(&rule.binding)
                    .map_err(|_| WorldError::InvalidStructure)?;
                let channel = ParameterChannel {
                    rule,
                    initial,
                    current: initial,
                    total_allocated: 0.0,
                    last_allocated: 0.0,
                };
                source
                    .parameter_site(&channel.rule.binding, channel.rule.positive_site)
                    .map_err(|_| WorldError::InvalidStructure)?;
                Ok(channel)
            })
            .collect::<Result<Vec<_>, WorldError>>()?;
        let state = Self {
            schema_version: FIELD_PARAMETER_SCHEMA_VERSION,
            revision: 0,
            channels,
        };
        state.validate(source)?;
        Ok(state)
    }
    /// Stable channel order from the authored rule list.
    pub fn channels(&self) -> &[ParameterChannel] {
        &self.channels
    }
    /// Monotonic revision of changed runtime geometric values.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn graph(&self, source: &Graph) -> Result<Graph, WorldError> {
        source
            .with_parameters(
                &self
                    .channels
                    .iter()
                    .map(|channel| (channel.rule.binding.clone(), channel.current))
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| WorldError::InvalidStructure)
    }
    pub(crate) fn validate(&self, source: &Graph) -> Result<(), WorldError> {
        if self.schema_version != FIELD_PARAMETER_SCHEMA_VERSION
            || self.channels.is_empty()
            || self.channels.len() > MAX_PARAMETER_CHANNELS
        {
            return Err(WorldError::InvalidStructure);
        }
        for (index, channel) in self.channels.iter().enumerate() {
            let rule = &channel.rule;
            if self.channels[..index]
                .iter()
                .any(|prior| prior.rule.binding == rule.binding)
                || [
                    channel.initial,
                    channel.current,
                    channel.total_allocated,
                    channel.last_allocated,
                    rule.uptake,
                    rule.gain_per_unit,
                    rule.maximum,
                ]
                .iter()
                .any(|value| !value.is_finite())
                || !(0.1..=2.0).contains(&channel.initial)
                || !(channel.initial..=2.0).contains(&rule.maximum)
                || !(0.0..=100.0).contains(&rule.uptake)
                || !(0.001..=10.0).contains(&rule.gain_per_unit)
                || channel.total_allocated < 0.0
                || channel.total_allocated
                    > (rule.maximum - channel.initial) / rule.gain_per_unit + 1e-9
                || !(0.0..=channel.total_allocated).contains(&channel.last_allocated)
                || (channel.current
                    - (channel.initial + channel.total_allocated * rule.gain_per_unit)
                        .min(rule.maximum))
                .abs()
                    > 1e-10
                || source
                    .parameter_value(&rule.binding)
                    .map_err(|_| WorldError::InvalidStructure)?
                    != channel.initial
                || source
                    .parameter_site(&rule.binding, rule.positive_site)
                    .is_err()
            {
                return Err(WorldError::InvalidStructure);
            }
        }
        self.graph(source)?;
        Ok(())
    }
    pub(crate) fn grow(&mut self, amounts: &[f64], source: &Graph) -> Result<bool, WorldError> {
        if amounts.len() != self.channels.len() {
            return Err(WorldError::OutOfRange);
        }
        let mut changed = false;
        for (channel, amount) in self.channels.iter_mut().zip(amounts) {
            if !amount.is_finite() || *amount < 0.0 || *amount > channel.remaining_units() + 1e-12 {
                return Err(WorldError::OutOfRange);
            }
            let old = channel.current;
            channel.total_allocated += amount;
            channel.last_allocated = *amount;
            channel.current = (channel.initial
                + channel.total_allocated * channel.rule.gain_per_unit)
                .min(channel.rule.maximum);
            changed |= channel.current != old;
        }
        if changed {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(WorldError::IdExhausted)?;
        }
        self.validate(source)?;
        Ok(changed)
    }
}

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

/// Two independently fed sphere radii in the fixed two-lobe graph schema.
/// Lobe indices 0 and 1 are stable; sites may follow their outer surfaces.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldMorphology {
    initial_radii: [f64; 2],
    radii: [f64; 2],
    max_radii: [f64; 2],
    uptake: [f64; 2],
    radius_per_unit: [f64; 2],
    total_allocated: [f64; 2],
    last_allocated: [f64; 2],
    feedback: bool,
}
impl FieldMorphology {
    pub(crate) fn new(
        initial_radii: [f64; 2],
        uptake: [f64; 2],
        radius_per_unit: [f64; 2],
        max_radii: [f64; 2],
        feedback: bool,
    ) -> Result<Self, WorldError> {
        let state = Self {
            initial_radii,
            radii: initial_radii,
            max_radii,
            uptake,
            radius_per_unit,
            total_allocated: [0.0; 2],
            last_allocated: [0.0; 2],
            feedback,
        };
        state.validate()?;
        Ok(state)
    }
    /// Current validated local-space radii.
    pub fn radii(&self) -> [f64; 2] {
        self.radii
    }
    /// Initial local-space radii, also used by the no-feedback ablation.
    pub fn initial_radii(&self) -> [f64; 2] {
        self.initial_radii
    }
    /// Per-lobe resource demand multipliers.
    pub fn uptake(&self) -> [f64; 2] {
        self.uptake
    }
    /// Radius gained per allocated source unit.
    pub fn radius_per_unit(&self) -> [f64; 2] {
        self.radius_per_unit
    }
    /// Upper local-space radius limits.
    pub fn max_radii(&self) -> [f64; 2] {
        self.max_radii
    }
    /// Cumulative allocations for each stable lobe.
    pub fn total_allocated(&self) -> [f64; 2] {
        self.total_allocated
    }
    /// Last completed tick's allocations for each stable lobe.
    pub fn last_allocated(&self) -> [f64; 2] {
        self.last_allocated
    }
    /// Whether environmental sampling follows the current lobe surfaces.
    pub fn feedback(&self) -> bool {
        self.feedback
    }
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        for i in 0..2 {
            let values = [
                self.initial_radii[i],
                self.radii[i],
                self.max_radii[i],
                self.uptake[i],
                self.radius_per_unit[i],
                self.total_allocated[i],
                self.last_allocated[i],
            ];
            if values.iter().any(|v| !v.is_finite())
                || !(0.1..=2.0).contains(&self.initial_radii[i])
                || !(self.initial_radii[i]..=2.0).contains(&self.max_radii[i])
                || !(0.0..=100.0).contains(&self.uptake[i])
                || !(0.001..=10.0).contains(&self.radius_per_unit[i])
                || self.total_allocated[i] < 0.0
                || self.total_allocated[i]
                    > (self.max_radii[i] - self.initial_radii[i]) / self.radius_per_unit[i] + 1e-9
                || !(0.0..=self.total_allocated[i]).contains(&self.last_allocated[i])
                || (self.radii[i]
                    - (self.initial_radii[i] + self.total_allocated[i] * self.radius_per_unit[i])
                        .min(self.max_radii[i]))
                .abs()
                    > 1e-10
            {
                return Err(WorldError::InvalidStructure);
            }
        }
        Ok(())
    }
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldInstance {
    id: EntityId,
    revision: DefinitionRevision,
    transform: Transform,
    enabled: bool,
    generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    growth: Option<FieldGrowth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    morphology: Option<FieldMorphology>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parameters: Option<FieldParameters>,
    #[serde(skip)]
    runtime_graph: Option<Arc<Graph>>,
}
impl PartialEq for FieldInstance {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.revision == other.revision
            && self.transform == other.transform
            && self.enabled == other.enabled
            && self.generation == other.generation
            && self.growth == other.growth
            && self.morphology == other.morphology
            && self.parameters == other.parameters
    }
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
    /// Optional bounded two-lobe morphology; visual only.
    pub fn morphology(&self) -> Option<&FieldMorphology> {
        self.morphology.as_ref()
    }
    /// Generic resource-driven primitive parameter channels, if enabled.
    pub fn parameters(&self) -> Option<&FieldParameters> {
        self.parameters.as_ref()
    }
    pub(crate) fn runtime_graph(&self) -> Option<&Graph> {
        self.runtime_graph.as_deref()
    }
    pub(crate) fn new(id: EntityId, revision: DefinitionRevision, transform: Transform) -> Self {
        Self {
            id,
            revision,
            transform,
            enabled: true,
            generation: 0,
            growth: None,
            morphology: None,
            parameters: None,
            runtime_graph: None,
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
        if self.growth.is_some()
            || self.morphology.is_some()
            || self.parameters.is_some()
            || growth.initial_scale != self.transform.scale()
        {
            return Err(WorldError::InvalidStructure);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(WorldError::IdExhausted)?;
        self.growth = Some(growth);
        Ok(())
    }
    pub(crate) fn enable_morphology(
        &mut self,
        morphology: FieldMorphology,
        source: &Graph,
    ) -> Result<(), WorldError> {
        if self.growth.is_some() || self.morphology.is_some() || self.parameters.is_some() {
            return Err(WorldError::InvalidStructure);
        }
        let graph = source
            .with_two_lobe_radii(morphology.radii())
            .map_err(|_| WorldError::InvalidStructure)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(WorldError::IdExhausted)?;
        self.runtime_graph = Some(Arc::new(graph));
        self.morphology = Some(morphology);
        Ok(())
    }
    pub(crate) fn grow_morphology(
        &mut self,
        amounts: [f64; 2],
        source: &Graph,
    ) -> Result<(), WorldError> {
        let morphology = self
            .morphology
            .as_mut()
            .ok_or(WorldError::InvalidStructure)?;
        let prior_radii = morphology.radii;
        for (i, amount) in amounts.into_iter().enumerate() {
            let remaining = ((morphology.max_radii[i] - morphology.radii[i])
                / morphology.radius_per_unit[i])
                .max(0.0);
            if !amount.is_finite() || amount < 0.0 || amount > remaining + 1e-12 {
                return Err(WorldError::OutOfRange);
            }
            morphology.total_allocated[i] += amount;
            morphology.last_allocated[i] = amount;
            morphology.radii[i] = (morphology.initial_radii[i]
                + morphology.total_allocated[i] * morphology.radius_per_unit[i])
                .min(morphology.max_radii[i]);
        }
        morphology.validate()?;
        if morphology.radii != prior_radii {
            let graph = source
                .with_two_lobe_radii(morphology.radii())
                .map_err(|_| WorldError::InvalidStructure)?;
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or(WorldError::IdExhausted)?;
            self.runtime_graph = Some(Arc::new(graph));
        }
        Ok(())
    }
    pub(crate) fn enable_parameters(
        &mut self,
        state: FieldParameters,
        source: &Graph,
    ) -> Result<(), WorldError> {
        if self.growth.is_some() || self.morphology.is_some() || self.parameters.is_some() {
            return Err(WorldError::InvalidStructure);
        }
        let graph = state.graph(source)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(WorldError::IdExhausted)?;
        self.runtime_graph = Some(Arc::new(graph));
        self.parameters = Some(state);
        Ok(())
    }
    pub(crate) fn grow_parameters(
        &mut self,
        amounts: &[f64],
        source: &Graph,
    ) -> Result<(), WorldError> {
        let state = self
            .parameters
            .as_mut()
            .ok_or(WorldError::InvalidStructure)?;
        if state.grow(amounts, source)? {
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or(WorldError::IdExhausted)?;
            self.runtime_graph = Some(Arc::new(state.graph(source)?));
        }
        Ok(())
    }
    pub(crate) fn rebuild_runtime_graph(&mut self, source: &Graph) -> Result<(), WorldError> {
        if self.morphology.is_some() && self.parameters.is_some() {
            return Err(WorldError::InvalidStructure);
        }
        self.runtime_graph = if let Some(morphology) = &self.morphology {
            morphology.validate()?;
            Some(Arc::new(
                source
                    .with_two_lobe_radii(morphology.radii())
                    .map_err(|_| WorldError::InvalidStructure)?,
            ))
        } else if let Some(parameters) = &self.parameters {
            parameters.validate(source)?;
            Some(Arc::new(parameters.graph(source)?))
        } else {
            None
        };
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
