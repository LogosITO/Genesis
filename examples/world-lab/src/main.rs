//! Standalone experimental viewer for externally authored mathematical scenes.
#[path = "../../first-light/src/hud.rs"]
mod hud;
mod scene;

use analytic_renderer::{
    DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer, GpuTimer,
    MixedDrawOptions,
    field_graph_gpu::GraphTracePolicy,
    material::{Material, MaterialPattern},
};
use scene::LabScene;
use spatial_math::{Transform, Vec3};
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

struct Graphics {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuRenderer,
    timer: Option<GpuTimer>,
    hud: hud::Hud,
}

impl Graphics {
    async fn open(window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await?;
        let info = adapter.get_info();
        eprintln!(
            "World Lab GPU: {} / {:?} / {}",
            info.name, info.backend, info.driver
        );
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Genesis World Lab"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await?;
        device.on_uncaptured_error(Arc::new(|error| eprintln!("GPU validation error: {error}")));
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or("no surface format")?;
        let size = window.inner_size();
        if size.width == 0
            || size.height == 0
            || size.width > device.limits().max_texture_dimension_2d
            || size.height > device.limits().max_texture_dimension_2d
        {
            return Err("invalid initial window size".into());
        }
        let config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or("surface unsupported")?;
        let config = wgpu::SurfaceConfiguration { format, ..config };
        surface.configure(&device, &config);
        let renderer = GpuRenderer::new(&device, format).await?;
        let timer = GpuTimer::new(&device);
        let hud = hud::Hud::new(&device, format);
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            timer,
            hud,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0
            && height > 0
            && width <= self.device.limits().max_texture_dimension_2d
            && height <= self.device.limits().max_texture_dimension_2d
        {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

struct App {
    path: PathBuf,
    scene: LabScene,
    graphics: Option<Graphics>,
    yaw: f64,
    elevation: f64,
    distance: f64,
    show_analytic: bool,
    show_fields: bool,
    diagnostic: bool,
    normals: bool,
    capture: bool,
    capture_once: Option<PathBuf>,
    captured: bool,
    failure: Option<String>,
    status: String,
    gpu_ms: Option<f64>,
    frames: u64,
    last_timer: Instant,
    profile: Option<FrameProfile>,
    profile_execution: Option<GpuFieldExecution>,
    profile_phase: &'static str,
    event_ms: f64,
}

struct FrameProfile {
    output: BufWriter<File>,
    frame_limit: u64,
    last_redraw: Option<Instant>,
}

impl App {
    fn new(path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let mut scene = LabScene::load(&path)?;
        let capture_once = std::env::var_os("GENESIS_WORLD_LAB_CAPTURE_ONCE").map(PathBuf::from);
        let profile = std::env::var_os("GENESIS_WORLD_LAB_PROFILE")
            .map(|path| -> Result<FrameProfile, Box<dyn Error>> {
                let frame_limit = std::env::var("GENESIS_WORLD_LAB_PROFILE_FRAMES")
                    .ok()
                    .map_or(Ok(600), |value| value.parse::<u64>())?;
                if !(32..=10_000).contains(&frame_limit) {
                    return Err("profile frame limit must be 32..=10000".into());
                }
                Ok(FrameProfile {
                    output: BufWriter::new(File::create(path)?),
                    frame_limit,
                    last_redraw: None,
                })
            })
            .transpose()?;
        let profile_scenario = std::env::var("GENESIS_WORLD_LAB_PROFILE_SCENARIO").ok();
        let profile_execution = match profile_scenario.as_deref() {
            None => None,
            Some("auto") => Some(GpuFieldExecution::Auto),
            Some("authoring") => Some(GpuFieldExecution::Authoring),
            Some("structural") => Some(GpuFieldExecution::Structural),
            Some("heterogeneous-auto") => Some(GpuFieldExecution::Auto),
            Some("heterogeneous-authoring") => Some(GpuFieldExecution::Authoring),
            Some("heterogeneous-combined") => Some(GpuFieldExecution::Combined),
            Some(_) => return Err("unknown profile scenario".into()),
        };
        if profile_execution.is_some()
            && (profile.is_none()
                || scene.world.field_instances().len() != 2
                || scene.world.field_definitions().len() != 1
                || scene.world.field_definitions()[0].source()
                    != include_bytes!("../scenes/bridge.json"))
        {
            return Err(
                "profile scenario requires the unmodified showcase scene and profile output".into(),
            );
        }
        if matches!(
            profile_scenario.as_deref(),
            Some("heterogeneous-auto" | "heterogeneous-combined" | "heterogeneous-authoring")
        ) {
            scene.world.spawn_field(
                br#"{"format_version":1,"id":"profile-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.5,0.4,0.5]}]}"#,
                Transform::new(Vec3::new(0.0, 1.2, -0.5)?, 0.7)?,
            )?;
            scene.graph_nodes += 1;
        }
        Ok(Self {
            yaw: scene.yaw,
            elevation: scene.elevation,
            distance: scene.distance,
            path,
            graphics: None,
            show_analytic: scene.show_analytic,
            show_fields: scene.show_fields,
            diagnostic: scene.diagnostic,
            normals: scene.normals,
            capture: capture_once.is_some(),
            capture_once,
            captured: false,
            failure: None,
            status: "SCENE LOADED".into(),
            gpu_ms: None,
            frames: 0,
            last_timer: Instant::now(),
            profile,
            profile_execution,
            profile_phase: "static",
            event_ms: 0.0,
            scene,
        })
    }

    fn reload(&mut self) {
        match LabScene::load(&self.path) {
            Ok(scene) => {
                if let Some(graphics) = &self.graphics {
                    let size = graphics.window.inner_size();
                    let prepared = scene
                        .camera(scene.yaw, scene.elevation, scene.distance)
                        .and_then(|camera| {
                            Ok(graphics.renderer.draw_mixed(
                                &graphics.device,
                                &graphics.queue,
                                &scene.world,
                                camera,
                                MixedDrawOptions {
                                    draw: DrawOptions {
                                        size: [size.width, size.height],
                                        normal_debug: scene.normals,
                                        surface: None,
                                        timer: None,
                                    },
                                    policy: GraphTracePolicy::default(),
                                    show_analytic: scene.show_analytic,
                                    show_fields: scene.show_fields,
                                    diagnostic: scene.diagnostic,
                                    materials: Some(&scene.materials),
                                    simulation_seconds: 0.0,
                                    background: scene.background,
                                    light_direction: scene.light_direction,
                                },
                            )?)
                        });
                    if let Err(error) = prepared {
                        self.status = format!("RELOAD GPU PREP ERROR: {error}");
                        eprintln!("World Lab: {}; previous scene retained", self.status);
                        return;
                    }
                }
                self.yaw = scene.yaw;
                self.elevation = scene.elevation;
                self.distance = scene.distance;
                self.show_analytic = scene.show_analytic;
                self.show_fields = scene.show_fields;
                self.diagnostic = scene.diagnostic;
                self.normals = scene.normals;
                self.scene = scene;
                self.status = "RELOAD OK".into();
                eprintln!("World Lab: reload accepted");
            }
            Err(error) => {
                self.status = format!("RELOAD ERROR: {error}");
                eprintln!("World Lab: reload rejected; previous scene retained: {error}");
            }
        }
    }

    fn profile_tick(&mut self) -> Result<(), Box<dyn Error>> {
        if self.profile_execution.is_none() {
            return Ok(());
        }
        let frame = self.frames;
        let first = self.scene.world.field_instances()[0].id();
        self.profile_phase = match frame {
            0..100 => "static",
            100..200 => {
                self.yaw += 0.008;
                "camera"
            }
            200..300 => {
                let color = if frame.is_multiple_of(2) {
                    [0.1, 0.7, 0.9]
                } else {
                    [0.9, 0.3, 0.1]
                };
                self.scene.materials.set_field(
                    first,
                    Material::new(color, color, [0.0; 3], MaterialPattern::Constant)?,
                );
                "material"
            }
            300..400 => {
                let x = -1.6 + ((frame - 300) as f64 * 0.05).sin() * 0.2;
                self.scene
                    .world
                    .set_field_transform(first, Transform::new(Vec3::new(x, 0.0, 0.0)?, 0.9)?)?;
                "transform"
            }
            400..550 => {
                let interval = if frame < 500 { 5 } else { 25 };
                if frame.is_multiple_of(interval) {
                    let radius = 0.65 + (frame - 400) as f64 * 0.002;
                    let mut source: serde_json::Value =
                        serde_json::from_slice(include_bytes!("../scenes/bridge.json"))?;
                    source["nodes"][0]["radius"] = serde_json::json!(radius);
                    let source = serde_json::to_vec(&source)?;
                    for id in self
                        .scene
                        .world
                        .field_instances()
                        .iter()
                        .map(|item| item.id())
                        .take(2)
                        .collect::<Vec<_>>()
                    {
                        self.scene.world.replace_field(id, &source)?;
                    }
                }
                if frame < 500 {
                    "rapid_revision"
                } else {
                    if self.scene.world.field_instances().len() == 3 && frame == 525 {
                        let id = self.scene.world.field_instances()[2].id();
                        self.scene.world.replace_field(id, br#"{"format_version":1,"id":"profile-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.5}]}"#)?;
                    }
                    if self.scene.world.field_instances().len() == 3 && frame == 545 {
                        let id = self.scene.world.field_instances()[2].id();
                        self.scene.world.replace_field(id, br#"{"format_version":1,"id":"profile-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.5,0.4,0.5]}]}"#)?;
                    }
                    "slow_revision"
                }
            }
            _ => {
                if frame == 550 {
                    let revision = self.scene.world.field_instances()[0].revision();
                    if self
                        .scene
                        .world
                        .replace_field(first, b"invalid profile edit")
                        .is_ok()
                        || self.scene.world.field_instances()[0].revision() != revision
                    {
                        return Err("invalid profile edit changed the world".into());
                    }
                }
                "invalid_edit_rollback"
            }
        };
        Ok(())
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let redraw_started = Instant::now();
        let interval_ms = self.profile.as_mut().and_then(|profile| {
            let previous = profile.last_redraw.replace(redraw_started);
            previous.map(|time| redraw_started.duration_since(time).as_secs_f64() * 1000.0)
        });
        if let Err(error) = self.profile_tick() {
            self.failure = Some(format!("profile scenario failed: {error}"));
            event_loop.exit();
            return;
        }
        let profile_update_ms = redraw_started.elapsed().as_secs_f64() * 1000.0;
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        let size = graphics.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        if size.width != graphics.config.width || size.height != graphics.config.height {
            graphics.resize(size.width, size.height);
        }
        let camera = match self.scene.camera(self.yaw, self.elevation, self.distance) {
            Ok(camera) => camera,
            Err(error) => {
                eprintln!("camera error: {error}");
                self.failure = Some(format!("camera error: {error}"));
                event_loop.exit();
                return;
            }
        };
        let acquire_started = Instant::now();
        let frame = match graphics.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                graphics.resize(size.width, size.height);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("surface validation error");
                self.failure = Some("surface validation error".into());
                event_loop.exit();
                return;
            }
        };
        let acquire_ms = acquire_started.elapsed().as_secs_f64() * 1000.0;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sample_timer = self.last_timer.elapsed() >= Duration::from_secs(2);
        let draw_started = Instant::now();
        let rendered = graphics.renderer.draw_mixed_with_execution(
            &graphics.device,
            &graphics.queue,
            &self.scene.world,
            camera,
            MixedDrawOptions {
                draw: DrawOptions {
                    size: [size.width, size.height],
                    normal_debug: self.normals,
                    surface: Some(&view),
                    timer: if sample_timer {
                        graphics.timer.as_ref()
                    } else {
                        None
                    },
                },
                policy: GraphTracePolicy::default(),
                show_analytic: self.show_analytic,
                show_fields: self.show_fields,
                diagnostic: self.diagnostic,
                materials: Some(&self.scene.materials),
                simulation_seconds: 0.0,
                background: self.scene.background,
                light_direction: self.scene.light_direction,
            },
            GpuFieldRenderMode {
                execution: self
                    .profile_execution
                    .unwrap_or(GpuFieldExecution::Authoring),
                spatial_guard: true,
            },
        );
        let image = match rendered {
            Ok(image) => image,
            Err(error) => {
                eprintln!("render failed: {error}");
                self.failure = Some(format!("render failed: {error}"));
                event_loop.exit();
                return;
            }
        };
        let draw_ms = draw_started.elapsed().as_secs_f64() * 1000.0;
        if self.capture {
            self.capture = false;
            let output = self
                .capture_once
                .clone()
                .unwrap_or_else(|| PathBuf::from("world-lab-capture.bmp"));
            match capture_bmp(
                &graphics.device,
                &graphics.queue,
                &image,
                [size.width, size.height],
                &output,
            ) {
                Ok(()) => {
                    self.captured = true;
                    self.status = format!("CAPTURED {}", output.display());
                    eprintln!("World Lab: {}", self.status);
                }
                Err(error) => {
                    self.status = format!("CAPTURE FAILED: {error}");
                    if self.capture_once.is_some() {
                        self.failure = Some(self.status.clone());
                    }
                    eprintln!("World Lab: {}", self.status);
                }
            }
        }
        let readback_started = Instant::now();
        if sample_timer {
            self.last_timer = Instant::now();
            if let Some(timer) = &graphics.timer {
                self.gpu_ms = timer.read_ms(&graphics.device, &graphics.queue).ok();
            }
        }
        let gpu_readback_ms = if sample_timer {
            readback_started.elapsed().as_secs_f64() * 1000.0
        } else {
            0.0
        };
        let compiler = graphics.renderer.compiler_stats();
        let field_path = match compiler.as_ref() {
            Some(state) if state.active == GpuFieldPath::Specialized => "COMPILED",
            Some(state) if state.active == GpuFieldPath::Structural => "STRUCTURAL",
            Some(state) if state.active == GpuFieldPath::Combined => "COMBINED",
            Some(state) if state.pending => "INTERPRETER/COMPILING",
            Some(state) if state.last_error.is_some() => "INTERPRETER/COMPILE ERROR",
            _ => "INTERPRETER",
        };
        let lines = [
            format!("{} | GENESIS WORLD LAB", self.scene.name),
            format!(
                "DEFS {} INST {} NODES {} | {}X{}",
                self.scene.world.field_definitions().len(),
                self.scene.world.field_instances().len() + self.scene.world.entities().len(),
                self.scene.graph_nodes,
                size.width,
                size.height
            ),
            format!(
                "A {} F {} DIAG {} NORMAL {} | GPU {} | FIELD {}",
                self.show_analytic as u8,
                self.show_fields as u8,
                self.diagnostic as u8,
                self.normals as u8,
                self.gpu_ms.map_or("N/A".into(), |x| format!("{x:.2}MS")),
                field_path,
            ),
            self.status.clone(),
            "ARROWS OR WASD ORBIT | +/- ZOOM | 1/2/3/N | R RELOAD | C CAPTURE | ESC".into(),
        ];
        let hud_started = Instant::now();
        graphics
            .hud
            .draw(&graphics.device, &graphics.queue, &view, &lines);
        let hud_ms = hud_started.elapsed().as_secs_f64() * 1000.0;
        let present_started = Instant::now();
        graphics.queue.present(frame);
        let present_ms = present_started.elapsed().as_secs_f64() * 1000.0;
        self.frames += 1;
        if let Some(profile) = self.profile.as_mut() {
            let compiler = graphics.renderer.compiler_stats();
            let draw = graphics.renderer.last_draw_stats();
            let fields = graphics.renderer.field_cache_stats();
            let record = serde_json::json!({
                "frame": self.frames, "interval_ms": interval_ms,
                "phase": self.profile_phase, "profile_update_ms": profile_update_ms,
                "event_ms": std::mem::take(&mut self.event_ms),
                "acquire_ms": acquire_ms, "draw_ms": draw_ms,
                "world_snapshot_ms": draw.map(|x| x.scene_ms),
                "analytic_binding_ms": draw.map(|x| x.analytic_binding_ms),
                "field_binding_ms": draw.map(|x| x.field_binding_ms),
                "pipeline_selection_ms": draw.map(|x| x.pipeline_selection_ms),
                "encode_submit_ms": draw.map(|x| x.encode_submit_ms),
                "field_node_enqueue_ms": fields.map(|x| x.last_node_enqueue_ms),
                "hud_ms": hud_ms, "present_ms": present_ms,
                "gpu_timestamp_sample_ms": sample_timer.then_some(self.gpu_ms).flatten(),
                "gpu_timestamp_readback_ms": gpu_readback_ms,
                "compiler_mode": field_path, "compiler_pending": compiler.as_ref().is_some_and(|x| x.pending),
                "compiler_requests": compiler.as_ref().map(|x| x.requests),
                "compiler_cache_hits": compiler.as_ref().map(|x| x.cache_hits),
                "compiler_failures": compiler.as_ref().map(|x| x.failures),
                "compiler_fallback_reason": compiler.as_ref().and_then(|x| x.fallback_reason),
                "compiler_source_ms": compiler.as_ref().and_then(|x| x.source_ms),
                "compiler_pipeline_ms": compiler.as_ref().and_then(|x| x.pipeline_ms),
                "compiler_source_bytes": compiler.as_ref().and_then(|x| x.source_bytes),
                "field_revision": self.scene.world.field_instances().first().map(|x| format!("{:?}", x.revision())),
                "present_mode": format!("{:?}", graphics.config.present_mode),
            });
            if let Err(error) = writeln!(profile.output, "{record}") {
                self.failure = Some(format!("frame profile write failed: {error}"));
                event_loop.exit();
            }
            if self.frames >= profile.frame_limit {
                if let Err(error) = profile.output.flush() {
                    self.failure = Some(format!("frame profile flush failed: {error}"));
                }
                event_loop.exit();
            }
        }
        if self.capture_once.is_some() {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title("Genesis Mathematical World Lab — Experimental")
                .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720)),
        ) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("window error: {error}");
                self.failure = Some(format!("window error: {error}"));
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Graphics::open(window)) {
            Ok(graphics) => self.graphics = Some(graphics),
            Err(error) => {
                eprintln!("GPU init failed: {error}");
                self.failure = Some(format!("GPU init failed: {error}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let event_started = Instant::now();
        let redraw = matches!(&event, WindowEvent::RedrawRequested);
        if self
            .graphics
            .as_ref()
            .is_none_or(|graphics| graphics.window.id() != id)
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(graphics) = &mut self.graphics {
                    graphics.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::ArrowLeft | KeyCode::KeyA => self.yaw -= 0.15,
                        KeyCode::ArrowRight | KeyCode::KeyD => self.yaw += 0.15,
                        KeyCode::ArrowUp | KeyCode::KeyW => {
                            self.elevation = (self.elevation + 0.1).min(1.3)
                        }
                        KeyCode::ArrowDown | KeyCode::KeyS => {
                            self.elevation = (self.elevation - 0.1).max(-1.3)
                        }
                        KeyCode::Equal | KeyCode::NumpadAdd => {
                            self.distance = (self.distance * 0.85).max(0.2)
                        }
                        KeyCode::Minus | KeyCode::NumpadSubtract => {
                            self.distance = (self.distance * 1.15).min(500.0)
                        }
                        KeyCode::Digit1 => self.show_analytic = !self.show_analytic,
                        KeyCode::Digit2 => self.show_fields = !self.show_fields,
                        KeyCode::Digit3 => self.diagnostic = !self.diagnostic,
                        KeyCode::KeyN => self.normals = !self.normals,
                        KeyCode::KeyR => self.reload(),
                        KeyCode::KeyC => self.capture = true,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if !redraw {
            self.event_ms += event_started.elapsed().as_secs_f64() * 1000.0;
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(graphics) = &self.graphics {
            graphics.window.request_redraw();
        }
    }
}

fn capture_bmp(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &wgpu::Texture,
    size: [u32; 2],
    path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    use std::sync::mpsc;
    let [width, height] = size;
    let row = width.checked_mul(4).ok_or("capture size overflow")?;
    let padded = row.div_ceil(256) * 256;
    let bytes = u64::from(padded) * u64::from(height);
    if bytes > device.limits().max_buffer_size {
        return Err("capture exceeds GPU buffer limit".into());
    }
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lab capture"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("lab capture copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = output.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = slice.get_mapped_range()?;
    let stride = width
        .checked_mul(3)
        .ok_or("capture size overflow")?
        .div_ceil(4)
        * 4;
    let data_size = stride.checked_mul(height).ok_or("capture size overflow")?;
    let mut bmp = Vec::with_capacity((54 + data_size) as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(54 + data_size).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&height.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    for y in (0..height).rev() {
        let line = &mapped[(y * padded) as usize..(y * padded + row) as usize];
        for pixel in line.chunks_exact(4) {
            bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        bmp.resize(bmp.len() + (stride - width * 3) as usize, 0);
    }
    drop(mapped);
    output.unmap();
    std::fs::write(path, bmp)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let path = match args.as_slice() {
        [] => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes/showcase.json"),
        [path] => PathBuf::from(path),
        _ => return Err("usage: world-lab [SCENE.json]".into()),
    };
    let mut app = App::new(path)?;
    eprintln!(
        "World Lab loaded {}; arrows/WASD orbit; +/- zoom; 1/2 visibility; 3 diagnostics; N normals; R reload; C BMP capture; Esc exit",
        app.scene.name
    );
    EventLoop::new()?.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    if app.capture_once.is_some() && !app.captured {
        return Err("GPU capture did not complete".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_scenario_changes_authoritative_revision_and_rejects_invalid_edit() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes/showcase.json");
        let mut app = App::new(path).unwrap();
        app.profile_execution = Some(GpuFieldExecution::Structural);
        let first = app.scene.world.field_instances()[0].id();
        let initial_revision = app.scene.world.field_instances()[0].revision();
        app.frames = 200;
        app.profile_tick().unwrap();
        app.frames = 300;
        app.profile_tick().unwrap();
        app.frames = 400;
        app.profile_tick().unwrap();
        let changed_revision = app.scene.world.field_instances()[0].revision();
        assert_ne!(initial_revision, changed_revision);
        assert_eq!(
            app.scene.world.field_instances()[1].revision(),
            changed_revision
        );
        assert!(app.scene.world.field_graph(first).is_some());
        app.frames = 550;
        app.profile_tick().unwrap();
        assert_eq!(
            app.scene.world.field_instances()[0].revision(),
            changed_revision
        );
        assert_eq!(app.profile_phase, "invalid_edit_rollback");
    }

    #[test]
    fn heterogeneous_profile_replaces_and_restores_one_topology() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes/showcase.json");
        let mut app = App::new(path).unwrap();
        app.profile_execution = Some(GpuFieldExecution::Combined);
        let third = app
            .scene
            .world
            .spawn_field(
                br#"{"format_version":1,"id":"profile-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.5,0.4,0.5]}]}"#,
                Transform::identity(),
            )
            .unwrap();
        let original = app.scene.world.field_instances()[2].revision();
        app.frames = 525;
        app.profile_tick().unwrap();
        assert_ne!(app.scene.world.field_instances()[2].revision(), original);
        app.frames = 545;
        app.profile_tick().unwrap();
        assert_eq!(app.scene.world.field_instances()[2].revision(), original);
        assert_eq!(app.scene.world.field_instances()[2].id(), third);
        app.frames = 550;
        app.profile_tick().unwrap();
        assert_eq!(app.profile_phase, "invalid_edit_rollback");
    }
    #[test]
    fn external_edit_reload_and_rejection_preserve_scene() {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/world-lab-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes");
        let path = folder.join("showcase.json");
        std::fs::copy(original.join("showcase.json"), &path).unwrap();
        std::fs::copy(original.join("bridge.json"), folder.join("bridge.json")).unwrap();
        let mut app = App::new(path.clone()).unwrap();
        let before = app.scene.world.entities()[0].sphere().radius();
        let modified = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"radius\": 0.56", "\"radius\": 0.70");
        std::fs::write(&path, modified).unwrap();
        app.reload();
        assert_eq!(app.scene.world.entities()[0].sphere().radius(), 0.70);
        assert_ne!(before, 0.70);
        let mut wrong_revision: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        wrong_revision["definitions"][0]["revision"] = serde_json::Value::String("00".into());
        std::fs::write(&path, serde_json::to_vec(&wrong_revision).unwrap()).unwrap();
        app.reload();
        assert_eq!(app.scene.world.entities()[0].sphere().radius(), 0.70);
        assert!(app.status.contains("revision mismatch"));
        std::fs::write(&path, b"{\"format_version\":2}").unwrap();
        app.reload();
        assert_eq!(app.scene.world.entities()[0].sphere().radius(), 0.70);
        assert!(app.status.starts_with("RELOAD ERROR"));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    #[ignore = "requires native GPU; external graph edit and real mixed-image readback"]
    fn external_graph_edit_changes_actual_gpu_image() {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/world-lab-gpu-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes");
        let path = folder.join("showcase.json");
        let graph = folder.join("bridge.json");
        std::fs::copy(original.join("showcase.json"), &path).unwrap();
        std::fs::copy(original.join("bridge.json"), &graph).unwrap();
        let before = LabScene::load(&path).unwrap();
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let renderer =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let size = [256, 144];
        let draw = |scene: &LabScene, output: &std::path::Path| {
            let camera = scene
                .camera(scene.yaw, scene.elevation, scene.distance)
                .unwrap();
            let image = renderer
                .draw_mixed(
                    &device,
                    &queue,
                    &scene.world,
                    camera,
                    MixedDrawOptions {
                        draw: DrawOptions {
                            size,
                            normal_debug: false,
                            surface: None,
                            timer: None,
                        },
                        policy: GraphTracePolicy::default(),
                        show_analytic: true,
                        show_fields: true,
                        diagnostic: false,
                        materials: Some(&scene.materials),
                        simulation_seconds: 0.0,
                        background: scene.background,
                        light_direction: scene.light_direction,
                    },
                )
                .unwrap();
            capture_bmp(&device, &queue, &image, size, output).unwrap();
        };
        let first = folder.join("before.bmp");
        draw(&before, &first);
        for _ in 0..200 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            draw(&before, &first);
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Specialized
        );
        let uploads = renderer.field_cache_stats().unwrap().node_uploads;
        let changed = std::fs::read_to_string(&graph)
            .unwrap()
            .replace("\"radius\": 0.8", "\"radius\": 1.1");
        std::fs::write(&graph, changed).unwrap();
        let after = LabScene::load(&path).unwrap();
        assert_ne!(
            before.world.field_definitions()[0].revision(),
            after.world.field_definitions()[0].revision()
        );
        let second = folder.join("after.bmp");
        draw(&after, &second);
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Interpreter
        );
        for _ in 0..200 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            draw(&after, &second);
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Specialized
        );
        assert!(renderer.field_cache_stats().unwrap().node_uploads > uploads);
        assert_ne!(
            std::fs::read(first).unwrap(),
            std::fs::read(&second).unwrap()
        );
        std::fs::write(&graph, b"invalid graph").unwrap();
        assert!(LabScene::load(&path).is_err());
        draw(&after, &second);
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Specialized
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
}
