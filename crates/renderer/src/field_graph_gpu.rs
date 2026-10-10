//! Direct WGSL interpretation of the bounded authored field graph.

use crate::{Camera, GpuTimer, Ray, RenderError};
use bytemuck::{Pod, Zeroable};
use std::{sync::mpsc, time::Instant};
use wgpu::util::DeviceExt;
use world_authoring::field_graph::{Graph, Node};
use world_state::{EntityId, WorldState};

const MAX_QUERY_RAYS: usize = 65_536;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuConfig {
    origin: [f32; 4],
    forward: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    view: [f32; 4],
    trace: [f32; 4],
    meta: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct GpuNode {
    pub(crate) links: [u32; 4],
    pub(crate) param: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRay {
    origin_near: [f32; 4],
    direction_far: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuOutcome {
    distance_normal: [f32; 4],
    meta: [u32; 4],
}

/// Bounded GPU sphere-tracing policy. All values are world units except counts and safety.
#[derive(Clone, Copy, Debug)]
pub struct GraphTracePolicy {
    /// Approximate positive hit threshold (0.00001..=0.1).
    pub tolerance: f32,
    /// Fraction of the ideal Lipschitz step (0, 1].
    pub safety: f32,
    /// Maximum field samples (1..=512).
    pub iterations: u32,
}
impl Default for GraphTracePolicy {
    fn default() -> Self {
        Self {
            tolerance: 0.001,
            safety: 0.8,
            iterations: 128,
        }
    }
}
impl GraphTracePolicy {
    pub(crate) fn validate(self) -> Result<(), RenderError> {
        if !self.tolerance.is_finite()
            || !self.safety.is_finite()
            || !(0.00001..=0.1).contains(&self.tolerance)
            || !(0.0..=1.0).contains(&self.safety)
            || self.safety == 0.0
            || !(1..=512).contains(&self.iterations)
        {
            return Err(RenderError::InvalidInput("invalid field trace policy"));
        }
        Ok(())
    }
}

/// Query result; `Miss` is conditional on ideal 1-Lipschitz arithmetic, not certified GPU rounding.
#[derive(Clone, Copy, Debug)]
pub enum GraphGpuOutcome {
    /// Approximate zero-set hit and finite-difference normal.
    Hit {
        /// Approximate ray distance in world units.
        distance: f32,
        /// Finite-difference unit normal.
        normal: [f32; 3],
    },
    /// The ideal safe step passed the ray far limit.
    Miss,
    /// Interior start, invalid arithmetic or undefined finite-difference normal.
    Uncertain(GraphGpuIssue),
    /// Iteration budget was exhausted.
    Exhausted,
}

/// Diagnostic reason from the bounded WGSL tracer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphGpuIssue {
    /// Invalid or unrepresentable field sample.
    Numeric,
    /// Ray sample entered the negative region.
    Inside,
    /// Finite-difference gradient did not define a normal.
    UndefinedNormal,
    /// Ray distance could not advance in `f32`.
    NoProgress,
}

/// Display-only nearest result across enabled world-owned graph occurrences.
#[derive(Clone, Copy, Debug)]
pub enum WorldFieldGpuOutcome {
    /// Approximate nearest world-space entry, tied by occurrence allocation order.
    Hit {
        /// Stable world occurrence ID.
        id: EntityId,
        /// Approximate world-unit ray distance.
        distance: f32,
        /// Display normal in world coordinates under uniform positive scale.
        normal: [f32; 3],
    },
    /// Every enabled graph's GPU tracer returned its ideal-model miss status.
    Miss,
    /// One graph cannot be classified; a nearest hit cannot be guaranteed.
    Uncertain {
        /// First occurrence that prevented classification.
        id: EntityId,
        /// GPU tracing failure reason.
        issue: GraphGpuIssue,
    },
    /// One graph exhausted its sample budget.
    Exhausted {
        /// First occurrence that exhausted its sample budget.
        id: EntityId,
    },
}

/// Timings and output for one blocking offscreen frame.
pub struct GraphFrame {
    /// Tightly packed RGBA8 pixels, top row first.
    pub rgba: Vec<u8>,
    /// CPU time from command creation through GPU readback completion.
    pub full_frame_ms: f64,
    /// CPU preparation before command submission, including small buffer uploads.
    pub cpu_prepare_ms: f64,
    /// CPU wait, transfer and mapping after submission; this can include GPU execution.
    pub readback_wait_ms: f64,
    /// Compute-pass timestamp when the device supports and enables `TIMESTAMP_QUERY`.
    pub gpu_compute_ms: Option<f64>,
}

/// Offscreen graph draw size, trace policy, and optional compute timestamp sampler.
pub struct GraphRenderOptions<'a> {
    /// Pixel dimensions, each from 1 to 4096, with at most 3,686,400 pixels total.
    pub size: [u32; 2],
    /// Bounded field tracer settings.
    pub policy: GraphTracePolicy,
    /// Timestamp sampler created from a device with `TIMESTAMP_QUERY` enabled.
    pub timer: Option<&'a GpuTimer>,
}

/// Isolated research pipeline; does not mutate world state or existing renderer buffers.
pub struct GraphGpuRenderer {
    layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::ComputePipeline,
    query_pipeline: wgpu::ComputePipeline,
    sample_pipeline: wgpu::ComputePipeline,
}

fn vec4(v: spatial_math::Vec3, w: f32) -> [f32; 4] {
    [v.x() as f32, v.y() as f32, v.z() as f32, w]
}

pub(crate) fn gpu_nodes(graph: &Graph) -> Vec<GpuNode> {
    graph
        .nodes()
        .iter()
        .map(|node| match *node {
            Node::Sphere(s) => GpuNode {
                links: [0, 0, 0, 0],
                param: [s.radius() as f32, 0.0, 0.0, 0.0],
            },
            Node::Box(b) => GpuNode {
                links: [1, 0, 0, 0],
                param: vec4(b.half_extent(), 0.0),
            },
            Node::Translate { child, offset } => GpuNode {
                links: [2, child, 0, 0],
                param: vec4(offset, 0.0),
            },
            Node::Scale { child, factor } => GpuNode {
                links: [3, child, 0, 0],
                param: [factor as f32, 0.0, 0.0, 0.0],
            },
            Node::Union { left, right } => GpuNode {
                links: [4, left, right, 0],
                param: [0.0; 4],
            },
            Node::SmoothUnion { left, right, width } => GpuNode {
                links: [5, left, right, 0],
                param: [width as f32, 0.0, 0.0, 0.0],
            },
        })
        .collect()
}

impl GraphGpuRenderer {
    /// Compile the fixed shader; external JSON can never provide WGSL source.
    pub async fn new(device: &wgpu::Device) -> Result<Self, RenderError> {
        Self::new_source(device, include_str!("../shaders/field_graph.wgsl")).await
    }

    async fn new_source(device: &wgpu::Device, source: &str) -> Result<Self, RenderError> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("validated field graph interpreter"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("field graph resources"),
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("field graph pipeline"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("field graph compute"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let render_pipeline = pipeline("render");
        let query_pipeline = pipeline("query");
        let sample_pipeline = pipeline("sample_query");
        if let Some(error) = scope.pop().await {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(Self {
            layout,
            render_pipeline,
            query_pipeline,
            sample_pipeline,
        })
    }

    fn config(
        graph: &Graph,
        camera: Camera,
        size: [u32; 2],
        policy: GraphTracePolicy,
    ) -> Result<GpuConfig, RenderError> {
        policy.validate()?;
        if size[0] == 0
            || size[1] == 0
            || size[0] > 4096
            || size[1] > 4096
            || u64::from(size[0]) * u64::from(size[1]) > 2560 * 1440
        {
            return Err(RenderError::InvalidInput(
                "field image exceeds dimension or pixel budget",
            ));
        }
        let aspect = f64::from(size[0]) / f64::from(size[1]);
        let (near, far) = camera.ray(0, 0, size[0], size[1])?.interval();
        Ok(GpuConfig {
            origin: vec4(camera.origin, 0.0),
            forward: vec4(camera.forward, 0.0),
            right: vec4(
                camera
                    .right
                    .checked_scale(camera.tan_half_fov * aspect)
                    .map_err(|_| RenderError::InvalidInput("camera scale"))?,
                0.0,
            ),
            up: vec4(
                camera
                    .up
                    .checked_scale(camera.tan_half_fov)
                    .map_err(|_| RenderError::InvalidInput("camera scale"))?,
                0.0,
            ),
            view: [size[0] as f32, size[1] as f32, near as f32, far as f32],
            trace: [
                policy.tolerance,
                policy.safety,
                (policy.tolerance * 0.5).max(0.0001),
                0.0,
            ],
            meta: [
                graph.root(),
                graph.nodes().len() as u32,
                policy.iterations,
                0,
            ],
        })
    }

    fn group(
        &self,
        device: &wgpu::Device,
        graph: &Graph,
        config: GpuConfig,
        view: &wgpu::TextureView,
        rays: &[GpuRay],
    ) -> Result<(wgpu::BindGroup, wgpu::Buffer), RenderError> {
        // shortcut: node buffers are uploaded per call; cache by exact revision if repeated-frame cost dominates.
        let nodes = gpu_nodes(graph);
        let bytes = (nodes.len() * std::mem::size_of::<GpuNode>()) as u64;
        if bytes > device.limits().max_storage_buffer_binding_size
            || bytes > device.limits().max_buffer_size
        {
            return Err(RenderError::AccelerationCapacity {
                requested_bytes: bytes,
                allowed_bytes: device
                    .limits()
                    .max_storage_buffer_binding_size
                    .min(device.limits().max_buffer_size),
            });
        }
        let config_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("field config"),
            contents: bytemuck::bytes_of(&config),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let node_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("field nodes"),
            contents: bytemuck::cast_slice(&nodes),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let dummy = GpuRay::zeroed();
        let ray_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("field rays"),
            contents: if rays.is_empty() {
                bytemuck::bytes_of(&dummy)
            } else {
                bytemuck::cast_slice(rays)
            },
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("field outcomes"),
            size: (rays.len().max(1) * std::mem::size_of::<GpuOutcome>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("field graph bindings"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: config_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: node_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: ray_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        Ok((group, output))
    }

    /// Render and read actual GPU pixels. Red means uncertain; yellow means iteration exhaustion.
    pub fn render_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        graph: &Graph,
        camera: Camera,
        size: [u32; 2],
        policy: GraphTracePolicy,
    ) -> Result<GraphFrame, RenderError> {
        self.render_rgba_timed(
            device,
            queue,
            graph,
            camera,
            GraphRenderOptions {
                size,
                policy,
                timer: None,
            },
        )
    }

    /// Render with optional GPU compute timestamps, separate from CPU preparation and readback.
    pub fn render_rgba_timed(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        graph: &Graph,
        camera: Camera,
        options: GraphRenderOptions<'_>,
    ) -> Result<GraphFrame, RenderError> {
        let GraphRenderOptions {
            size,
            policy,
            timer,
        } = options;
        let started = Instant::now();
        let config = Self::config(graph, camera, size, policy)?;
        if size[0] > device.limits().max_texture_dimension_2d
            || size[1] > device.limits().max_texture_dimension_2d
        {
            return Err(RenderError::InvalidInput("adapter texture limit"));
        }
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("field graph image"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = image.create_view(&wgpu::TextureViewDescriptor::default());
        let (group, _) = self.group(device, graph, config, &view, &[])?;
        let row_bytes = size[0] * 4;
        let aligned = row_bytes.div_ceil(256) * 256;
        if u64::from(aligned) * u64::from(size[1]) > device.limits().max_buffer_size {
            return Err(RenderError::InvalidInput(
                "field image readback exceeds adapter buffer limit",
            ));
        }
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("field image readback"),
            size: u64::from(aligned) * u64::from(size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("field render"),
        });
        {
            let timestamps = timer.map(|timer| wgpu::ComputePassTimestampWrites {
                query_set: &timer.queries,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("field evaluate"),
                timestamp_writes: timestamps,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
        if let Some(timer) = timer {
            encoder.resolve_query_set(&timer.queries, 0..2, &timer.resolved, 0);
            encoder.copy_buffer_to_buffer(&timer.resolved, 0, &timer.readback, 0, 16);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &image,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        let cpu_prepare_ms = started.elapsed().as_secs_f64() * 1000.0;
        queue.submit([encoder.finish()]);
        let readback_started = Instant::now();
        let bytes = read_buffer(device, &output)?;
        let readback_wait_ms = readback_started.elapsed().as_secs_f64() * 1000.0;
        let mut rgba = Vec::with_capacity((row_bytes * size[1]) as usize);
        for row in bytes.chunks_exact(aligned as usize) {
            rgba.extend_from_slice(&row[..row_bytes as usize]);
        }
        let full_frame_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(GraphFrame {
            rgba,
            full_frame_ms,
            cpu_prepare_ms,
            readback_wait_ms,
            gpu_compute_ms: timer
                .map(|timer| timer.read_ms(device, queue))
                .transpose()?,
        })
    }

    /// Execute explicit rays with the same shader evaluator as the image path.
    pub fn query(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        graph: &Graph,
        camera: Camera,
        rays: &[Ray],
        policy: GraphTracePolicy,
    ) -> Result<Vec<GraphGpuOutcome>, RenderError> {
        if rays.is_empty() {
            return Ok(Vec::new());
        }
        if rays.len() > MAX_QUERY_RAYS {
            return Err(RenderError::InvalidInput("too many field query rays"));
        }
        let data: Vec<GpuRay> = rays
            .iter()
            .map(|ray| {
                let (near, far) = ray.interval();
                GpuRay {
                    origin_near: vec4(ray.origin(), near as f32),
                    direction_far: vec4(ray.direction(), far as f32),
                }
            })
            .collect();
        let config = Self::config(graph, camera, [1, 1], policy)?;
        let raw = self.dispatch_query(device, queue, graph, config, &data, &self.query_pipeline)?;
        Ok(raw
            .iter()
            .map(|r| match r.meta[0] {
                0 => GraphGpuOutcome::Miss,
                1 => GraphGpuOutcome::Hit {
                    distance: r.distance_normal[3],
                    normal: [
                        r.distance_normal[0],
                        r.distance_normal[1],
                        r.distance_normal[2],
                    ],
                },
                3 => GraphGpuOutcome::Exhausted,
                _ => GraphGpuOutcome::Uncertain(match r.meta[2] {
                    2 => GraphGpuIssue::Inside,
                    3 => GraphGpuIssue::UndefinedNormal,
                    4 => GraphGpuIssue::NoProgress,
                    _ => GraphGpuIssue::Numeric,
                }),
            })
            .collect())
    }

    /// GPU ray diagnostics for visible world-owned fields. Graph buffers are derived
    /// from the world and uploaded per occurrence; no contact authority is implied.
    pub fn query_world_fields(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &WorldState,
        camera: Camera,
        rays: &[Ray],
        policy: GraphTracePolicy,
    ) -> Result<Vec<WorldFieldGpuOutcome>, RenderError> {
        policy.validate()?;
        world
            .validate()
            .map_err(|_| RenderError::InvalidInput("invalid world"))?;
        if rays.len() > MAX_QUERY_RAYS {
            return Err(RenderError::InvalidInput("too many field query rays"));
        }
        let mut nearest = vec![WorldFieldGpuOutcome::Miss; rays.len()];
        for instance in world.field_instances().iter().filter(|item| item.enabled()) {
            let scale = instance.transform().scale();
            let local_rays = rays
                .iter()
                .map(|ray| {
                    let local_origin = instance
                        .transform()
                        .to_local(ray.origin())
                        .map_err(|_| RenderError::InvalidInput("field local ray overflow"))?;
                    let (near, far) = ray.interval();
                    Ray::new(local_origin, ray.direction(), near / scale, far / scale)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let graph = world
                .field_graph(instance.id())
                .ok_or(RenderError::InvalidInput("missing field graph"))?;
            for (slot, outcome) in nearest.iter_mut().zip(self.query(
                device,
                queue,
                graph,
                camera,
                &local_rays,
                policy,
            )?) {
                match outcome {
                    GraphGpuOutcome::Hit { distance, normal } => {
                        let distance = distance * scale as f32;
                        if !distance.is_finite() {
                            return Err(RenderError::InvalidInput("field hit overflow"));
                        }
                        if matches!(slot, WorldFieldGpuOutcome::Miss)
                            || matches!(slot, WorldFieldGpuOutcome::Hit { distance: prior, .. } if distance < *prior)
                        {
                            *slot = WorldFieldGpuOutcome::Hit {
                                id: instance.id(),
                                distance,
                                normal,
                            };
                        }
                    }
                    GraphGpuOutcome::Uncertain(issue) => {
                        if !matches!(
                            slot,
                            WorldFieldGpuOutcome::Uncertain { .. }
                                | WorldFieldGpuOutcome::Exhausted { .. }
                        ) {
                            *slot = WorldFieldGpuOutcome::Uncertain {
                                id: instance.id(),
                                issue,
                            };
                        }
                    }
                    GraphGpuOutcome::Exhausted => {
                        if !matches!(
                            slot,
                            WorldFieldGpuOutcome::Uncertain { .. }
                                | WorldFieldGpuOutcome::Exhausted { .. }
                        ) {
                            *slot = WorldFieldGpuOutcome::Exhausted { id: instance.id() };
                        }
                    }
                    GraphGpuOutcome::Miss => {}
                }
            }
        }
        Ok(nearest)
    }

    /// Read the WGSL evaluator's `f32` field samples. `None` denotes invalid GPU arithmetic.
    /// Samples are diagnostic and never certify a mathematical sign or boundary.
    pub fn sample_points(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        graph: &Graph,
        camera: Camera,
        points: &[spatial_math::Vec3],
    ) -> Result<Vec<Option<f32>>, RenderError> {
        if points.is_empty() {
            return Ok(Vec::new());
        }
        if points.len() > 4096 {
            return Err(RenderError::InvalidInput("too many field sample points"));
        }
        if points
            .iter()
            .any(|p| [p.x(), p.y(), p.z()].iter().any(|x| x.abs() > 10_000.0))
        {
            return Err(RenderError::InvalidInput(
                "field sample outside supported coordinates",
            ));
        }
        let data: Vec<GpuRay> = points
            .iter()
            .map(|&point| GpuRay {
                origin_near: vec4(point, 0.0),
                direction_far: [0.0; 4],
            })
            .collect();
        let config = Self::config(graph, camera, [1, 1], GraphTracePolicy::default())?;
        let raw =
            self.dispatch_query(device, queue, graph, config, &data, &self.sample_pipeline)?;
        Ok(raw
            .iter()
            .map(|r| (r.meta[0] == 1).then_some(r.distance_normal[0]))
            .collect())
    }

    fn dispatch_query(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        graph: &Graph,
        config: GpuConfig,
        data: &[GpuRay],
        pipeline: &wgpu::ComputePipeline,
    ) -> Result<Vec<GpuOutcome>, RenderError> {
        if data.is_empty() {
            return Ok(Vec::new());
        }
        if data.len() > MAX_QUERY_RAYS {
            return Err(RenderError::InvalidInput("too many field query inputs"));
        }
        let input_bytes = std::mem::size_of_val(data) as u64;
        let output_bytes = (data.len() * std::mem::size_of::<GpuOutcome>()) as u64;
        if input_bytes > device.limits().max_storage_buffer_binding_size
            || input_bytes > device.limits().max_buffer_size
            || output_bytes > device.limits().max_storage_buffer_binding_size
            || output_bytes > device.limits().max_buffer_size
        {
            return Err(RenderError::InvalidInput(
                "field query exceeds adapter buffer limit",
            ));
        }
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("field query dummy"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = image.create_view(&wgpu::TextureViewDescriptor::default());
        let (group, output) = self.group(device, graph, config, &view, data)?;
        let size = output_bytes;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("field query readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("field query"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("field query"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((data.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
        queue.submit([encoder.finish()]);
        let bytes = read_buffer(device, &readback)?;
        Ok(bytemuck::cast_slice::<u8, GpuOutcome>(&bytes).to_vec())
    }
}

fn read_buffer(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>, RenderError> {
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| RenderError::Gpu(e.to_string()))?;
    rx.recv()
        .map_err(|e| RenderError::Gpu(e.to_string()))?
        .map_err(|e| RenderError::Gpu(e.to_string()))?;
    let bytes = slice
        .get_mapped_range()
        .map_err(|e| RenderError::Gpu(e.to_string()))?
        .to_vec();
    buffer.unmap();
    Ok(bytes)
}

#[cfg(test)]
mod workload_tests {
    use super::*;
    use spatial_math::Vec3;
    use world_authoring::field_graph::{TraceOptions, TraceOutcome, compile_json, trace};

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }

    #[test]
    #[ignore = "requires native GPU; profile real query step and status distributions"]
    fn gpu_ray_workload_distribution() {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
        let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
        let size = [256, 256];
        let rays = (0..size[1])
            .flat_map(|y| (0..size[0]).map(move |x| camera.ray(x, y, size[0], size[1]).unwrap()))
            .collect::<Vec<_>>();
        let data = rays
            .iter()
            .map(|ray| {
                let (near, far) = ray.interval();
                GpuRay {
                    origin_near: vec4(ray.origin(), near as f32),
                    direction_far: vec4(ray.direction(), far as f32),
                }
            })
            .collect::<Vec<_>>();
        for (name, source) in [
            ("sphere", br#"{"format_version":1,"id":"profile-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.9}]}"#.as_slice()),
            ("balanced-16", include_bytes!("../../../examples/field-graph/definitions/balanced-16.json").as_slice()),
        ] {
            let graph = compile_json(source).unwrap();
            let config = GraphGpuRenderer::config(&graph, camera, size, GraphTracePolicy::default()).unwrap();
            let outcomes = renderer.dispatch_query(&device, &queue, &graph, config, &data, &renderer.query_pipeline).unwrap();
            let mut samples = Vec::with_capacity(outcomes.len());
            let mut statuses = [0usize; 4];
            for result in &outcomes {
                let kind = result.meta[0] as usize;
                assert!(kind < statuses.len());
                statuses[kind] += 1;
                let ray_samples = result.meta[1] + u32::from(kind != 3);
                let normal_samples = if kind == 1 || (kind == 2 && result.meta[2] == 3) { 6 } else { 0 };
                samples.push(ray_samples + normal_samples);
            }
            samples.sort_unstable();
            let total: u64 = samples.iter().map(|&count| u64::from(count)).sum();
            println!("GPU_WORKLOAD {}", serde_json::json!({
                "scene": name, "definition_hash": graph.revision().hex(),
                "adapter": adapter.get_info().name, "backend": format!("{:?}", adapter.get_info().backend),
                "rays": outcomes.len(), "status_miss_hit_uncertain_exhausted": statuses,
                "samples_total": total, "samples_p50": samples[samples.len()/2],
                "samples_p95": samples[(samples.len()*95).div_ceil(100)-1], "samples_max": samples[samples.len()-1],
                "charged_node_visits_upper_bound": total * graph.visits() as u64,
                "graph_worst_case_visits_per_sample": graph.visits(),
            }));
        }
    }

    fn specialized_source(graph: &Graph) -> String {
        crate::field_compiler::specialized_field_source(graph).expect("supported test graph")
    }

    fn save_bmp(path: &std::path::Path, rgba: &[u8], size: [u32; 2]) {
        let [width, height] = size;
        let stride = (width * 3).div_ceil(4) * 4;
        let data_bytes = stride * height;
        let mut bmp = Vec::with_capacity((54 + data_bytes) as usize);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&(54 + data_bytes).to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(width as i32).to_le_bytes());
        bmp.extend_from_slice(&(height as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        for y in (0..height).rev() {
            let row_start = bmp.len();
            for pixel in
                rgba[(y * width * 4) as usize..((y + 1) * width * 4) as usize].chunks_exact(4)
            {
                bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
            bmp.resize(row_start + stride as usize, 0);
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bmp).unwrap();
    }

    #[test]
    #[ignore = "requires native GPU; compare validated graph specialization to the interpreter"]
    fn specialized_wgsl_feasibility() {
        let instance = if std::env::var("GENESIS_TEST_GPU_BACKEND").as_deref() == Ok("dx12") {
            let mut options = wgpu::InstanceDescriptor::new_without_display_handle();
            options.backends = wgpu::Backends::DX12;
            wgpu::Instance::new(options)
        } else {
            wgpu::Instance::default()
        };
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
            ..Default::default()
        }))
        .unwrap();
        let timer = GpuTimer::new(&device);
        let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
        for (name, source) in [
            ("sphere", br#"{"format_version":1,"id":"profile-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.9}]}"#.as_slice()),
            ("box", br#"{"format_version":1,"id":"compiler-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.7,0.5,0.9]}]}"#.as_slice()),
            ("nested-transform", br#"{"format_version":1,"id":"compiler-nested","root":"u","nodes":[{"op":"sphere","id":"s","radius":0.65},{"op":"scale","id":"z","child":"s","factor":1.4},{"op":"translate","id":"t","child":"z","offset":[0.3,0,0]},{"op":"box","id":"b","half_extent":[0.4,0.6,0.3]},{"op":"union","id":"u","left":"t","right":"b"}]}"#.as_slice()),
            ("balanced-16", include_bytes!("../../../examples/field-graph/definitions/balanced-16.json").as_slice()),
            ("smooth-bridge", include_bytes!("../../../examples/field-graph/definitions/smooth-bridge.json").as_slice()),
        ] {
            let graph = compile_json(source).unwrap();
            let started = Instant::now();
            let interpreter = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
            let interpreter_compile_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            let shader = specialized_source(&graph);
            let source_generation_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            let specialized = pollster::block_on(GraphGpuRenderer::new_source(&device, &shader)).unwrap();
            let specialized_compile_ms = started.elapsed().as_secs_f64() * 1000.0;
            let rays = (0..256).flat_map(|y| (0..256).map(move |x| camera.ray(x, y, 256, 256).unwrap())).collect::<Vec<_>>();
            let expected = interpreter.query(&device, &queue, &graph, camera, &rays, GraphTracePolicy::default()).unwrap();
            let actual = specialized.query(&device, &queue, &graph, camera, &rays, GraphTracePolicy::default()).unwrap();
            let mut distance_error = 0.0_f32;
            let mut worst_distance = (0usize, 0.0_f32, 0.0_f32);
            let mut normal_error = 0.0_f32;
            for (index, (a, b)) in expected.iter().zip(&actual).enumerate() {
                match (a, b) {
                    (GraphGpuOutcome::Hit { distance: da, normal: na }, GraphGpuOutcome::Hit { distance: db, normal: nb }) => {
                        if (da - db).abs() > distance_error {
                            distance_error = (da - db).abs();
                            worst_distance = (index, *da, *db);
                        }
                        for axis in 0..3 { normal_error = normal_error.max((na[axis] - nb[axis]).abs()); }
                    }
                    (GraphGpuOutcome::Miss, GraphGpuOutcome::Miss) | (GraphGpuOutcome::Exhausted, GraphGpuOutcome::Exhausted) => {}
                    (GraphGpuOutcome::Uncertain(a), GraphGpuOutcome::Uncertain(b)) if a == b => {}
                    _ => panic!("{name} specialized status differs on ray {index}: {a:?} versus {b:?}"),
                }
            }
            assert!(distance_error <= 0.005 && normal_error <= 0.05, "{name} distance/normal drift: {distance_error}/{normal_error}");
            let worst_ray = rays[worst_distance.0];
            let (near, far) = worst_ray.interval();
            let cpu_worst = trace(&graph, worst_ray.origin(), worst_ray.direction(), TraceOptions {
                near, far, tolerance: f64::from(GraphTracePolicy::default().tolerance),
                safety: f64::from(GraphTracePolicy::default().safety),
                iterations: GraphTracePolicy::default().iterations,
            }).unwrap();
            let cpu_worst_distance = match cpu_worst {
                TraceOutcome::Hit { distance } => Some(distance),
                _ => None,
            };
            let points = [worst_distance.1, worst_distance.2].map(|distance| {
                let o = worst_ray.origin();
                let d = worst_ray.direction();
                let t = f64::from(distance);
                v(o.x() + d.x() * t, o.y() + d.y() * t, o.z() + d.z() * t)
            });
            let cpu_samples = points.map(|point| graph.sample(point).unwrap());
            let interpreter_samples = interpreter.sample_points(&device, &queue, &graph, camera, &points).unwrap();
            let specialized_samples = specialized.sample_points(&device, &queue, &graph, camera, &points).unwrap();
            let mut measurements = Vec::new();
            for (label, renderer) in [("interpreter", &interpreter), ("specialized", &specialized)] {
                let mut gpu = Vec::new();
                let mut image = Vec::new();
                for run in 0..10 {
                    let frame = renderer.render_rgba_timed(&device, &queue, &graph, camera, GraphRenderOptions {
                        size: [1280, 720], policy: GraphTracePolicy::default(), timer: timer.as_ref(),
                    }).unwrap();
                    if run >= 3 && let Some(ms) = frame.gpu_compute_ms { gpu.push(ms); }
                    if run == 9 { image = frame.rgba; }
                }
                gpu.sort_by(f64::total_cmp);
                measurements.push((label, gpu.get(gpu.len()/2).copied(), image));
            }
            let different_pixels = measurements[0].2.chunks_exact(4).zip(measurements[1].2.chunks_exact(4)).filter(|(a,b)| a != b).count();
            let background = &measurements[0].2[..4];
            assert_eq!(&measurements[1].2[..4], background);
            let changed_occupancy = measurements[0].2.chunks_exact(4).zip(measurements[1].2.chunks_exact(4))
                .filter(|(a,b)| (*a == background) != (*b == background)).count();
            let max_channel_difference = measurements[0].2.iter().zip(&measurements[1].2)
                .map(|(a,b)| a.abs_diff(*b)).max().unwrap_or(0);
            let capture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/performance-research-1-captures");
            for (label, _, image) in &measurements {
                save_bmp(&capture_dir.join(format!("{name}-{label}.bmp")), image, [1280, 720]);
            }
            println!("COMPILER_EXPERIMENT {}", serde_json::json!({
                "scene": name, "definition_hash": graph.revision().hex(), "nodes": graph.nodes().len(),
                "source_generation_ms": source_generation_ms, "interpreter_compile_ms": interpreter_compile_ms,
                "specialized_compile_ms": specialized_compile_ms, "interpreter_gpu_ms_p50": measurements[0].1,
                "specialized_gpu_ms_p50": measurements[1].1, "different_pixels": different_pixels,
                "changed_occupancy_pixels": changed_occupancy, "max_channel_difference": max_channel_difference,
                "max_distance_error": distance_error, "worst_distance_ray": worst_distance,
                "cpu_worst_distance": cpu_worst_distance,
                "cpu_worst_samples": cpu_samples, "interpreter_worst_samples": interpreter_samples,
                "specialized_worst_samples": specialized_samples,
                "max_normal_component_error": normal_error,
                "rays_compared": rays.len(), "adapter": adapter.get_info().name,
                "backend": format!("{:?}", adapter.get_info().backend),
            }));
        }
    }
}
