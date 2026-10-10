use crate::{
    Bvh, Camera, Ray, RenderError, Scene, SemanticTarget,
    acceleration::FlatNode,
    field_compiler::{SEMANTICS_VERSION, specialized_field_source},
    field_graph_gpu::{GpuNode, GraphTracePolicy, gpu_nodes},
    material::MaterialSet,
};
use bytemuck::{Pod, Zeroable};
use std::{
    collections::VecDeque,
    sync::{Mutex, mpsc},
    time::Instant,
};
use wgpu::util::DeviceExt;
use world_authoring::{DefinitionRevision, field_graph::Graph};
use world_state::WorldState;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuCamera {
    rows: [[f32; 4]; 6],
    scene: [u32; 4], // primitive count, node count, unused, traversal mode
}
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct GpuPrimitive {
    center_kind: [f32; 4],
    dimensions: [f32; 4],
    color: [f32; 4],
    identity: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRay {
    origin_near: [f32; 4],
    direction_far: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuHit {
    distance_normal: [f32; 4],
    meta: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct GpuFieldInstance {
    position_scale: [f32; 4],
    info: [u32; 4],
    bounds_min: [f32; 4],
    bounds_max: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMixedOptions {
    trace: [f32; 4],
    info: [u32; 4], // occurrence count, node count, visibility/diagnostics flags, iteration budget
    background: [f32; 4],
    light: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct GpuMaterial {
    values: [[f32; 4]; 5],
}

struct FieldBuffers {
    nodes: Vec<GpuNode>,
    instances: Vec<GpuFieldInstance>,
    node_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    node_capacity: u64,
    instance_capacity: u64,
    materials: Vec<GpuMaterial>,
    analytic_indices: Vec<u32>,
    material_buffer: wgpu::Buffer,
    analytic_index_buffer: wgpu::Buffer,
    material_capacity: u64,
    analytic_index_capacity: u64,
    node_uploads: u64,
    material_uploads: u64,
}

#[derive(Clone, Copy)]
struct MixedSceneOptions<'a> {
    policy: GraphTracePolicy,
    flags: [bool; 3],
    materials: Option<&'a MaterialSet>,
    simulation_seconds: f32,
    background: [f32; 3],
    light_direction: [f32; 3],
    render_mode: GpuFieldRenderMode,
}
struct MixedImage {
    size: [u32; 2],
    texture: wgpu::Texture,
}

enum DrawMode<'a> {
    Analytic(GpuTraversal),
    Mixed(&'a WorldState, MixedSceneOptions<'a>),
}

fn mixed_shader_source(field_source: &str) -> String {
    let analytic = include_str!("../shaders/analytic.wgsl")
        .split_once("// GENESIS_ANALYTIC_LIBRARY_END")
        .expect("analytic render entry")
        .0;
    let field = field_source
        .split_once("// GENESIS_EVALUATOR_BEGIN")
        .expect("field evaluator")
        .1
        .split_once("// GENESIS_EVALUATOR_END")
        .expect("field render entry")
        .0;
    let mixed =
        include_str!("../shaders/mixed.wgsl").replace("// FIELD_EVALUATOR_INSERTION_POINT", field);
    format!("{analytic}\n{mixed}")
}

fn compile_mixed_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    graph: &Graph,
) -> Result<CompiledArtifact, String> {
    let started = Instant::now();
    let field = specialized_field_source(graph)
        .ok_or_else(|| "definition exceeds finite GPU or shader source budget".to_owned())?;
    let source = mixed_shader_source(&field);
    let source_bytes = source.len();
    let source_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let oom_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("specialized native mathematical field"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("specialized mixed mathematical geometry"),
        layout: Some(layout),
        module: &module,
        entry_point: Some("render_mixed"),
        compilation_options: Default::default(),
        cache: None,
    });
    let validation = pollster::block_on(validation_scope.pop());
    let oom = pollster::block_on(oom_scope.pop());
    if let Some(error) = validation.or(oom) {
        Err(error.to_string())
    } else {
        Ok(CompiledArtifact {
            pipeline,
            source_ms,
            pipeline_ms: started.elapsed().as_secs_f64() * 1000.0,
            source_bytes,
        })
    }
}

fn material_index(material: crate::material::Material, materials: &mut Vec<GpuMaterial>) -> u32 {
    let gpu = GpuMaterial {
        values: material.gpu(),
    };
    if let Some(index) = materials.iter().position(|item| *item == gpu) {
        index as u32
    } else {
        let index = materials.len() as u32;
        materials.push(gpu);
        index
    }
}

/// GPU query result. GPU `f32` values are approximate and not error bounded.
#[derive(Clone, Copy, Debug)]
pub struct GpuResult {
    /// Stable snapshot identity.
    pub id: u32,
    /// Approximate world-space ray distance.
    pub distance: f32,
    /// Approximate outward normal.
    pub normal: [f32; 3],
}

/// One sampled compute-pass duration, available only with `TIMESTAMP_QUERY`.
pub struct GpuTimer {
    pub(crate) queries: wgpu::QuerySet,
    pub(crate) resolved: wgpu::Buffer,
    pub(crate) readback: wgpu::Buffer,
}

/// One frame's viewport and optional presentation or timing targets.
pub struct DrawOptions<'a> {
    /// Pixel width and height; both must be nonzero.
    pub size: [u32; 2],
    /// Show outward normals as RGB.
    pub normal_debug: bool,
    /// Surface view to present to, or `None` for offscreen rendering.
    pub surface: Option<&'a wgpu::TextureView>,
    /// Optional timestamp sampler; the device must support it.
    pub timer: Option<&'a GpuTimer>,
}

/// Native mixed viewport controls. Both geometry paths use the same normalized world ray.
pub struct MixedDrawOptions<'a> {
    /// Existing viewport, normal and presentation targets.
    pub draw: DrawOptions<'a>,
    /// Bounded field tracing policy.
    pub policy: GraphTracePolicy,
    /// Draw analytic primitives from the world snapshot.
    pub show_analytic: bool,
    /// Draw enabled world-owned Field Graph occurrences.
    pub show_fields: bool,
    /// Mark uncertain, exhausted and near-coincident depth explicitly.
    pub diagnostic: bool,
    /// Presentation-only per-entity material assignments.
    pub materials: Option<&'a MaterialSet>,
    /// Deterministic simulation time in seconds; never wall-clock time.
    pub simulation_seconds: f32,
    /// Linear RGB background, each component in `[0,1]`.
    pub background: [f32; 3],
    /// Nonzero world-space directional light vector.
    pub light_direction: [f32; 3],
}

/// Select the native Field Graph interpreter or allow revision-scoped specialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuFieldExecution {
    /// Interpret until a validated specialized pipeline for the current definition is ready.
    Auto,
    /// Always use the reference interpreter.
    Interpreter,
}

/// Native mixed-field execution and visual bounds controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuFieldRenderMode {
    /// Select automatic revision specialization or the reference interpreter.
    pub execution: GpuFieldExecution,
    /// Skip field tracing for rays outside the expanded visual field bounds.
    pub spatial_guard: bool,
}

impl Default for GpuFieldRenderMode {
    fn default() -> Self {
        Self {
            execution: GpuFieldExecution::Auto,
            spatial_guard: true,
        }
    }
}

/// Actual field execution selected for the most recent mixed draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuFieldPath {
    /// No compiled pipeline was selected.
    Interpreter,
    /// One exact definition revision was compiled and selected.
    Specialized,
}

/// Renderer-local compilation diagnostics. Pipelines never cross device ownership.
#[derive(Clone, Debug)]
pub struct GpuCompilerStats {
    /// Path used by the most recent mixed draw.
    pub active: GpuFieldPath,
    /// Successful pipeline entries currently resident.
    pub cached_pipelines: usize,
    /// One compilation may be in flight.
    pub pending: bool,
    /// Number of worker compilation requests.
    pub requests: u64,
    /// Number of ready-cache selections.
    pub cache_hits: u64,
    /// Number of failed compilation attempts.
    pub failures: u64,
    /// Last compiler failure, if any.
    pub last_error: Option<String>,
    /// Source generation time of the most recently selected successful pipeline.
    pub source_ms: Option<f64>,
    /// Shader validation and pipeline creation time for that pipeline, on the worker thread.
    pub pipeline_ms: Option<f64>,
    /// Generated mixed WGSL source bytes of that pipeline.
    pub source_bytes: Option<usize>,
}

/// Direct analytic traversal, GPU BVH traversal, or the measured-size policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuTraversal {
    /// Test every primitive; reference and fallback path.
    Direct,
    /// Traverse the flattened BVH, returning an error if preparation fails.
    Bvh,
    /// Use direct traversal for at most four primitives; otherwise try BVH and fall back to direct.
    Auto,
}
impl GpuTraversal {
    fn code(self) -> u32 {
        match self {
            Self::Direct => 0,
            Self::Bvh => 1,
            Self::Auto => 2,
        }
    }
}
impl GpuTimer {
    /// Returns `None` when the device was created without timestamp support.
    pub fn new(device: &wgpu::Device) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        Some(Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("compute timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolved: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("resolved timestamps"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp readback"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        })
    }
    /// Reads the latest sampled compute pass in milliseconds; call only after a draw with this timer.
    pub fn read_ms(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<f64, RenderError> {
        let slice = self.readback.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Gpu(e.to_string()))?
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let timestamps: &[u64] = bytemuck::cast_slice(&mapped);
        let duration = timestamps[1].saturating_sub(timestamps[0]) as f64
            * f64::from(queue.get_timestamp_period())
            / 1_000_000.0;
        drop(mapped);
        self.readback.unmap();
        Ok(duration)
    }
}

fn vec4(v: spatial_math::Vec3, w: f32) -> [f32; 4] {
    [v.x() as f32, v.y() as f32, v.z() as f32, w]
}
fn scene_data(scene: &Scene) -> Vec<GpuPrimitive> {
    scene
        .primitives()
        .iter()
        .map(|p| GpuPrimitive {
            center_kind: vec4(
                p.center,
                match p.kind {
                    crate::PrimitiveKind::Sphere => 0.0,
                    crate::PrimitiveKind::Box => 1.0,
                    crate::PrimitiveKind::Capsule => 2.0,
                },
            ),
            dimensions: vec4(p.dimensions, p.radius as f32),
            color: [
                p.color[0],
                p.color[1],
                p.color[2],
                if p.emphasis { 1.0 } else { 0.0 },
            ],
            identity: [p.id, 0, 0, 0],
        })
        .collect()
}
fn camera_data(
    camera: Camera,
    width: u32,
    height: u32,
    normal_debug: bool,
    object_count: usize,
    traversal: GpuTraversal,
) -> GpuCamera {
    let aspect = f64::from(width) / f64::from(height);
    let scale_x = camera.tan_half_fov * aspect;
    let scale_y = camera.tan_half_fov;
    GpuCamera {
        rows: [
            vec4(camera.origin, 0.0),
            vec4(camera.forward, 0.0),
            [
                camera.right.x() as f32 * scale_x as f32,
                camera.right.y() as f32 * scale_x as f32,
                camera.right.z() as f32 * scale_x as f32,
                0.0,
            ],
            [
                camera.up.x() as f32 * scale_y as f32,
                camera.up.y() as f32 * scale_y as f32,
                camera.up.z() as f32 * scale_y as f32,
                0.0,
            ],
            [
                width as f32,
                height as f32,
                camera.near as f32,
                camera.far as f32,
            ],
            [if normal_debug { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        ],
        scene: [object_count as u32, 0, 0, traversal.code()],
    }
}

/// Compute renderer and optional full-screen image presentation pipeline.
pub struct GpuRenderer {
    device: wgpu::Device,
    compute_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::ComputePipeline,
    query_pipeline: wgpu::ComputePipeline,
    mixed_layout: wgpu::BindGroupLayout,
    mixed_pipeline_layout: wgpu::PipelineLayout,
    mixed_pipeline: wgpu::ComputePipeline,
    present_layout: wgpu::BindGroupLayout,
    present_pipeline: wgpu::RenderPipeline,
    primitive_budget_bytes: u64,
    dummy_bvh: wgpu::Buffer,
    primitives: Mutex<Option<PrimitiveBuffer>>,
    fields: Mutex<Option<FieldBuffers>>,
    mixed_image: Mutex<Option<MixedImage>>,
    compiler: Mutex<CompilerCache>,
}

const MAX_COMPILED_PIPELINES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompilerKey {
    revision: DefinitionRevision,
    semantics: u32,
}

struct CompiledPipeline {
    key: CompilerKey,
    result: Result<CompiledArtifact, String>,
}

struct CompiledArtifact {
    pipeline: wgpu::ComputePipeline,
    source_ms: f64,
    pipeline_ms: f64,
    source_bytes: usize,
}

struct PendingCompilation {
    key: CompilerKey,
    receiver: mpsc::Receiver<Result<CompiledArtifact, String>>,
}

struct CompilerCache {
    entries: VecDeque<CompiledPipeline>,
    pending: Option<PendingCompilation>,
    active: GpuFieldPath,
    requests: u64,
    cache_hits: u64,
    failures: u64,
    last_error: Option<String>,
    source_ms: Option<f64>,
    pipeline_ms: Option<f64>,
    source_bytes: Option<usize>,
}

impl Default for CompilerCache {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            pending: None,
            active: GpuFieldPath::Interpreter,
            requests: 0,
            cache_hits: 0,
            failures: 0,
            last_error: None,
            source_ms: None,
            pipeline_ms: None,
            source_bytes: None,
        }
    }
}

impl CompilerCache {
    fn insert(&mut self, key: CompilerKey, result: Result<CompiledArtifact, String>) {
        if let Err(error) = &result {
            self.failures = self.failures.saturating_add(1);
            self.last_error = Some(error.clone());
        } else {
            self.last_error = None;
        }
        self.entries.push_back(CompiledPipeline { key, result });
        if self.entries.len() > MAX_COMPILED_PIPELINES {
            self.entries.pop_front();
        }
    }
}
struct PrimitiveBuffer {
    buffer: wgpu::Buffer,
    capacity_bytes: u64,
    data: Vec<GpuPrimitive>,
    bvh: Option<BvhBuffer>,
    bvh_dirty: bool,
    bvh_rebuilds: u64,
    last_prepare: GpuUploadStats,
}
struct BvhBuffer {
    buffer: wgpu::Buffer,
    capacity_bytes: u64,
    node_count: u32,
    depth: usize,
}
/// Current resident GPU acceleration buffer, if one has been prepared.
#[derive(Clone, Copy, Debug)]
pub struct GpuAccelerationStats {
    /// Flattened node count.
    pub node_count: u32,
    /// CPU builder depth.
    pub depth: usize,
    /// Allocated BVH buffer bytes, including spare capacity.
    pub allocated_bytes: u64,
    /// Number of BVH rebuilds since this renderer allocated its current primitive buffer.
    pub rebuilds: u64,
}
/// Resident field/material buffer updates since the current cache allocation.
#[derive(Clone, Copy, Debug)]
pub struct GpuFieldCacheStats {
    /// Number of definition-node uploads.
    pub node_uploads: u64,
    /// Number of material-record uploads.
    pub material_uploads: u64,
    /// Active unique definition bytes.
    pub node_bytes: u64,
}
/// CPU-side cost of queuing the most recent scene upload; not GPU transfer time.
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuUploadStats {
    /// Primitive record bytes queued for transfer.
    pub primitive_bytes: u64,
    /// BVH node bytes queued for transfer.
    pub bvh_bytes: u64,
    /// CPU time spent in `queue.write_buffer` calls, in milliseconds.
    pub enqueue_ms: f64,
    /// CPU BVH build and flatten time, in milliseconds.
    pub bvh_build_ms: f64,
}
impl GpuRenderer {
    /// Inspect bounded, renderer-local mathematical shader compilation.
    pub fn compiler_stats(&self) -> Option<GpuCompilerStats> {
        let state = self.compiler.lock().ok()?;
        Some(GpuCompilerStats {
            active: state.active,
            cached_pipelines: state
                .entries
                .iter()
                .filter(|entry| entry.result.is_ok())
                .count(),
            pending: state.pending.is_some(),
            requests: state.requests,
            cache_hits: state.cache_hits,
            failures: state.failures,
            last_error: state.last_error.clone(),
            source_ms: state.source_ms,
            pipeline_ms: state.pipeline_ms,
            source_bytes: state.source_bytes,
        })
    }

    fn compiled_pipeline(
        &self,
        device: &wgpu::Device,
        world: &WorldState,
        enabled: bool,
    ) -> Result<Option<wgpu::ComputePipeline>, RenderError> {
        if device != &self.device {
            return Err(RenderError::InvalidInput("renderer device changed"));
        }
        let mut state = self
            .compiler
            .lock()
            .map_err(|_| RenderError::Gpu("compiler cache lock poisoned".into()))?;
        state.active = GpuFieldPath::Interpreter;
        let finished =
            state
                .pending
                .as_ref()
                .and_then(|pending| match pending.receiver.try_recv() {
                    Ok(result) => Some(Ok(result)),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("compilation worker disconnected".to_owned()))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                });
        if let Some(finished) = finished {
            let pending = state.pending.take().expect("completed pending request");
            state.insert(pending.key, finished.unwrap_or_else(Err));
        }
        if !enabled {
            return Ok(None);
        }
        let mut selected = None;
        for instance in world.field_instances().iter().filter(|item| item.enabled()) {
            if selected.is_some_and(|revision| revision != instance.revision()) {
                return Ok(None);
            }
            selected = Some(instance.revision());
        }
        let Some(revision) = selected else {
            return Ok(None);
        };
        let key = CompilerKey {
            revision,
            semantics: SEMANTICS_VERSION,
        };
        if let Some(index) = state.entries.iter().position(|entry| entry.key == key) {
            let entry = state.entries.remove(index).expect("found entry");
            let pipeline = entry.result.as_ref().ok().map(|artifact| {
                state.source_ms = Some(artifact.source_ms);
                state.pipeline_ms = Some(artifact.pipeline_ms);
                state.source_bytes = Some(artifact.source_bytes);
                artifact.pipeline.clone()
            });
            state.entries.push_back(entry);
            if pipeline.is_some() {
                state.cache_hits = state.cache_hits.saturating_add(1);
                state.active = GpuFieldPath::Specialized;
            }
            return Ok(pipeline);
        }
        if state.pending.is_none() {
            let graph = world
                .field_instances()
                .iter()
                .find(|item| item.enabled() && item.revision() == revision)
                .and_then(|item| world.field_graph(item.id()))
                .ok_or(RenderError::InvalidInput("missing field graph"))?
                .clone();
            let device = device.clone();
            let layout = self.mixed_pipeline_layout.clone();
            let (sender, receiver) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("genesis-field-compiler".into())
                .spawn(move || {
                    let result = compile_mixed_pipeline(&device, &layout, &graph);
                    let _ = sender.send(result);
                }) {
                Ok(_) => {
                    state.requests = state.requests.saturating_add(1);
                    state.pending = Some(PendingCompilation { key, receiver });
                }
                Err(error) => state.insert(key, Err(error.to_string())),
            }
        }
        Ok(None)
    }
    /// Inspect whether presentation-only edits avoided graph node uploads.
    pub fn field_cache_stats(&self) -> Option<GpuFieldCacheStats> {
        let cached = self.fields.lock().ok()?;
        let fields = cached.as_ref()?;
        Some(GpuFieldCacheStats {
            node_uploads: fields.node_uploads,
            material_uploads: fields.material_uploads,
            node_bytes: (fields.nodes.len() * std::mem::size_of::<GpuNode>()) as u64,
        })
    }
    /// Uses a 16 MiB primitive-storage budget. This excludes render targets and query buffers.
    pub async fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, RenderError> {
        Self::with_primitive_budget(device, surface_format, 16 * 1024 * 1024).await
    }
    /// Compiles and validates both WGSL pipelines. The surface format is used only for presentation.
    /// `primitive_budget_bytes` limits the resident primitive buffer, including spare capacity.
    pub async fn with_primitive_budget(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        primitive_budget_bytes: u64,
    ) -> Result<Self, RenderError> {
        if primitive_budget_bytes < std::mem::size_of::<GpuPrimitive>() as u64 {
            return Err(RenderError::InvalidInput(
                "primitive budget below one GPU record",
            ));
        }
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("analytic intersections"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/analytic.wgsl").into()),
        });
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("analytic compute resources"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("analytic compute pipeline"),
            bind_group_layouts: &[Some(&compute_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("render analytic primitives"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("render"),
            compilation_options: Default::default(),
            cache: None,
        });
        let query_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("query analytic primitives"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("query"),
            compilation_options: Default::default(),
            cache: None,
        });
        let mixed_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mixed field resources"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let mixed_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native mixed analytic and field rendering"),
            source: wgpu::ShaderSource::Wgsl(
                mixed_shader_source(include_str!("../shaders/field_graph.wgsl")).into(),
            ),
        });
        let mixed_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mixed compute pipeline"),
                bind_group_layouts: &[Some(&compute_layout), Some(&mixed_layout)],
                immediate_size: 0,
            });
        let mixed_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("render mixed mathematical geometry"),
            layout: Some(&mixed_pipeline_layout),
            module: &mixed_shader,
            entry_point: Some("render_mixed"),
            compilation_options: Default::default(),
            cache: None,
        });
        let present_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present image"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/present.wgsl").into()),
        });
        let present_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present image layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let present_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("present pipeline"),
                bind_group_layouts: &[Some(&present_layout)],
                immediate_size: 0,
            });
        let present_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen image presentation"),
            layout: Some(&present_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &present_shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &present_shader,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = error_scope.pop().await {
            return Err(RenderError::Gpu(error.to_string()));
        }
        let dummy_bvh = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("empty analytic BVH"),
            contents: bytemuck::bytes_of(&FlatNode::zeroed()),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Ok(Self {
            device: device.clone(),
            compute_layout,
            render_pipeline,
            query_pipeline,
            mixed_layout,
            mixed_pipeline_layout,
            mixed_pipeline,
            present_layout,
            present_pipeline,
            primitive_budget_bytes,
            dummy_bvh,
            primitives: Mutex::new(None),
            fields: Mutex::new(None),
            mixed_image: Mutex::new(None),
            compiler: Mutex::new(CompilerCache::default()),
        })
    }

    fn image(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("analytic color image"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }
    /// Returns current GPU BVH allocation information; `None` before acceleration is prepared.
    pub fn acceleration_stats(&self) -> Option<GpuAccelerationStats> {
        let cached = self.primitives.lock().ok()?;
        let scene = cached.as_ref()?;
        let bvh = scene.bvh.as_ref()?;
        Some(GpuAccelerationStats {
            node_count: bvh.node_count,
            depth: bvh.depth,
            allocated_bytes: bvh.capacity_bytes,
            rebuilds: scene.bvh_rebuilds,
        })
    }
    /// Returns the latest CPU enqueue measurements; GPU copy execution is not timed.
    pub fn last_upload_stats(&self) -> Option<GpuUploadStats> {
        Some(self.primitives.lock().ok()?.as_ref()?.last_prepare)
    }
    fn prepare_bvh(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        cached: &mut PrimitiveBuffer,
    ) -> Result<GpuUploadStats, RenderError> {
        let build_started = Instant::now();
        let bvh = Bvh::build(scene)?;
        let nodes = bvh.flat_nodes()?;
        let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
        let requested_bytes = u64::try_from(nodes.len().max(1))
            .ok()
            .and_then(|count| count.checked_mul(std::mem::size_of::<FlatNode>() as u64))
            .ok_or(RenderError::TooManyObjects)?;
        let allowed_bytes = self
            .primitive_budget_bytes
            .min(device.limits().max_storage_buffer_binding_size)
            .min(device.limits().max_buffer_size);
        if requested_bytes > allowed_bytes {
            return Err(RenderError::AccelerationCapacity {
                requested_bytes,
                allowed_bytes,
            });
        }
        if cached
            .bvh
            .as_ref()
            .is_none_or(|old| requested_bytes > old.capacity_bytes)
        {
            let capacity_bytes = requested_bytes.next_power_of_two().min(allowed_bytes);
            let error_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("analytic BVH nodes"),
                size: capacity_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if let Some(error) = pollster::block_on(error_scope.pop()) {
                return Err(RenderError::Gpu(format!("BVH allocation failed: {error}")));
            }
            cached.bvh = Some(BvhBuffer {
                buffer,
                capacity_bytes,
                node_count: 0,
                depth: 0,
            });
        }
        let storage = cached.bvh.as_mut().expect("created above");
        let upload_started = Instant::now();
        queue.write_buffer(&storage.buffer, 0, bytemuck::cast_slice(&nodes));
        let enqueue_ms = upload_started.elapsed().as_secs_f64() * 1000.0;
        storage.node_count = u32::try_from(nodes.len()).map_err(|_| RenderError::TooManyObjects)?;
        storage.depth = bvh.depth();
        cached.bvh_dirty = false;
        cached.bvh_rebuilds = cached.bvh_rebuilds.saturating_add(1);
        Ok(GpuUploadStats {
            primitive_bytes: 0,
            bvh_bytes: requested_bytes,
            enqueue_ms,
            bvh_build_ms: build_ms,
        })
    }
    fn binding(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        mut camera: GpuCamera,
        image: &wgpu::TextureView,
        rays: &[GpuRay],
    ) -> Result<(wgpu::BindGroup, wgpu::Buffer), RenderError> {
        let record_bytes = std::mem::size_of::<GpuPrimitive>() as u64;
        let requested_bytes = u64::try_from(scene.primitives().len().max(1))
            .ok()
            .and_then(|count| count.checked_mul(record_bytes))
            .ok_or(RenderError::TooManyObjects)?;
        let allowed_bytes = self
            .primitive_budget_bytes
            .min(device.limits().max_storage_buffer_binding_size)
            .min(device.limits().max_buffer_size);
        if requested_bytes > allowed_bytes {
            return Err(RenderError::PrimitiveCapacity {
                requested_bytes,
                allowed_bytes,
            });
        }
        let objects = scene_data(scene);
        let mut cached = self
            .primitives
            .lock()
            .map_err(|_| RenderError::Gpu("primitive buffer lock poisoned".into()))?;
        let replace = cached
            .as_ref()
            .is_none_or(|old| requested_bytes > old.capacity_bytes);
        if replace {
            let capacity_bytes = requested_bytes.next_power_of_two().min(allowed_bytes);
            let error_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("analytic primitives"),
                size: capacity_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if let Some(error) = pollster::block_on(error_scope.pop()) {
                return Err(RenderError::Gpu(format!(
                    "primitive allocation failed: {error}"
                )));
            }
            *cached = Some(PrimitiveBuffer {
                buffer,
                capacity_bytes,
                data: Vec::new(),
                bvh: None,
                bvh_dirty: true,
                bvh_rebuilds: 0,
                last_prepare: GpuUploadStats::default(),
            });
        }
        let primitive_buffer = cached.as_mut().expect("created above");
        let mut upload = GpuUploadStats::default();
        if replace || primitive_buffer.data != objects {
            let upload_started = Instant::now();
            if objects.is_empty() {
                queue.write_buffer(
                    &primitive_buffer.buffer,
                    0,
                    bytemuck::bytes_of(&GpuPrimitive::zeroed()),
                );
            } else {
                queue.write_buffer(&primitive_buffer.buffer, 0, bytemuck::cast_slice(&objects));
            }
            upload.primitive_bytes = requested_bytes;
            upload.enqueue_ms = upload_started.elapsed().as_secs_f64() * 1000.0;
            primitive_buffer.data = objects;
            primitive_buffer.bvh_dirty = true;
        }
        let requested_mode = camera.scene[3];
        let want_bvh = requested_mode == 1 || (requested_mode == 2 && scene.primitives().len() > 4);
        camera.scene[3] = 0;
        if want_bvh && !scene.primitives().is_empty() {
            if primitive_buffer.bvh_dirty || primitive_buffer.bvh.is_none() {
                match self.prepare_bvh(device, queue, scene, primitive_buffer) {
                    Ok(bvh_upload) => {
                        upload.bvh_bytes = bvh_upload.bvh_bytes;
                        upload.enqueue_ms += bvh_upload.enqueue_ms;
                        upload.bvh_build_ms = bvh_upload.bvh_build_ms;
                    }
                    Err(
                        RenderError::AccelerationCapacity { .. }
                        | RenderError::TooManyObjects
                        | RenderError::Gpu(_),
                    ) if requested_mode == 2 => {}
                    Err(error) => return Err(error),
                }
            }
            if !primitive_buffer.bvh_dirty {
                camera.scene[1] = primitive_buffer
                    .bvh
                    .as_ref()
                    .expect("prepared above")
                    .node_count;
                camera.scene[3] = 1;
            }
        }
        primitive_buffer.last_prepare = upload;
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera uniform"),
            contents: bytemuck::bytes_of(&camera),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let empty_ray = GpuRay::zeroed();
        let ray_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("query rays"),
            contents: if rays.is_empty() {
                bytemuck::bytes_of(&empty_ray)
            } else {
                bytemuck::cast_slice(rays)
            },
            usage: wgpu::BufferUsages::STORAGE,
        });
        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("query results"),
            size: (rays.len().max(1) * std::mem::size_of::<GpuHit>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("analytic resources"),
            layout: &self.compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: primitive_buffer.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(image),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: ray_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: result_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: primitive_buffer
                        .bvh
                        .as_ref()
                        .map_or(&self.dummy_bvh, |bvh| &bvh.buffer)
                        .as_entire_binding(),
                },
            ],
        });
        Ok((group, result_buffer))
    }

    fn mixed_binding(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &WorldState,
        scene: &Scene,
        options: MixedSceneOptions<'_>,
    ) -> Result<wgpu::BindGroup, RenderError> {
        options.policy.validate()?;
        if !options.simulation_seconds.is_finite()
            || !(0.0..=1_000_000.0).contains(&options.simulation_seconds)
        {
            return Err(RenderError::InvalidInput(
                "invalid material simulation time",
            ));
        }
        let light_length_squared = options.light_direction.iter().map(|x| x * x).sum::<f32>();
        if options
            .background
            .iter()
            .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
            || options.light_direction.iter().any(|x| !x.is_finite())
            || !light_length_squared.is_finite()
            || light_length_squared < 1e-12
        {
            return Err(RenderError::InvalidInput(
                "invalid mixed background or light",
            ));
        }
        if let Some(materials) = options.materials {
            materials.validate_world(world)?;
        }
        world
            .validate()
            .map_err(|_| RenderError::InvalidInput("invalid world"))?;
        let mut nodes = Vec::new();
        let mut instances = Vec::new();
        let mut definitions = Vec::new();
        let mut materials = Vec::new();
        let analytic_indices: Vec<u32> = scene
            .primitives()
            .iter()
            .map(|primitive| {
                if let (Some(assignments), Some(SemanticTarget::Sphere(id))) =
                    (options.materials, primitive.target())
                {
                    assignments
                        .sphere(id)
                        .map(|material| material_index(material, &mut materials))
                        .unwrap_or(u32::MAX)
                } else {
                    u32::MAX
                }
            })
            .collect();
        for instance in world.field_instances().iter().filter(|item| item.enabled()) {
            let graph = world
                .field_graph(instance.id())
                .ok_or(RenderError::InvalidInput("missing field graph"))?;
            let offset = if let Some((_, offset)) = definitions
                .iter()
                .find(|(revision, _)| *revision == instance.revision())
            {
                *offset
            } else {
                let offset = u32::try_from(nodes.len()).map_err(|_| RenderError::TooManyObjects)?;
                let mut graph_nodes = gpu_nodes(graph);
                for node in &mut graph_nodes {
                    if (2..=5).contains(&node.links[0]) {
                        node.links[1] = node.links[1]
                            .checked_add(offset)
                            .ok_or(RenderError::TooManyObjects)?;
                    }
                    if (4..=5).contains(&node.links[0]) {
                        node.links[2] = node.links[2]
                            .checked_add(offset)
                            .ok_or(RenderError::TooManyObjects)?;
                    }
                }
                nodes.extend(graph_nodes);
                definitions.push((instance.revision(), offset));
                offset
            };
            let t = instance.transform();
            let id = instance.id().value();
            let material = options
                .materials
                .and_then(|assignments| assignments.field(instance.id()))
                .map(|material| material_index(material, &mut materials))
                .unwrap_or(u32::MAX);
            let bounds = graph.bounds();
            let lower = [bounds.min.x(), bounds.min.y(), bounds.min.z()];
            let upper = [bounds.max.x(), bounds.max.y(), bounds.max.z()];
            // Include the largest hit tolerance and slack for supported f32 ray/graph arithmetic.
            let padding = |axis: usize| {
                0.25_f32 + 0.001 * (lower[axis].abs().max(upper[axis].abs()) as f32).max(1.0)
            };
            instances.push(GpuFieldInstance {
                position_scale: vec4(t.translation(), t.scale() as f32),
                info: [
                    graph
                        .root()
                        .checked_add(offset)
                        .ok_or(RenderError::TooManyObjects)?,
                    id as u32,
                    (id >> 32) as u32,
                    material,
                ],
                bounds_min: std::array::from_fn(|axis| {
                    if axis == 3 {
                        0.0
                    } else {
                        (lower[axis] as f32 - padding(axis)).next_down()
                    }
                }),
                bounds_max: std::array::from_fn(|axis| {
                    if axis == 3 {
                        0.0
                    } else {
                        (upper[axis] as f32 + padding(axis)).next_up()
                    }
                }),
            });
        }
        let node_count = u32::try_from(nodes.len()).map_err(|_| RenderError::TooManyObjects)?;
        let node_bytes = (nodes.len().max(1) * std::mem::size_of::<GpuNode>()) as u64;
        let instance_bytes =
            (instances.len().max(1) * std::mem::size_of::<GpuFieldInstance>()) as u64;
        let material_bytes = (72 * std::mem::size_of::<GpuMaterial>()) as u64;
        let analytic_index_bytes =
            (analytic_indices.len().max(1) * std::mem::size_of::<u32>()) as u64;
        let limit = device
            .limits()
            .max_storage_buffer_binding_size
            .min(device.limits().max_buffer_size);
        if [
            node_bytes,
            instance_bytes,
            material_bytes,
            analytic_index_bytes,
        ]
        .into_iter()
        .any(|bytes| bytes > limit)
        {
            return Err(RenderError::AccelerationCapacity {
                requested_bytes: node_bytes
                    .max(instance_bytes)
                    .max(material_bytes)
                    .max(analytic_index_bytes),
                allowed_bytes: limit,
            });
        }
        let mut cached = self
            .fields
            .lock()
            .map_err(|_| RenderError::Gpu("field buffer lock poisoned".into()))?;
        let replace = cached.as_ref().is_none_or(|old| {
            old.node_capacity < node_bytes
                || old.instance_capacity < instance_bytes
                || old.material_capacity < material_bytes
                || old.analytic_index_capacity < analytic_index_bytes
        });
        if replace {
            let node_capacity = node_bytes.next_power_of_two().min(limit);
            let instance_capacity = instance_bytes.next_power_of_two().min(limit);
            let material_capacity = material_bytes.next_power_of_two().min(limit);
            let analytic_index_capacity = analytic_index_bytes.next_power_of_two().min(limit);
            *cached = Some(FieldBuffers {
                nodes: Vec::new(),
                instances: Vec::new(),
                node_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("resident field graph nodes"),
                    size: node_capacity,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                instance_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("resident field instances"),
                    size: instance_capacity,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                materials: Vec::new(),
                analytic_indices: Vec::new(),
                material_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("resident surface materials"),
                    size: material_capacity,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                analytic_index_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("resident analytic material indices"),
                    size: analytic_index_capacity,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                node_capacity,
                instance_capacity,
                material_capacity,
                analytic_index_capacity,
                node_uploads: 0,
                material_uploads: 0,
            });
        }
        let storage = cached.as_mut().expect("allocated above");
        if replace || storage.nodes != nodes {
            let dummy = GpuNode::zeroed();
            queue.write_buffer(
                &storage.node_buffer,
                0,
                if nodes.is_empty() {
                    bytemuck::bytes_of(&dummy)
                } else {
                    bytemuck::cast_slice(&nodes)
                },
            );
            storage.nodes = nodes;
            storage.node_uploads += 1;
        }
        if replace || storage.instances != instances {
            let dummy = GpuFieldInstance::zeroed();
            queue.write_buffer(
                &storage.instance_buffer,
                0,
                if instances.is_empty() {
                    bytemuck::bytes_of(&dummy)
                } else {
                    bytemuck::cast_slice(&instances)
                },
            );
            storage.instances = instances;
        }
        if replace || storage.materials != materials {
            let dummy = GpuMaterial::zeroed();
            queue.write_buffer(
                &storage.material_buffer,
                0,
                if materials.is_empty() {
                    bytemuck::bytes_of(&dummy)
                } else {
                    bytemuck::cast_slice(&materials)
                },
            );
            storage.materials = materials;
            storage.material_uploads += 1;
        }
        if replace || storage.analytic_indices != analytic_indices {
            let dummy = u32::MAX;
            queue.write_buffer(
                &storage.analytic_index_buffer,
                0,
                if analytic_indices.is_empty() {
                    bytemuck::bytes_of(&dummy)
                } else {
                    bytemuck::cast_slice(&analytic_indices)
                },
            );
            storage.analytic_indices = analytic_indices;
        }
        let options = GpuMixedOptions {
            trace: [
                options.policy.tolerance,
                options.policy.safety,
                (options.policy.tolerance * 0.5).max(0.0001),
                options.simulation_seconds,
            ],
            info: [
                storage.instances.len() as u32,
                node_count,
                u32::from(options.flags[0])
                    | (u32::from(options.flags[1]) << 1)
                    | (u32::from(options.flags[2]) << 2)
                    | (u32::from(options.render_mode.spatial_guard) << 3),
                options.policy.iterations,
            ],
            background: [
                options.background[0],
                options.background[1],
                options.background[2],
                0.0,
            ],
            light: [
                options.light_direction[0],
                options.light_direction[1],
                options.light_direction[2],
                0.0,
            ],
        };
        let options_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mixed render options"),
            contents: bytemuck::bytes_of(&options),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mixed world fields"),
            layout: &self.mixed_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: storage.node_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: storage.instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: options_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: storage.material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: storage.analytic_index_buffer.as_entire_binding(),
                },
            ],
        }))
    }

    /// Computes an offscreen RGBA8 image and optionally presents it to a surface view.
    /// Zero-sized windows should skip this call until resized.
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        camera: Camera,
        options: DrawOptions<'_>,
    ) -> Result<wgpu::Texture, RenderError> {
        self.draw_with_traversal(device, queue, scene, camera, GpuTraversal::Auto, options)
    }
    /// Draws with an explicit direct, BVH, or automatic traversal policy.
    pub fn draw_with_traversal(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        camera: Camera,
        traversal: GpuTraversal,
        options: DrawOptions<'_>,
    ) -> Result<wgpu::Texture, RenderError> {
        self.draw_internal(
            device,
            queue,
            scene,
            camera,
            options,
            DrawMode::Analytic(traversal),
        )
    }

    /// Render analytic and world-owned implicit geometry directly to a GPU image.
    /// No readback is performed; uncertainty uses visible warning colors.
    pub fn draw_mixed(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &WorldState,
        camera: Camera,
        options: MixedDrawOptions<'_>,
    ) -> Result<wgpu::Texture, RenderError> {
        self.draw_mixed_with_execution(
            device,
            queue,
            world,
            camera,
            options,
            GpuFieldRenderMode::default(),
        )
    }

    /// Explicit execution and visual bounds policy for differential testing.
    /// Compilation is requested on one worker; this call does not wait for its completion.
    pub fn draw_mixed_with_execution(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &WorldState,
        camera: Camera,
        options: MixedDrawOptions<'_>,
        render_mode: GpuFieldRenderMode,
    ) -> Result<wgpu::Texture, RenderError> {
        let scene = Scene::from_world(world)?;
        self.draw_internal(
            device,
            queue,
            &scene,
            camera,
            options.draw,
            DrawMode::Mixed(
                world,
                MixedSceneOptions {
                    policy: options.policy,
                    flags: [
                        options.show_analytic,
                        options.show_fields,
                        options.diagnostic,
                    ],
                    materials: options.materials,
                    simulation_seconds: options.simulation_seconds,
                    background: options.background,
                    light_direction: options.light_direction,
                    render_mode,
                },
            ),
        )
    }

    fn draw_internal(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        camera: Camera,
        options: DrawOptions<'_>,
        mode: DrawMode<'_>,
    ) -> Result<wgpu::Texture, RenderError> {
        let [width, height] = options.size;
        if width == 0
            || height == 0
            || width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(RenderError::InvalidInput("invalid render resolution"));
        }
        let image = if matches!(mode, DrawMode::Mixed(..)) {
            let mut cached = self
                .mixed_image
                .lock()
                .map_err(|_| RenderError::Gpu("mixed image lock poisoned".into()))?;
            if cached
                .as_ref()
                .is_none_or(|item| item.size != [width, height])
            {
                *cached = Some(MixedImage {
                    size: [width, height],
                    texture: Self::image(device, width, height),
                });
            }
            cached.as_ref().expect("allocated above").texture.clone()
        } else {
            Self::image(device, width, height)
        };
        let image_view = image.create_view(&wgpu::TextureViewDescriptor::default());
        let (group, _) = self.binding(
            device,
            queue,
            scene,
            camera_data(
                camera,
                width,
                height,
                options.normal_debug,
                scene.primitives().len(),
                match mode {
                    DrawMode::Analytic(traversal) => traversal,
                    DrawMode::Mixed(..) => GpuTraversal::Auto,
                },
            ),
            &image_view,
            &[],
        )?;
        let field_group = match mode {
            DrawMode::Analytic(..) => None,
            DrawMode::Mixed(world, mixed_options) => {
                Some(self.mixed_binding(device, queue, world, scene, mixed_options)?)
            }
        };
        let compiled_pipeline = match mode {
            DrawMode::Mixed(world, settings) => self.compiled_pipeline(
                device,
                world,
                settings.flags[1] && settings.render_mode.execution == GpuFieldExecution::Auto,
            )?,
            DrawMode::Analytic(..) => None,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("analytic frame"),
        });
        {
            let timestamps = options.timer.map(|timer| wgpu::ComputePassTimestampWrites {
                query_set: &timer.queries,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("analytic intersections"),
                timestamp_writes: timestamps,
            });
            pass.set_pipeline(if field_group.is_some() {
                compiled_pipeline.as_ref().unwrap_or(&self.mixed_pipeline)
            } else {
                &self.render_pipeline
            });
            pass.set_bind_group(0, &group, &[]);
            if let Some(field_group) = &field_group {
                pass.set_bind_group(1, field_group, &[]);
            }
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        if let Some(timer) = options.timer {
            encoder.resolve_query_set(&timer.queries, 0..2, &timer.resolved, 0);
            encoder.copy_buffer_to_buffer(&timer.resolved, 0, &timer.readback, 0, 16);
        }
        if let Some(surface_view) = options.surface {
            let present_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("present image"),
                layout: &self.present_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&image_view),
                }],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("image presentation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.present_pipeline);
            pass.set_bind_group(0, &present_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        Ok(image)
    }

    /// Runs the same WGSL analytic intersection routines on explicit rays and reads results back.
    /// Intended for diagnostics and tests, not every display frame.
    pub fn query(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        rays: &[Ray],
    ) -> Result<Vec<Option<GpuResult>>, RenderError> {
        self.query_with_traversal(device, queue, scene, rays, GpuTraversal::Direct)
    }
    /// Queries explicit rays through the selected path; direct traversal remains the reference.
    pub fn query_with_traversal(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        rays: &[Ray],
        traversal: GpuTraversal,
    ) -> Result<Vec<Option<GpuResult>>, RenderError> {
        if rays.is_empty() {
            return Ok(Vec::new());
        }
        if rays.len() > 65_536 {
            return Err(RenderError::InvalidInput("too many GPU query rays"));
        }
        let input: Vec<GpuRay> = rays
            .iter()
            .map(|r| GpuRay {
                origin_near: vec4(r.origin, r.near as f32),
                direction_far: vec4(r.direction, r.far as f32),
            })
            .collect();
        let image = Self::image(device, 1, 1);
        let image_view = image.create_view(&wgpu::TextureViewDescriptor::default());
        let mut query_camera = GpuCamera::zeroed();
        query_camera.scene[0] = scene.primitives().len() as u32;
        query_camera.scene[3] = traversal.code();
        let (group, result_buffer) =
            self.binding(device, queue, scene, query_camera, &image_view, &input)?;
        let size = (rays.len() * std::mem::size_of::<GpuHit>()) as u64;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("query readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("analytic query"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("analytic query pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.query_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((rays.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&result_buffer, 0, &readback, 0, size);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Gpu(e.to_string()))?
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let output: Vec<_> = bytemuck::cast_slice::<u8, GpuHit>(&mapped)
            .iter()
            .map(|hit| {
                if hit.meta[0] == 0 {
                    None
                } else {
                    Some(GpuResult {
                        id: hit.meta[1],
                        distance: hit.distance_normal[3],
                        normal: [
                            hit.distance_normal[0],
                            hit.distance_normal[1],
                            hit.distance_normal[2],
                        ],
                    })
                }
            })
            .collect();
        drop(mapped);
        readback.unmap();
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_layout_and_shader_validation() {
        assert_eq!(std::mem::size_of::<GpuCamera>(), 112);
        assert_eq!(std::mem::offset_of!(GpuCamera, scene), 96);
        assert_eq!(std::mem::size_of::<GpuPrimitive>(), 64);
        assert_eq!(std::mem::offset_of!(GpuPrimitive, center_kind), 0);
        assert_eq!(std::mem::offset_of!(GpuPrimitive, dimensions), 16);
        assert_eq!(std::mem::offset_of!(GpuPrimitive, color), 32);
        assert_eq!(std::mem::offset_of!(GpuPrimitive, identity), 48);
        assert_eq!(std::mem::size_of::<GpuRay>(), 32);
        assert_eq!(std::mem::size_of::<GpuHit>(), 32);
        assert_eq!(std::mem::offset_of!(GpuHit, meta), 16);
        assert_eq!(std::mem::size_of::<FlatNode>(), 64);
        assert_eq!(std::mem::offset_of!(FlatNode, min), 0);
        assert_eq!(std::mem::offset_of!(FlatNode, max), 16);
        assert_eq!(std::mem::offset_of!(FlatNode, meta), 32);
        assert_eq!(std::mem::offset_of!(FlatNode, leaf), 48);
        assert_eq!(std::mem::size_of::<GpuFieldInstance>(), 64);
        assert_eq!(std::mem::offset_of!(GpuFieldInstance, position_scale), 0);
        assert_eq!(std::mem::offset_of!(GpuFieldInstance, info), 16);
        assert_eq!(std::mem::offset_of!(GpuFieldInstance, bounds_min), 32);
        assert_eq!(std::mem::offset_of!(GpuFieldInstance, bounds_max), 48);
        assert_eq!(std::mem::size_of::<GpuMixedOptions>(), 64);
        for shader in [
            include_str!("../shaders/analytic.wgsl"),
            include_str!("../shaders/present.wgsl"),
            &mixed_shader_source(include_str!("../shaders/field_graph.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(shader).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
        let graph = world_authoring::field_graph::compile_json(include_bytes!(
            "../../../examples/field-graph/definitions/balanced-16.json"
        ))
        .unwrap();
        let field = specialized_field_source(&graph).unwrap();
        let source = mixed_shader_source(&field);
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn compiled_cache_is_bounded_and_revision_scoped() {
        let mut cache = CompilerCache::default();
        for index in 0..=MAX_COMPILED_PIPELINES {
            let key = CompilerKey {
                revision: DefinitionRevision::of(&[index as u8]),
                semantics: SEMANTICS_VERSION,
            };
            cache.insert(key, Err("test failure".into()));
        }
        assert_eq!(cache.entries.len(), MAX_COMPILED_PIPELINES);
        assert_eq!(cache.failures, (MAX_COMPILED_PIPELINES + 1) as u64);
        assert_eq!(
            cache.entries.front().unwrap().key.revision,
            DefinitionRevision::of(&[1])
        );
        assert_ne!(
            cache.entries.front().unwrap().key,
            CompilerKey {
                revision: DefinitionRevision::of(&[1]),
                semantics: SEMANTICS_VERSION + 1,
            }
        );
    }
}
