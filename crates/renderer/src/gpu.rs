use crate::{Bvh, Camera, Ray, RenderError, Scene, acceleration::FlatNode};
use bytemuck::{Pod, Zeroable};
use std::{
    sync::{Mutex, mpsc},
    time::Instant,
};
use wgpu::util::DeviceExt;

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
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    readback: wgpu::Buffer,
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
            color: [p.color[0], p.color[1], p.color[2], 1.0],
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
    compute_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::ComputePipeline,
    query_pipeline: wgpu::ComputePipeline,
    present_layout: wgpu::BindGroupLayout,
    present_pipeline: wgpu::RenderPipeline,
    primitive_budget_bytes: u64,
    dummy_bvh: wgpu::Buffer,
    primitives: Mutex<Option<PrimitiveBuffer>>,
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
            compute_layout,
            render_pipeline,
            query_pipeline,
            present_layout,
            present_pipeline,
            primitive_budget_bytes,
            dummy_bvh,
            primitives: Mutex::new(None),
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
        let [width, height] = options.size;
        if width == 0
            || height == 0
            || width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(RenderError::InvalidInput("invalid render resolution"));
        }
        let image = Self::image(device, width, height);
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
                traversal,
            ),
            &image_view,
            &[],
        )?;
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
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &group, &[]);
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
        for shader in [
            include_str!("../shaders/analytic.wgsl"),
            include_str!("../shaders/present.wgsl"),
        ] {
            let module = naga::front::wgsl::parse_str(shader).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
