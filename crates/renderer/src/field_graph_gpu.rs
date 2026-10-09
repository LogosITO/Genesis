//! Direct WGSL interpretation of the bounded authored field graph.

use crate::{Camera, Ray, RenderError};
use bytemuck::{Pod, Zeroable};
use std::{sync::mpsc, time::Instant};
use wgpu::util::DeviceExt;
use world_authoring::field_graph::{Graph, Node};

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
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuNode {
    links: [u32; 4],
    param: [f32; 4],
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
    fn validate(self) -> Result<(), RenderError> {
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

/// Timings and output for one blocking offscreen frame.
pub struct GraphFrame {
    /// Tightly packed RGBA8 pixels, top row first.
    pub rgba: Vec<u8>,
    /// CPU time from command creation through GPU readback completion.
    pub full_frame_ms: f64,
}

/// Isolated research pipeline; does not mutate world state or existing renderer buffers.
pub struct GraphGpuRenderer {
    layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::ComputePipeline,
    query_pipeline: wgpu::ComputePipeline,
}

fn vec4(v: spatial_math::Vec3, w: f32) -> [f32; 4] {
    [v.x() as f32, v.y() as f32, v.z() as f32, w]
}

fn gpu_nodes(graph: &Graph) -> Vec<GpuNode> {
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
        })
        .collect()
}

impl GraphGpuRenderer {
    /// Compile the fixed shader; external JSON can never provide WGSL source.
    pub async fn new(device: &wgpu::Device) -> Result<Self, RenderError> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("validated field graph interpreter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/field_graph.wgsl").into()),
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
        if let Some(error) = scope.pop().await {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(Self {
            layout,
            render_pipeline,
            query_pipeline,
        })
    }

    fn config(
        graph: &Graph,
        camera: Camera,
        size: [u32; 2],
        policy: GraphTracePolicy,
    ) -> Result<GpuConfig, RenderError> {
        policy.validate()?;
        if size[0] == 0 || size[1] == 0 || size[0] > 2048 || size[1] > 2048 {
            return Err(RenderError::InvalidInput(
                "field image outside 1..=2048 pixels",
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
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("field evaluate"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
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
        queue.submit([encoder.finish()]);
        let bytes = read_buffer(device, &output)?;
        let mut rgba = Vec::with_capacity((row_bytes * size[1]) as usize);
        for row in bytes.chunks_exact(aligned as usize) {
            rgba.extend_from_slice(&row[..row_bytes as usize]);
        }
        Ok(GraphFrame {
            rgba,
            full_frame_ms: started.elapsed().as_secs_f64() * 1000.0,
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
        if rays.len() > 4096 {
            return Err(RenderError::InvalidInput("too many field query rays"));
        }
        let config = Self::config(graph, camera, [1, 1], policy)?;
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
        let (group, output) = self.group(device, graph, config, &view, &data)?;
        let size = (data.len() * std::mem::size_of::<GpuOutcome>()) as u64;
        if size > device.limits().max_buffer_size
            || size > device.limits().max_storage_buffer_binding_size
        {
            return Err(RenderError::InvalidInput(
                "field query exceeds adapter buffer limit",
            ));
        }
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
            pass.set_pipeline(&self.query_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((rays.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
        queue.submit([encoder.finish()]);
        let bytes = read_buffer(device, &readback)?;
        Ok(bytemuck::cast_slice::<u8, GpuOutcome>(&bytes)
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
