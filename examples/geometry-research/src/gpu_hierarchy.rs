//! Research GPU hierarchy execution; no expanded leaf-transform buffer.

use crate::hierarchy::GpuProgram;
#[cfg(test)]
use crate::hierarchy::{GpuDefinition, GpuEdge, GpuNode};
use bytemuck::{Pod, Zeroable};
use std::{error::Error, sync::mpsc, time::Instant};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Parameters {
    origin: [f32; 4],
    forward: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    view: [f32; 4],
    counts: [u32; 4],
    scale: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Diagnostic {
    pub normal_t: [f32; 4],
    pub meta: [u32; 4], // hit, depth-first ID, definition, exhausted
    pub work: [u32; 4], // visits, leaves, stack peak
}

pub struct HierarchyRenderer {
    pipeline: wgpu::ComputePipeline,
    definitions: wgpu::Buffer,
    nodes: wgpu::Buffer,
    edges: wgpu::Buffer,
    program_counts: [u32; 4],
    pub packed_bytes: usize,
    pub pipeline_ms: f64,
}

pub struct Frame {
    pub texture: wgpu::Texture,
    pub diagnostics: Vec<Diagnostic>,
    pub compute_ms: Option<f64>,
    pub blocking_ms: f64,
    pub upload_enqueue_ms: f64,
}

#[derive(Clone, Copy)]
pub struct View {
    pub size: [u32; 2],
    pub center: [f64; 3],
    pub distance: f64,
    pub scale: f64,
    pub prune: bool,
}

fn storage<T: Pod>(device: &wgpu::Device, label: &'static str, values: &[T]) -> wgpu::Buffer {
    // Auto layouts require one whole storage-array element even when the array is logically empty.
    let bytes = bytemuck::cast_slice(values);
    let zero = vec![0; size_of::<T>()];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: if bytes.is_empty() { &zero } else { bytes },
        usage: wgpu::BufferUsages::STORAGE,
    })
}

fn mapped_bytes(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>, Box<dyn Error>> {
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = slice.get_mapped_range()?;
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(bytes)
}

impl HierarchyRenderer {
    pub fn new(device: &wgpu::Device, program: &GpuProgram) -> Result<Self, Box<dyn Error>> {
        let limits = device.limits();
        let packed_bytes = size_of_val(program.definitions.as_slice())
            + size_of_val(program.nodes.as_slice())
            + size_of_val(program.edges.as_slice());
        for bytes in [
            size_of_val(program.definitions.as_slice()),
            size_of_val(program.nodes.as_slice()),
            size_of_val(program.edges.as_slice()),
        ] {
            if bytes as u64 > limits.max_storage_buffer_binding_size {
                return Err("hierarchy exceeds adapter storage binding limit".into());
            }
        }
        let started = Instant::now();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GR-6 generic compact hierarchy"),
            source: wgpu::ShaderSource::Wgsl(include_str!("hierarchy.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GR-6 hierarchy renderer"),
            layout: None,
            module: &shader,
            entry_point: Some("render"),
            compilation_options: Default::default(),
            cache: None,
        });
        let pipeline_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(Self {
            pipeline,
            definitions: storage(device, "GR-6 definitions", &program.definitions),
            nodes: storage(device, "GR-6 nodes", &program.nodes),
            edges: storage(device, "GR-6 edges", &program.edges),
            program_counts: [
                program.nodes.len() as u32,
                program.edges.len() as u32,
                program.definitions.len() as u32,
                program.root,
            ],
            packed_bytes,
            pipeline_ms,
        })
    }

    /// Replaces validated hierarchy data on the same device without recompiling WGSL.
    pub fn update_program(
        &mut self,
        device: &wgpu::Device,
        program: &GpuProgram,
    ) -> Result<f64, Box<dyn Error>> {
        let started = Instant::now();
        for bytes in [
            size_of_val(program.definitions.as_slice()),
            size_of_val(program.nodes.as_slice()),
            size_of_val(program.edges.as_slice()),
        ] {
            if bytes as u64 > device.limits().max_storage_buffer_binding_size {
                return Err("updated hierarchy exceeds adapter storage binding limit".into());
            }
        }
        self.definitions = storage(device, "GR-6 updated definitions", &program.definitions);
        self.nodes = storage(device, "GR-6 updated nodes", &program.nodes);
        self.edges = storage(device, "GR-6 updated edges", &program.edges);
        self.program_counts = [
            program.nodes.len() as u32,
            program.edges.len() as u32,
            program.definitions.len() as u32,
            program.root,
        ];
        self.packed_bytes = size_of_val(program.definitions.as_slice())
            + size_of_val(program.nodes.as_slice())
            + size_of_val(program.edges.as_slice());
        Ok(started.elapsed().as_secs_f64() * 1000.0)
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        options: View,
    ) -> Result<Frame, Box<dyn Error>> {
        let started = Instant::now();
        let View {
            size,
            center,
            distance,
            scale,
            prune,
        } = options;
        let aspect = size[0] as f64 / size[1] as f64;
        let spread = (0.8_f64 / 2.0).tan();
        let params = Parameters {
            origin: [
                center[0] as f32,
                center[1] as f32,
                (center[2] - distance) as f32,
                0.0,
            ],
            forward: [0.0, 0.0, 1.0, 0.0],
            right: [-(aspect * spread) as f32, 0.0, 0.0, 0.0],
            up: [0.0, spread as f32, 0.0, 0.0],
            view: [size[0] as f32, size[1] as f32, 0.001, 1000.0],
            counts: self.program_counts,
            scale: [scale as f32, f32::from(prune as u8), 0.0, 0.0],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GR-6 camera and dynamic scale"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let count = u64::from(size[0]) * u64::from(size[1]);
        let diagnostic_bytes = count * size_of::<Diagnostic>() as u64;
        if diagnostic_bytes > device.limits().max_storage_buffer_binding_size {
            return Err("diagnostic frame exceeds adapter storage binding limit".into());
        }
        let diagnostics = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GR-6 per-pixel diagnostics"),
            size: diagnostic_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GR-6 diagnostic readback"),
            size: diagnostic_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GR-6 hierarchy image"),
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
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GR-6 compact hierarchy bindings"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.definitions.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.nodes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.edges.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: diagnostics.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        let upload_enqueue_ms = started.elapsed().as_secs_f64() * 1000.0;
        let queries = if device.features().contains(
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        ) {
            Some(device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("GR-6 GPU timer"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }))
        } else {
            None
        };
        let timestamp = queries.as_ref().map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GR-6 timestamps"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });
        let timestamp_readback = queries.as_ref().map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GR-6 timestamp readback"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        if let Some(queries) = &queries {
            encoder.write_timestamp(queries, 0);
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
        if let (Some(queries), Some(timestamp), Some(timestamp_readback)) =
            (&queries, &timestamp, &timestamp_readback)
        {
            encoder.write_timestamp(queries, 1);
            encoder.resolve_query_set(queries, 0..2, timestamp, 0);
            encoder.copy_buffer_to_buffer(timestamp, 0, timestamp_readback, 0, 16);
        }
        encoder.copy_buffer_to_buffer(&diagnostics, 0, &readback, 0, diagnostic_bytes);
        queue.submit([encoder.finish()]);
        let bytes = mapped_bytes(device, &readback)?;
        let diagnostics = bytemuck::cast_slice::<u8, Diagnostic>(&bytes).to_vec();
        let compute_ms = if let Some(buffer) = timestamp_readback {
            let bytes = mapped_bytes(device, &buffer)?;
            let values = bytemuck::cast_slice::<u8, u64>(&bytes);
            Some(
                (values[1] - values[0]) as f64 * f64::from(queue.get_timestamp_period())
                    / 1_000_000.0,
            )
        } else {
            None
        };
        Ok(Frame {
            texture,
            diagnostics,
            compute_ms,
            blocking_ms: started.elapsed().as_secs_f64() * 1000.0,
            upload_enqueue_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hierarchy::Hierarchy;
    use analytic_renderer::{Camera, GpuRenderer};
    use spatial_math::Vec3;
    #[test]
    fn host_layout_matches_wgsl_storage_records() {
        assert_eq!(size_of::<GpuDefinition>(), 48);
        assert_eq!(size_of::<GpuNode>(), 48);
        assert_eq!(size_of::<GpuEdge>(), 48);
        assert_eq!(size_of::<Diagnostic>(), 48);
        assert_eq!(size_of::<Parameters>(), 112);
    }

    #[test]
    #[ignore = "requires a native GPU; run with --ignored --test-threads=1"]
    fn actual_gpu_small_hierarchies_and_return_to_base() {
        let sources: &[&[u8]] = &[
            br#"{"format_version":1,"root":"s","definitions":[{"kind":"sphere","id":"d","radius":0.8,"color":[1,0,0]}],"nodes":[{"kind":"leaf","id":"s","definition":"d"}]}"#,
            br#"{"format_version":1,"root":"c","definitions":[{"kind":"capsule","id":"d","a":[-0.5,0,0],"b":[0.5,0,0],"radius":0.2,"color":[0,1,0]}],"nodes":[{"kind":"leaf","id":"c","definition":"d"}]}"#,
            br#"{"format_version":1,"root":"outer","definitions":[{"kind":"capsule","id":"d","a":[0,0,0],"b":[0,1,0],"radius":0.15,"color":[0,0,1]}],"nodes":[{"kind":"leaf","id":"c","definition":"d"},{"kind":"group","id":"inner","children":[{"node":"c","at":[0,-0.5,0],"angle_deg":25}]},{"kind":"group","id":"outer","children":[{"node":"inner","angle_deg":-40,"scale":1.2}]}]}"#,
            br#"{"format_version":1,"root":"r","definitions":[{"kind":"sphere","id":"d","radius":0.2,"color":[1,1,0]}],"nodes":[{"kind":"leaf","id":"s","definition":"d"},{"kind":"repeat","id":"r","child":"s","count":3,"step":{"at":[0.7,0,0]}}]}"#,
            br#"{"format_version":1,"root":"g","definitions":[{"kind":"sphere","id":"d","radius":0.5,"color":[1,0,1]}],"nodes":[{"kind":"leaf","id":"s","definition":"d"},{"kind":"group","id":"g","children":[{"node":"s"},{"node":"s"}]}]}"#,
            br#"{"format_version":1,"root":"g","definitions":[],"nodes":[{"kind":"group","id":"g","children":[]}]}"#,
        ];
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let direct =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let size = [64, 64];
        let view = View {
            size,
            center: [0.0; 3],
            distance: 5.0,
            scale: 1.0,
            prune: false,
        };
        let camera = Camera::look_at(
            Vec3::new(0.0, 0.0, -5.0).unwrap(),
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0).unwrap(),
            0.8,
        )
        .unwrap();
        for source in sources {
            let hierarchy = Hierarchy::parse(source).unwrap();
            let renderer =
                HierarchyRenderer::new(&device, &hierarchy.gpu_program().unwrap()).unwrap();
            let scene = hierarchy.materialize(1.0).unwrap().scene;
            let raw = renderer.render(&device, &queue, view).unwrap();
            let pruned = renderer
                .render(
                    &device,
                    &queue,
                    View {
                        prune: true,
                        ..view
                    },
                )
                .unwrap();
            assert!(raw.diagnostics.iter().all(|d| d.meta[3] == 0));
            assert!(pruned.diagnostics.iter().all(|d| d.meta[3] == 0));
            for (a, b) in raw.diagnostics.iter().zip(&pruned.diagnostics) {
                assert_eq!(a.meta[0..2], b.meta[0..2]);
            }
            let pixels = [(20, 20), (32, 32), (43, 32), (32, 43), (12, 32)];
            let rays: Vec<_> = pixels
                .iter()
                .map(|&(x, y)| camera.ray(x, y, size[0], size[1]).unwrap())
                .collect();
            for (&(x, y), hit) in pixels
                .iter()
                .zip(direct.query(&device, &queue, &scene, &rays).unwrap())
            {
                let result = raw.diagnostics[(y * size[0] + x) as usize];
                assert_eq!(
                    hit.map(|h| h.id),
                    (result.meta[0] != 0).then_some(result.meta[1])
                );
            }
            let grown = renderer
                .render(&device, &queue, View { scale: 1.3, ..view })
                .unwrap();
            let returned = renderer.render(&device, &queue, view).unwrap();
            assert_eq!(
                raw.diagnostics.iter().map(|d| d.meta).collect::<Vec<_>>(),
                returned
                    .diagnostics
                    .iter()
                    .map(|d| d.meta)
                    .collect::<Vec<_>>()
            );
            if !scene.primitives().is_empty() {
                assert_ne!(
                    raw.diagnostics
                        .iter()
                        .map(|d| d.normal_t[3])
                        .collect::<Vec<_>>(),
                    grown
                        .diagnostics
                        .iter()
                        .map(|d| d.normal_t[3])
                        .collect::<Vec<_>>()
                );
            }
        }

        let original = Hierarchy::parse(sources[0]).unwrap();
        let mut renderer =
            HierarchyRenderer::new(&device, &original.gpu_program().unwrap()).unwrap();
        let original_frame = renderer.render(&device, &queue, view).unwrap();
        let revised = Hierarchy::parse(
            std::str::from_utf8(sources[0])
                .unwrap()
                .replace("\"radius\":0.8", "\"radius\":0.5")
                .as_bytes(),
        )
        .unwrap();
        let parameter_update_ms = renderer
            .update_program(&device, &revised.gpu_program().unwrap())
            .unwrap();
        let changed = renderer.render(&device, &queue, view).unwrap();
        assert_ne!(
            original_frame
                .diagnostics
                .iter()
                .map(|d| d.meta[0])
                .collect::<Vec<_>>(),
            changed
                .diagnostics
                .iter()
                .map(|d| d.meta[0])
                .collect::<Vec<_>>()
        );
        renderer
            .update_program(&device, &original.gpu_program().unwrap())
            .unwrap();
        let returned = renderer.render(&device, &queue, view).unwrap();
        assert_eq!(
            original_frame
                .diagnostics
                .iter()
                .map(|d| d.meta[0])
                .collect::<Vec<_>>(),
            returned
                .diagnostics
                .iter()
                .map(|d| d.meta[0])
                .collect::<Vec<_>>()
        );
        let original_repeat = Hierarchy::parse(sources[3]).unwrap();
        renderer
            .update_program(&device, &original_repeat.gpu_program().unwrap())
            .unwrap();
        let before_repeat_edit = renderer.render(&device, &queue, view).unwrap();
        let reduced_repeat = Hierarchy::parse(
            std::str::from_utf8(sources[3])
                .unwrap()
                .replace("\"count\":3", "\"count\":2")
                .as_bytes(),
        )
        .unwrap();
        let topology_update_ms = renderer
            .update_program(&device, &reduced_repeat.gpu_program().unwrap())
            .unwrap();
        let after_repeat_edit = renderer.render(&device, &queue, view).unwrap();
        assert_ne!(
            before_repeat_edit
                .diagnostics
                .iter()
                .map(|d| d.meta[0])
                .collect::<Vec<_>>(),
            after_repeat_edit
                .diagnostics
                .iter()
                .map(|d| d.meta[0])
                .collect::<Vec<_>>()
        );
        println!(
            "GR6_UPDATE parameter_enqueue_ms={parameter_update_ms:.4} topology_enqueue_ms={topology_update_ms:.4} pipeline_reused=true"
        );
        // Deliberately bypass host validation to exercise the shader's explicit
        // bounded-work diagnostic; valid parsed sources cannot contain cycles.
        let cyclic = GpuProgram {
            definitions: Vec::new(),
            nodes: vec![GpuNode {
                info: [1, 0, 1, 0],
                lower: [-1.0, -1.0, -1.0, 0.0],
                upper: [1.0, 1.0, 1.0, 0.0],
            }],
            edges: vec![GpuEdge {
                child: [0, 0, 0, 0],
                at_scale: [0.0, 0.0, 0.0, 1.0],
                angle: [0.0; 4],
            }],
            root: 0,
        };
        renderer.update_program(&device, &cyclic).unwrap();
        let exhausted = renderer.render(&device, &queue, view).unwrap();
        assert!(exhausted.diagnostics.iter().all(|d| d.meta[3] == 1));
    }
}
