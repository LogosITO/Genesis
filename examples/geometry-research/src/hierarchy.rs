//! Research-owned hierarchical authoring; execution materializes analytic GPU primitives.

use analytic_field::{Capsule, Sphere};
use analytic_renderer::{Primitive, Scene};
use serde::Deserialize;
use spatial_math::{Transform, Vec3};
use std::{collections::HashMap, fmt};

pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
pub const MAX_NODES: usize = 64;
pub const MAX_DEPTH: usize = 12;
pub const MAX_VISITS: usize = 2048;
pub const MAX_FEATURES: usize = 512;

#[derive(Debug)]
pub enum HierarchyError {
    Parse(serde_json::Error),
    Invalid(&'static str),
    Budget(&'static str),
}
impl fmt::Display for HierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid JSON: {error}"),
            Self::Invalid(detail) => write!(f, "invalid hierarchy: {detail}"),
            Self::Budget(detail) => write!(f, "hierarchy budget exceeded: {detail}"),
        }
    }
}
impl std::error::Error for HierarchyError {}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    #[serde(default)]
    pub at: [f64; 3],
    #[serde(default)]
    pub angle_deg: f64,
    #[serde(default = "one")]
    pub scale: f64,
}
const fn one() -> f64 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    format_version: u32,
    root: String,
    definitions: Vec<Definition>,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Definition {
    Sphere {
        id: String,
        radius: f64,
        color: [f32; 3],
    },
    Capsule {
        id: String,
        a: [f64; 3],
        b: [f64; 3],
        radius: f64,
        color: [f32; 3],
    },
}
impl Definition {
    fn id(&self) -> &str {
        match self {
            Self::Sphere { id, .. } | Self::Capsule { id, .. } => id,
        }
    }
    fn validate(&self) -> Result<(), HierarchyError> {
        let (radius, color) = match self {
            Self::Sphere { radius, color, .. } => (radius, color),
            Self::Capsule {
                a,
                b,
                radius,
                color,
                ..
            } => {
                for point in [a, b] {
                    if point.iter().any(|v| !v.is_finite() || v.abs() > 1000.0) {
                        return Err(HierarchyError::Invalid("capsule endpoint"));
                    }
                }
                (radius, color)
            }
        };
        if !radius.is_finite()
            || !(0.0001..=1000.0).contains(radius)
            || color
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(HierarchyError::Invalid("leaf dimensions or color"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Child {
    node: String,
    #[serde(flatten)]
    placement: Placement,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Node {
    Leaf {
        id: String,
        definition: String,
    },
    Group {
        id: String,
        children: Vec<Child>,
    },
    Repeat {
        id: String,
        child: String,
        count: usize,
        step: Placement,
    },
}
impl Node {
    fn id(&self) -> &str {
        match self {
            Self::Leaf { id, .. } | Self::Group { id, .. } | Self::Repeat { id, .. } => id,
        }
    }
}

#[derive(Clone, Copy)]
struct Pose {
    at: [f64; 3],
    angle: f64,
    scale: f64,
}
impl Pose {
    const IDENTITY: Self = Self {
        at: [0.0; 3],
        angle: 0.0,
        scale: 1.0,
    };
    fn point(self, p: [f64; 3]) -> [f64; 3] {
        let (sin, cos) = self.angle.sin_cos();
        [
            self.at[0] + self.scale * (cos * p[0] - sin * p[1]),
            self.at[1] + self.scale * (sin * p[0] + cos * p[1]),
            self.at[2] + self.scale * p[2],
        ]
    }
    fn then(self, placement: Placement) -> Result<Self, HierarchyError> {
        if !placement.scale.is_finite()
            || !(0.01..=100.0).contains(&placement.scale)
            || !placement.angle_deg.is_finite()
            || placement.angle_deg.abs() > 360.0
            || placement
                .at
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1000.0)
        {
            return Err(HierarchyError::Invalid("placement"));
        }
        let result = Self {
            at: self.point(placement.at),
            angle: self.angle + placement.angle_deg.to_radians(),
            scale: self.scale * placement.scale,
        };
        if !result.angle.is_finite()
            || !result.scale.is_finite()
            || result.at.iter().any(|v| !v.is_finite() || v.abs() > 9000.0)
        {
            return Err(HierarchyError::Invalid("composed placement"));
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}
impl Bounds {
    fn include(&mut self, point: [f64; 3], radius: f64) {
        for (axis, value) in point.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(value - radius);
            self.max[axis] = self.max[axis].max(value + radius);
        }
    }
}

/// Validated source DAG. It is not a `WorldState` object or a collision authority.
pub struct Hierarchy {
    source: Source,
    nodes: HashMap<String, usize>,
    definitions: HashMap<String, usize>,
    root: usize,
}

/// Fully expanded analytic snapshot, with deterministic IDs in depth-first order.
pub struct Materialized {
    pub scene: Scene,
    pub bounds: Option<Bounds>,
    pub visits: usize,
}

impl Hierarchy {
    pub fn parse(bytes: &[u8]) -> Result<Self, HierarchyError> {
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(HierarchyError::Budget("source bytes"));
        }
        let source: Source = serde_json::from_slice(bytes).map_err(HierarchyError::Parse)?;
        if source.format_version != 1 {
            return Err(HierarchyError::Invalid("format version"));
        }
        if source.nodes.len() > MAX_NODES || source.definitions.len() > MAX_NODES {
            return Err(HierarchyError::Budget("authored nodes or definitions"));
        }
        let mut nodes = HashMap::new();
        let mut definitions = HashMap::new();
        for (i, definition) in source.definitions.iter().enumerate() {
            definition.validate()?;
            if definition.id().is_empty()
                || definitions.insert(definition.id().to_owned(), i).is_some()
            {
                return Err(HierarchyError::Invalid("duplicate or empty definition ID"));
            }
        }
        for (i, node) in source.nodes.iter().enumerate() {
            if node.id().is_empty() || nodes.insert(node.id().to_owned(), i).is_some() {
                return Err(HierarchyError::Invalid("duplicate or empty node ID"));
            }
        }
        let root = *nodes
            .get(&source.root)
            .ok_or(HierarchyError::Invalid("root"))?;
        let hierarchy = Self {
            source,
            nodes,
            definitions,
            root,
        };
        let mut states = vec![0_u8; hierarchy.source.nodes.len()];
        for i in 0..states.len() {
            hierarchy.validate_node(i, 1, &mut states)?;
        }
        hierarchy.materialize(1.0)?;
        Ok(hierarchy)
    }

    fn validate_node(
        &self,
        i: usize,
        depth: usize,
        states: &mut [u8],
    ) -> Result<(), HierarchyError> {
        if depth > MAX_DEPTH {
            return Err(HierarchyError::Budget("definition depth"));
        }
        if states[i] == 1 {
            return Err(HierarchyError::Invalid("cycle"));
        }
        if states[i] == 2 {
            return Ok(());
        }
        states[i] = 1;
        match &self.source.nodes[i] {
            Node::Leaf { definition, .. } => {
                if !self.definitions.contains_key(definition) {
                    return Err(HierarchyError::Invalid("definition reference"));
                }
            }
            Node::Group { children, .. } => {
                if children.len() > 16 {
                    return Err(HierarchyError::Budget("group children"));
                }
                for child in children {
                    Pose::IDENTITY.then(child.placement)?;
                    self.validate_node(self.node(&child.node)?, depth + 1, states)?;
                }
            }
            Node::Repeat {
                child, count, step, ..
            } => {
                if *count > 32 {
                    return Err(HierarchyError::Budget("repeat count"));
                }
                Pose::IDENTITY.then(*step)?;
                self.validate_node(self.node(child)?, depth + 1, states)?;
            }
        }
        states[i] = 2;
        Ok(())
    }

    fn node(&self, id: &str) -> Result<usize, HierarchyError> {
        self.nodes
            .get(id)
            .copied()
            .ok_or(HierarchyError::Invalid("node reference"))
    }

    pub fn node_count(&self) -> usize {
        self.source.nodes.len()
    }
    pub fn definition_count(&self) -> usize {
        self.source.definitions.len()
    }

    /// Rebuilds derived geometry after a deterministic uniform instance-scale update.
    pub fn materialize(&self, scale: f64) -> Result<Materialized, HierarchyError> {
        if !scale.is_finite() || !(0.1..=2.0).contains(&scale) {
            return Err(HierarchyError::Invalid("dynamic scale"));
        }
        let mut result = Materialized {
            scene: Scene::default(),
            bounds: None,
            visits: 0,
        };
        self.emit(
            self.root,
            Pose {
                scale,
                ..Pose::IDENTITY
            },
            1,
            &mut result,
        )?;
        Ok(result)
    }

    fn emit(
        &self,
        i: usize,
        pose: Pose,
        depth: usize,
        out: &mut Materialized,
    ) -> Result<(), HierarchyError> {
        out.visits += 1;
        if out.visits > MAX_VISITS || depth > MAX_DEPTH {
            return Err(HierarchyError::Budget("expanded visits or depth"));
        }
        match &self.source.nodes[i] {
            Node::Leaf { definition, .. } => {
                if out.scene.primitives().len() == MAX_FEATURES {
                    return Err(HierarchyError::Budget("derived features"));
                }
                let id = out.scene.primitives().len() as u32;
                let definition = &self.source.definitions[*self
                    .definitions
                    .get(definition)
                    .ok_or(HierarchyError::Invalid("definition reference"))?];
                let (primitive, a, b, radius) = match definition {
                    Definition::Sphere { radius, color, .. } => {
                        let center = pose.point([0.0; 3]);
                        let r = radius * pose.scale;
                        let primitive = Primitive::sphere(
                            id,
                            Sphere::new(r).map_err(|_| HierarchyError::Invalid("sphere"))?,
                            Transform::new(vec3(center)?, 1.0)
                                .map_err(|_| HierarchyError::Invalid("sphere transform"))?,
                            *color,
                        )
                        .map_err(|_| HierarchyError::Invalid("sphere render range"))?;
                        (primitive, center, center, r)
                    }
                    Definition::Capsule {
                        a,
                        b,
                        radius,
                        color,
                        ..
                    } => {
                        let a = pose.point(*a);
                        let b = pose.point(*b);
                        let r = radius * pose.scale;
                        let capsule = Capsule::new(vec3(a)?, vec3(b)?, r)
                            .map_err(|_| HierarchyError::Invalid("capsule"))?;
                        let primitive = Primitive::capsule(id, capsule, *color)
                            .map_err(|_| HierarchyError::Invalid("capsule render range"))?;
                        (primitive, a, b, r)
                    }
                };
                if !radius.is_finite() || radius < 0.0001 {
                    return Err(HierarchyError::Invalid("derived radius"));
                }
                let bounds = out.bounds.get_or_insert(Bounds {
                    min: [f64::INFINITY; 3],
                    max: [f64::NEG_INFINITY; 3],
                });
                bounds.include(a, radius);
                bounds.include(b, radius);
                out.scene
                    .push(primitive)
                    .map_err(|_| HierarchyError::Budget("scene capacity"))?;
            }
            Node::Group { children, .. } => {
                for child in children {
                    self.emit(
                        self.node(&child.node)?,
                        pose.then(child.placement)?,
                        depth + 1,
                        out,
                    )?;
                }
            }
            Node::Repeat {
                child, count, step, ..
            } => {
                let child = self.node(child)?;
                let mut current = pose;
                for _ in 0..*count {
                    self.emit(child, current, depth + 1, out)?;
                    current = current.then(*step)?;
                }
            }
        }
        Ok(())
    }
}

fn vec3(p: [f64; 3]) -> Result<Vec3, HierarchyError> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 9000.0) {
        return Err(HierarchyError::Invalid("derived coordinate"));
    }
    Vec3::new(p[0], p[1], p[2]).map_err(|_| HierarchyError::Invalid("derived coordinate"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use analytic_renderer::Ray;

    const BRANCH: &[u8] = include_bytes!("../definitions/branching.json");
    const ARCADE: &[u8] = include_bytes!("../definitions/arcade.json");

    #[test]
    fn two_families_share_one_grammar_and_stable_ids() {
        for (source, minimum) in [(BRANCH, 100), (ARCADE, 20)] {
            let hierarchy = Hierarchy::parse(source).unwrap();
            let original = hierarchy.materialize(1.0).unwrap();
            let grown = hierarchy.materialize(1.3).unwrap();
            assert!(original.scene.primitives().len() >= minimum);
            assert_eq!(
                original.scene.primitives().len(),
                grown.scene.primitives().len()
            );
            for (a, b) in original
                .scene
                .primitives()
                .iter()
                .zip(grown.scene.primitives())
            {
                assert_eq!(a.id, b.id);
            }
            assert_ne!(original.bounds.unwrap().max, grown.bounds.unwrap().max);
            assert_eq!(original.visits, grown.visits);
        }
    }

    #[test]
    fn nested_transform_and_cpu_bvh_agree() {
        let hierarchy = Hierarchy::parse(BRANCH).unwrap();
        let geometry = hierarchy.materialize(1.0).unwrap();
        let bounds = geometry.bounds.unwrap();
        for p in geometry.scene.primitives() {
            let c = p.center();
            assert!(
                [c.x(), c.y(), c.z()]
                    .into_iter()
                    .enumerate()
                    .all(|(axis, v)| v >= bounds.min[axis] && v <= bounds.max[axis])
            );
        }
        let bvh = analytic_renderer::Bvh::build(&geometry.scene).unwrap();
        for x in -20..=20 {
            let ray = Ray::new(
                Vec3::new(x as f64 * 0.15, 1.5, -8.0).unwrap(),
                Vec3::new(0.0, 0.0, 1.0).unwrap(),
                0.001,
                20.0,
            )
            .unwrap();
            assert_eq!(
                geometry.scene.intersect(ray).map(|h| h.id),
                bvh.intersect(ray).map(|h| h.id)
            );
        }
    }

    #[test]
    fn malformed_and_excessive_sources_are_rejected() {
        let cycle = br#"{"format_version":1,"root":"a","definitions":[],"nodes":[{"kind":"group","id":"a","children":[{"node":"a"}]}]}"#;
        assert!(matches!(
            Hierarchy::parse(cycle),
            Err(HierarchyError::Invalid("cycle"))
        ));
        let missing = br#"{"format_version":1,"root":"a","definitions":[],"nodes":[{"kind":"leaf","id":"a","definition":"none"}]}"#;
        assert!(matches!(
            Hierarchy::parse(missing),
            Err(HierarchyError::Invalid("definition reference"))
        ));
        let bad = br#"{"format_version":1,"root":"a","definitions":[],"nodes":[{"kind":"repeat","id":"a","child":"b","count":33,"step":{}},{"kind":"group","id":"b","children":[]}]}"#;
        assert!(matches!(
            Hierarchy::parse(bad),
            Err(HierarchyError::Budget("repeat count"))
        ));
        let h = Hierarchy::parse(ARCADE).unwrap();
        assert!(h.materialize(f64::NAN).is_err());
        assert!(h.materialize(0.0).is_err());
    }

    #[test]
    fn work_budget_empty_group_and_definition_revision() {
        let empty = br#"{"format_version":1,"root":"empty","definitions":[],"nodes":[{"kind":"group","id":"empty","children":[]}]}"#;
        let empty = Hierarchy::parse(empty).unwrap().materialize(1.0).unwrap();
        assert!(empty.scene.primitives().is_empty());
        assert!(empty.bounds.is_none());

        let excessive = br#"{"format_version":1,"root":"outer","definitions":[{"kind":"sphere","id":"s","radius":1,"color":[1,1,1]}],"nodes":[{"kind":"leaf","id":"leaf","definition":"s"},{"kind":"repeat","id":"inner","child":"leaf","count":32,"step":{}},{"kind":"repeat","id":"outer","child":"inner","count":32,"step":{}}]}"#;
        assert!(matches!(
            Hierarchy::parse(excessive),
            Err(HierarchyError::Budget("derived features"))
        ));

        let original = Hierarchy::parse(BRANCH).unwrap().materialize(1.0).unwrap();
        let revised_bytes = std::str::from_utf8(BRANCH)
            .unwrap()
            .replace("\"radius\":0.09", "\"radius\":0.12");
        let revised = Hierarchy::parse(revised_bytes.as_bytes())
            .unwrap()
            .materialize(1.0)
            .unwrap();
        assert_eq!(
            original.scene.primitives()[0].id,
            revised.scene.primitives()[0].id
        );
        assert_ne!(
            original.scene.primitives()[0].capsule_radius(),
            revised.scene.primitives()[0].capsule_radius()
        );
    }

    #[test]
    fn invalid_transform_and_depth_are_rejected() {
        let transform = br#"{"format_version":1,"root":"g","definitions":[],"nodes":[{"kind":"group","id":"g","children":[{"node":"e","scale":0}]},{"kind":"group","id":"e","children":[]}]}"#;
        assert!(matches!(
            Hierarchy::parse(transform),
            Err(HierarchyError::Invalid("placement"))
        ));

        let mut nodes = vec![serde_json::json!({"kind":"group","id":"end","children":[]})];
        let mut child = "end".to_owned();
        for index in 0..MAX_DEPTH {
            let id = format!("level_{index}");
            nodes.push(serde_json::json!({"kind":"group","id":id,"children":[{"node":child}]}));
            child = id;
        }
        let deep = serde_json::to_vec(
            &serde_json::json!({"format_version":1,"root":child,"definitions":[],"nodes":nodes}),
        )
        .unwrap();
        assert!(matches!(
            Hierarchy::parse(&deep),
            Err(HierarchyError::Budget(_))
        ));
    }

    #[test]
    fn nested_rotation_and_shared_leaf_scales_are_data_driven() {
        let source = br#"{"format_version":1,"root":"outer","definitions":[{"kind":"sphere","id":"orb","radius":0.25,"color":[1,0,0]}],"nodes":[{"kind":"leaf","id":"leaf","definition":"orb"},{"kind":"group","id":"inner","children":[{"node":"leaf","at":[1,0,0]},{"node":"leaf","at":[2,0,0],"scale":2}]},{"kind":"group","id":"outer","children":[{"node":"inner","at":[1,0,0],"angle_deg":90}]}]}"#;
        let result = Hierarchy::parse(source).unwrap().materialize(1.0).unwrap();
        let primitives = result.scene.primitives();
        assert_eq!(primitives.len(), 2);
        assert!((primitives[0].center().x() - 1.0).abs() < 1e-12);
        assert!((primitives[0].center().y() - 1.0).abs() < 1e-12);
        assert!((primitives[1].center().x() - 1.0).abs() < 1e-12);
        assert!((primitives[1].center().y() - 2.0).abs() < 1e-12);
        assert!((primitives[0].dimensions().x() - 0.25).abs() < 1e-12);
        assert!((primitives[1].dimensions().x() - 0.5).abs() < 1e-12);
    }
}
