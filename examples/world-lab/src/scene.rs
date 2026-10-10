use analytic_field::Sphere;
use analytic_renderer::{
    Camera, Scene,
    material::{Material, MaterialPattern, MaterialSet},
};
use serde::Deserialize;
use spatial_math::{Transform, Vec3};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use world_authoring::{DefinitionRevision, field_graph};
use world_state::{DeterministicSeed, MAX_FIELD_DEFINITIONS, MAX_FIELD_INSTANCES, WorldState};

const MAX_SCENE_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    format_version: u32,
    name: String,
    definitions: Vec<Definition>,
    materials: Vec<MaterialSource>,
    spheres: Vec<SphereSource>,
    fields: Vec<FieldSource>,
    camera: CameraSource,
    #[serde(default)]
    render: RenderSource,
    background: [f32; 3],
    light_direction: [f32; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    name: String,
    path: PathBuf,
    revision: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialSource {
    name: String,
    base: [f32; 3],
    alternate: [f32; 3],
    emission: [f32; 3],
    pattern: PatternSource,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PatternSource {
    Constant,
    Gradient {
        axis: [f32; 3],
        frequency: f32,
        phase: f32,
        speed: f32,
    },
    Bands {
        axis: [f32; 3],
        frequency: f32,
        phase: f32,
        speed: f32,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SphereSource {
    radius: f64,
    translation: [f64; 3],
    scale: f64,
    material: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldSource {
    definition: String,
    translation: [f64; 3],
    scale: f64,
    material: String,
    #[serde(default = "enabled")]
    enabled: bool,
}
fn enabled() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraSource {
    target: [f64; 3],
    distance: f64,
    yaw: f64,
    elevation: f64,
    fov_radians: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderSource {
    #[serde(default = "enabled")]
    show_analytic: bool,
    #[serde(default = "enabled")]
    show_fields: bool,
    #[serde(default)]
    diagnostic: bool,
    #[serde(default)]
    normals: bool,
}
impl Default for RenderSource {
    fn default() -> Self {
        Self {
            show_analytic: true,
            show_fields: true,
            diagnostic: false,
            normals: false,
        }
    }
}

pub struct LabScene {
    pub name: String,
    pub world: WorldState,
    pub materials: MaterialSet,
    pub target: Vec3,
    pub distance: f64,
    pub yaw: f64,
    pub elevation: f64,
    pub fov: f64,
    pub background: [f32; 3],
    pub light_direction: [f32; 3],
    pub show_analytic: bool,
    pub show_fields: bool,
    pub diagnostic: bool,
    pub normals: bool,
    pub graph_nodes: usize,
}

fn vec3(value: [f64; 3]) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(value[0], value[1], value[2])?)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()).into());
    }
    Ok(bytes)
}

impl LabScene {
    pub fn camera(
        &self,
        yaw: f64,
        elevation: f64,
        distance: f64,
    ) -> Result<Camera, Box<dyn Error>> {
        let origin = Vec3::new(
            self.target.x() + distance * elevation.cos() * yaw.sin(),
            self.target.y() + distance * elevation.sin(),
            self.target.z() + distance * elevation.cos() * yaw.cos(),
        )?;
        Ok(Camera::look_at(
            origin,
            self.target,
            Vec3::new(0.0, 1.0, 0.0)?,
            self.fov,
        )?)
    }

    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        let source: Source = serde_json::from_slice(&read_bounded(path, MAX_SCENE_BYTES)?)?;
        if source.format_version != 1 {
            return Err("unsupported scene format_version".into());
        }
        if source.name.trim().is_empty() || source.name.len() > 64 {
            return Err("invalid scene name".into());
        }
        if source.definitions.len() > MAX_FIELD_DEFINITIONS
            || source.fields.len() > MAX_FIELD_INSTANCES
            || source.spheres.len() > 64
            || source.materials.len() > 72
        {
            return Err("scene capacity exceeded".into());
        }
        let parent = path
            .canonicalize()?
            .parent()
            .ok_or("scene has no parent")?
            .to_path_buf();
        let mut definitions = BTreeMap::new();
        let mut graph_nodes = 0;
        for definition in source.definitions {
            if definition.name.is_empty() || definitions.contains_key(&definition.name) {
                return Err("duplicate or empty definition name".into());
            }
            let file = parent.join(&definition.path).canonicalize()?;
            if !file.starts_with(&parent) {
                return Err("definition path escapes scene directory".into());
            }
            let bytes = read_bounded(&file, field_graph::MAX_SOURCE_BYTES as u64)?;
            let graph = field_graph::compile_json(&bytes)?;
            let revision = DefinitionRevision::of(&bytes).hex();
            if definition
                .revision
                .as_ref()
                .is_some_and(|expected| *expected != revision)
            {
                return Err(format!(
                    "definition {} revision mismatch: actual {revision}",
                    definition.name
                )
                .into());
            }
            graph_nodes += graph.nodes().len();
            definitions.insert(definition.name, bytes);
        }
        let mut material_map = BTreeMap::new();
        for source in source.materials {
            if source.name.is_empty() || material_map.contains_key(&source.name) {
                return Err("duplicate or empty material name".into());
            }
            let pattern = match source.pattern {
                PatternSource::Constant => MaterialPattern::Constant,
                PatternSource::Gradient {
                    axis,
                    frequency,
                    phase,
                    speed,
                } => MaterialPattern::Gradient {
                    axis,
                    frequency,
                    phase,
                    speed,
                },
                PatternSource::Bands {
                    axis,
                    frequency,
                    phase,
                    speed,
                } => MaterialPattern::Bands {
                    axis,
                    frequency,
                    phase,
                    speed,
                },
            };
            material_map.insert(
                source.name,
                Material::new(source.base, source.alternate, source.emission, pattern)?,
            );
        }
        let mut world = WorldState::new(DeterministicSeed(0));
        let mut materials = MaterialSet::default();
        for sphere in source.spheres {
            let material = *material_map
                .get(&sphere.material)
                .ok_or("unknown sphere material")?;
            let id = world.spawn_sphere(
                Sphere::new(sphere.radius)?,
                Transform::new(vec3(sphere.translation)?, sphere.scale)?,
                0.0,
            )?;
            materials.set_sphere(id, material);
        }
        for field in source.fields {
            let bytes = definitions
                .get(&field.definition)
                .ok_or("unknown field definition")?;
            let material = *material_map
                .get(&field.material)
                .ok_or("unknown field material")?;
            let id = world.spawn_field(
                bytes,
                Transform::new(vec3(field.translation)?, field.scale)?,
            )?;
            world.set_field_enabled(id, field.enabled)?;
            materials.set_field(id, material);
        }
        if source.camera.distance < 0.1
            || source.camera.distance > 500.0
            || !source.camera.yaw.is_finite()
            || !source.camera.elevation.is_finite()
            || source.camera.elevation.abs() > 1.4
        {
            return Err("invalid camera orbit".into());
        }
        let scene = Self {
            name: source.name,
            world,
            materials,
            target: vec3(source.camera.target)?,
            distance: source.camera.distance,
            yaw: source.camera.yaw,
            elevation: source.camera.elevation,
            fov: source.camera.fov_radians,
            background: source.background,
            light_direction: source.light_direction,
            show_analytic: source.render.show_analytic,
            show_fields: source.render.show_fields,
            diagnostic: source.render.diagnostic,
            normals: source.render.normals,
            graph_nodes,
        };
        scene.camera(scene.yaw, scene.elevation, scene.distance)?;
        let light_length_squared = scene.light_direction.iter().map(|x| x * x).sum::<f32>();
        if scene
            .background
            .iter()
            .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
            || scene.light_direction.iter().any(|x| !x.is_finite())
            || !light_length_squared.is_finite()
            || light_length_squared < 1e-12
        {
            return Err("invalid background or light".into());
        }
        scene.world.validate()?;
        analytic_renderer::Bvh::build(&Scene::from_world(&scene.world)?)?;
        Ok(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn showcase_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/showcase.json");
        let active = LabScene::load(&path).unwrap();
        assert!(active.world.field_instances().len() >= 2);
        assert!(active.graph_nodes >= 4);
    }
}
