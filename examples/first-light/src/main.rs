//! Native First Light viewport. World simulation advances in fixed steps; rendering samples it.

mod hud;
mod passage;

use analytic_field::{AxisAlignedBox, Capsule, Sphere};
use analytic_renderer::{
    BranchVisibility, Camera, DrawOptions, GpuRenderer, GpuTimer, PickOutcome, Primitive, Scene,
    SemanticTarget,
};
use spatial_math::{Transform, Vec3};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
use world_authoring::{SegmentReference, Structure, compile_json, read_source_file};
use world_simulation::{
    EnvironmentEvent, EnvironmentEventKind, SimulationStep, SimulationTime, advance,
    advance_life_cached, contact::ContactScene,
};
use world_state::{DeterministicSeed, EntityId, GrowthParameters, WorldState};

const STEP: Duration = Duration::from_nanos(16_666_667);

fn median_p95(samples: &[f64]) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    (
        sorted[sorted.len() / 2],
        sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)],
    )
}

fn p99(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted
        .get((sorted.len() * 99 / 100).min(sorted.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0.0)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite scene coordinate")
}

struct AuthoredPreview {
    path: PathBuf,
    structure: Structure,
    scene: Scene,
    target: Vec3,
    distance: f64,
    generation: u64,
    selection: Option<PreviewSelection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewSelection {
    generation: u64,
    segment: SegmentReference,
}

impl AuthoredPreview {
    fn from_structure(
        path: PathBuf,
        structure: Structure,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut scene = Scene::default();
        let mut lower = [f64::INFINITY; 3];
        let mut upper = [f64::NEG_INFINITY; 3];
        for segment in structure.segments() {
            let capsule = Capsule::new(segment.start, segment.end, segment.radius)?;
            scene.push(Primitive::capsule(segment.id, capsule, [0.30, 0.82, 0.52])?)?;
            for point in [segment.start, segment.end] {
                for (index, value) in [point.x(), point.y(), point.z()].into_iter().enumerate() {
                    lower[index] = lower[index].min(value);
                    upper[index] = upper[index].max(value);
                }
            }
        }
        let extent = (0..3).map(|i| upper[i] - lower[i]).fold(0.0, f64::max);
        if extent > 400.0 {
            return Err("structure exceeds preview camera range (400 world units)".into());
        }
        let target = Vec3::new(
            (lower[0] + upper[0]) / 2.0,
            (lower[1] + upper[1]) / 2.0,
            (lower[2] + upper[2]) / 2.0,
        )?;
        Ok(Self {
            path,
            structure,
            scene,
            target,
            distance: (extent * 2.0).max(7.0),
            generation: 1,
            selection: None,
        })
    }

    fn load(path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let structure = compile_json(&read_source_file(&path)?)?;
        let preview = Self::from_structure(path, structure)?;
        analytic_renderer::Bvh::build(&preview.scene)?;
        Ok(preview)
    }

    fn replace(&mut self, source: &[u8]) -> Result<bool, Box<dyn std::error::Error>> {
        if source == self.structure.source() {
            return Ok(false);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("preview generation overflow")?;
        let mut next = Self::from_structure(self.path.clone(), compile_json(source)?)?;
        analytic_renderer::Bvh::build(&next.scene)?;
        next.generation = generation;
        *self = next;
        Ok(true)
    }

    fn reload(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        self.replace(&read_source_file(&self.path)?)
    }

    fn select(&mut self, draw_id: u32) {
        self.selection = self
            .structure
            .reference(draw_id)
            .map(|segment| PreviewSelection {
                generation: self.generation,
                segment,
            });
    }

    fn resolve(&self, selected: PreviewSelection) -> bool {
        selected.generation == self.generation && self.structure.segment(selected.segment).is_some()
    }

    fn print_summary(&self) {
        println!(
            "{}",
            serde_json::json!({
                "kind": "authored-structure",
                "id": self.structure.id(),
                "source_format_version": self.structure.format_version(),
                "revision": self.structure.revision(),
                "content_sha256": self.structure.content_revision().hex(),
                "compiler_semantics_version": world_authoring::COMPILER_SEMANTICS_VERSION,
                "preview_generation": self.generation,
                "expanded_symbols": self.structure.expanded_symbols(),
                "segments": self.structure.segments().len(),
                "max_stack": self.structure.max_stack_used(),
                "work": self.structure.work()
            })
        );
    }
}

struct Graphics {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuRenderer,
    timer: Option<GpuTimer>,
    hud: Option<hud::Hud>,
    surface_retries: u8,
}

enum SurfaceFrame {
    Ready(wgpu::SurfaceTexture, bool),
    Reconfigure,
    Skip,
    Fatal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchmarkScenario {
    Idle,
    Growth,
    Interaction,
}

impl BenchmarkScenario {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "growth" => Some(Self::Growth),
            "interaction" => Some(Self::Interaction),
            _ => None,
        }
    }
}

fn classify_surface(result: wgpu::CurrentSurfaceTexture) -> SurfaceFrame {
    match result {
        wgpu::CurrentSurfaceTexture::Success(frame) => SurfaceFrame::Ready(frame, false),
        wgpu::CurrentSurfaceTexture::Suboptimal(frame) => SurfaceFrame::Ready(frame, true),
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            SurfaceFrame::Reconfigure
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            SurfaceFrame::Skip
        }
        wgpu::CurrentSurfaceTexture::Validation => SurfaceFrame::Fatal,
    }
}

fn drawable_size(width: u32, height: u32, max_dimension: u32) -> bool {
    width > 0 && height > 0 && width <= max_dimension && height <= max_dimension
}

fn requested_window_size(value: &str) -> Option<winit::dpi::PhysicalSize<u32>> {
    let (width, height) = value.split_once('x')?;
    let width: u32 = width.parse().ok()?;
    let height: u32 = height.parse().ok()?;
    drawable_size(width, height, 16_384).then_some(winit::dpi::PhysicalSize::new(width, height))
}

fn next_surface_retry(previous: u8) -> Option<u8> {
    previous.checked_add(1).filter(|attempt| *attempt <= 3)
}

impl Graphics {
    async fn open(
        window: Arc<Window>,
        lost: Arc<AtomicBool>,
        passage: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("No compatible Vulkan or DirectX 12 GPU adapter: {error}"))?;
        let info = adapter.get_info();
        eprintln!(
            "adapter={} backend={:?} driver={} wgpu=30.0.1",
            info.name, info.backend, info.driver
        );
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("First Light"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("GPU device initialization failed: {error}"))?;
        let uncaptured_error = Arc::clone(&lost);
        device.on_uncaptured_error(Arc::new(move |error| {
            eprintln!("GPU error: {error}");
            uncaptured_error.store(true, Ordering::Relaxed);
        }));
        let device_lost = Arc::clone(&lost);
        device.set_device_lost_callback(move |reason, message| {
            eprintln!("GPU device lost: {reason:?}: {message}");
            device_lost.store(true, Ordering::Relaxed);
        });
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or("surface has no supported format")?;
        let size = window.inner_size();
        if size.width > device.limits().max_texture_dimension_2d
            || size.height > device.limits().max_texture_dimension_2d
        {
            return Err("initial window exceeds GPU texture limit".into());
        }
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface configuration unavailable")?;
        let config = wgpu::SurfaceConfiguration { format, ..config };
        surface.configure(&device, &config);
        let renderer = GpuRenderer::new(&device, format).await?;
        let timer = GpuTimer::new(&device);
        let hud = passage.then(|| hud::Hud::new(&device, format));
        Ok(Self {
            window,
            surface,
            adapter,
            device,
            queue,
            config,
            renderer,
            timer,
            hud,
            surface_retries: 0,
        })
    }
    fn configure_surface(&mut self, width: u32, height: u32, force: bool) -> bool {
        if !drawable_size(width, height, self.device.limits().max_texture_dimension_2d) {
            eprintln!("resize skipped: {width}x{height} outside drawable GPU limits");
            return false;
        }
        if force || self.config.width != width || self.config.height != height {
            if self.config.width != width || self.config.height != height {
                self.surface_retries = 0;
            }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
        true
    }
}

struct App {
    graphics: Option<Graphics>,
    lost: Arc<AtomicBool>,
    world: WorldState,
    authored: Option<AuthoredPreview>,
    time: SimulationTime,
    step: SimulationStep,
    contact_cache: Option<ContactScene>,
    paused: bool,
    normals: bool,
    yaw: f64,
    elevation: f64,
    previous: Instant,
    accumulator: Duration,
    stats_since: Instant,
    frames: u32,
    cpu_total: Duration,
    life: bool,
    passage: bool,
    won: bool,
    focused: bool,
    status: String,
    measure: bool,
    benchmark_scenario: Option<BenchmarkScenario>,
    frame_samples: Vec<f64>,
    interval_samples: Vec<f64>,
    acquire_samples: Vec<f64>,
    submit_samples: Vec<f64>,
    present_samples: Vec<f64>,
    upload_samples: Vec<f64>,
    bvh_build_samples: Vec<f64>,
    step_samples: Vec<f64>,
    snapshot_samples: Vec<f64>,
    gpu_samples: Vec<f64>,
    source_id: Option<EntityId>,
    body_id: Option<EntityId>,
    body_keys: [bool; 4],
    body_input_dirty: bool,
    pending_move: bool,
    pending_source_toggle: bool,
    pending_prune: Option<(EntityId, u32)>,
    selected: Option<SemanticTarget>,
    cursor: Option<winit::dpi::PhysicalPosition<f64>>,
    last_camera: Option<Camera>,
}
impl App {
    fn new(life: bool, passage: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let benchmark_scenario = match std::env::var("GENESIS_MEASURE_SCENARIO") {
            Ok(value) if passage => Some(
                BenchmarkScenario::parse(&value)
                    .ok_or("GENESIS_MEASURE_SCENARIO must be idle, growth, or interaction")?,
            ),
            Ok(_) => return Err("GENESIS_MEASURE_SCENARIO requires --passage".into()),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(error.into()),
        };
        let mut world = WorldState::new(DeterministicSeed(7));
        let mut time = SimulationTime::default();
        if passage {
            let initial = passage::initial()?;
            world = initial.world().clone();
            time = initial.time();
        }
        let source_id = if passage {
            Some(world.sources()[0].id())
        } else if life {
            let parameters = GrowthParameters::new(0.14, 0.32, 0.12, 1.0)?;
            world.spawn_organism(v(-0.6, 0.0, 0.0), parameters)?;
            world.spawn_organism(v(0.6, 0.0, 0.0), parameters)?;
            Some(world.spawn_finite_source(v(0.0, 2.0, 0.0), 4.0, 10.0, 0.0, 0.02, 0.02)?)
        } else {
            world.spawn_sphere(
                Sphere::new(0.7)?,
                Transform::new(v(-1.4, 0.0, 0.0), 1.0)?,
                0.12,
            )?;
            None
        };
        let body_id = if passage {
            Some(world.body().expect("passage body").id())
        } else if life {
            Some(world.spawn_body(v(0.12, 0.32, -1.2), 0.11)?)
        } else {
            None
        };
        Ok(Self {
            graphics: None,
            lost: Arc::new(AtomicBool::new(false)),
            world,
            authored: None,
            time,
            step: SimulationStep::new(STEP.as_secs_f64())?,
            contact_cache: None,
            paused: benchmark_scenario == Some(BenchmarkScenario::Idle),
            normals: false,
            yaw: 0.0,
            elevation: 0.25,
            previous: Instant::now(),
            accumulator: Duration::ZERO,
            stats_since: Instant::now(),
            frames: 0,
            cpu_total: Duration::ZERO,
            life,
            passage,
            won: false,
            focused: true,
            status: if passage {
                "MOVE SOURCE, CUT STEM, REACH GOAL".into()
            } else {
                String::new()
            },
            measure: std::env::var_os("GENESIS_MEASURE").is_some() || benchmark_scenario.is_some(),
            benchmark_scenario,
            frame_samples: Vec::new(),
            interval_samples: Vec::new(),
            acquire_samples: Vec::new(),
            submit_samples: Vec::new(),
            present_samples: Vec::new(),
            upload_samples: Vec::new(),
            bvh_build_samples: Vec::new(),
            step_samples: Vec::new(),
            snapshot_samples: Vec::new(),
            gpu_samples: Vec::new(),
            source_id,
            body_id,
            body_keys: [false; 4],
            body_input_dirty: false,
            pending_move: false,
            pending_source_toggle: false,
            pending_prune: None,
            selected: None,
            cursor: None,
            last_camera: None,
        })
    }
    fn authoring(path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let preview = AuthoredPreview::load(path)?;
        preview.print_summary();
        let mut app = Self::new(false, false)?;
        app.world = WorldState::new(DeterministicSeed(7));
        app.paused = true;
        app.authored = Some(preview);
        Ok(app)
    }
    fn pick_cursor(&mut self) {
        let Some(graphics) = self.graphics.as_ref() else {
            return;
        };
        let Some(cursor) = self.cursor else {
            self.status = "MOVE THE CURSOR INTO THE WINDOW".into();
            eprintln!("selection: cursor unavailable");
            return;
        };
        let Some(camera) = self.last_camera else {
            self.status = "WAIT FOR THE NEXT FRAME, THEN CLICK AGAIN".into();
            eprintln!("selection: camera changed; wait for redraw");
            return;
        };
        let size = graphics.window.inner_size();
        if !cursor.x.is_finite()
            || !cursor.y.is_finite()
            || cursor.x < 0.0
            || cursor.y < 0.0
            || cursor.x >= f64::from(size.width)
            || cursor.y >= f64::from(size.height)
        {
            self.selected = None;
            if let Some(preview) = self.authored.as_mut() {
                preview.selection = None;
            }
            self.status = "CLICK INSIDE THE WINDOW".into();
            eprintln!("selection: click outside viewport");
            return;
        }
        let scene = match self.scene() {
            Ok(scene) => scene,
            Err(error) => {
                eprintln!("selection snapshot failed: {error}");
                return;
            }
        };
        if let Some(preview) = self.authored.as_mut() {
            match camera.ray(cursor.x as u32, cursor.y as u32, size.width, size.height) {
                Ok(ray) => {
                    preview.selection = None;
                    if let Some(hit) = scene.intersect(ray) {
                        preview.select(hit.id);
                        self.status = if preview
                            .selection
                            .is_some_and(|selected| preview.resolve(selected))
                        {
                            format!("SEGMENT {} SELECTED (STATIC PREVIEW)", hit.id)
                        } else {
                            "PREVIEW SELECTION UNAVAILABLE".into()
                        };
                    } else {
                        self.status = "NO AUTHORED SEGMENT AT CURSOR".into();
                    }
                }
                Err(error) => self.status = format!("PREVIEW PICK FAILED: {error}"),
            }
            return;
        }
        match scene.pick_pixel(
            camera,
            cursor.x as u32,
            cursor.y as u32,
            size.width,
            size.height,
            100.0,
        ) {
            Ok(PickOutcome::Hit(hit)) => {
                self.selected = Some(hit.target);
                let green_behind_front = self.passage
                    && camera
                        .ray(cursor.x as u32, cursor.y as u32, size.width, size.height)
                        .ok()
                        .and_then(|ray| {
                            scene
                                .branch_visibility(ray, self.world.organisms()[0].id(), 1)
                                .ok()
                        })
                        .is_some_and(|visibility| {
                            matches!(visibility, BranchVisibility::Occluded { .. })
                        });
                self.status = match passage::branch(self.selected, &self.world) {
                    Some((organism, _))
                        if self.passage && organism != self.world.organisms()[0].id() =>
                    {
                        if green_behind_front {
                            "BLUE PICKED; GREEN STEM BEHIND IT. CHANGE VIEW."
                        } else {
                            "BLUE PICKED; GREEN STEM STILL BLOCKS"
                        }
                    }
                    Some(_) => "BRANCH SELECTED. PRESS P TO CUT.",
                    None if green_behind_front => "GREEN STEM BEHIND THIS OBJECT. CHANGE VIEW.",
                    None => "ONLY GROWTH BRANCHES CAN BE CUT",
                }
                .into();
                eprintln!(
                    "selection tick={} target={:?} distance={:.5} position={:?}",
                    self.time.ticks(),
                    hit.target,
                    hit.distance,
                    hit.position
                );
            }
            Ok(PickOutcome::Miss) => {
                self.selected = None;
                self.status = "NO TARGET; USE ARROWS TO CHANGE VIEW".into();
                eprintln!("selection: miss");
            }
            Ok(PickOutcome::Ambiguous { first, second }) => {
                self.selected = None;
                self.status = "OVERLAP; LOOK AROUND AND CLICK AGAIN".into();
                eprintln!("selection: ambiguous {first:?} / {second:?}; no cut authorized");
            }
            Ok(PickOutcome::Indeterminate) => {
                self.selected = None;
                self.status = "UNCERTAIN PICK; TRY ANOTHER ANGLE".into();
                eprintln!("selection: indeterminate; no cut authorized");
            }
            Err(error) => {
                self.selected = None;
                self.status = "PICK FAILED".into();
                eprintln!("selection failed: {error}");
            }
        }
    }
    fn queue_prune(&mut self) {
        let target = match self.selected {
            Some(SemanticTarget::Connection { organism, child })
            | Some(SemanticTarget::GrowthNode {
                organism,
                node: child,
            }) => (organism, child),
            other => {
                self.status = "SELECT A NON-ROOT BRANCH FIRST".into();
                eprintln!(
                    "prune rejected: select a non-root growth node or connection, got {other:?}"
                );
                return;
            }
        };
        if target.1 == 0
            || self
                .world
                .organisms()
                .iter()
                .find(|tree| tree.id() == target.0)
                .and_then(|tree| tree.node(target.1))
                .is_none()
        {
            eprintln!("prune rejected: root or stale node");
            self.selected = None;
            self.status = "THAT BRANCH NO LONGER EXISTS".into();
            return;
        }
        if self.pending_prune.is_some() {
            self.status = "A CUT IS ALREADY QUEUED".into();
            eprintln!("prune rejected: another cut is already queued");
            return;
        }
        self.pending_prune = Some(target);
        self.status = "CUT QUEUED FOR NEXT SIMULATION TICK".into();
        eprintln!(
            "prune queued for tick={} organism={} child={}; resume if paused",
            self.time.ticks() + 1,
            target.0.value(),
            target.1
        );
    }
    fn set_body_key(&mut self, index: usize, pressed: bool) {
        if self.body_keys[index] != pressed {
            self.body_keys[index] = pressed;
            self.body_input_dirty = true;
        }
    }
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused && self.life {
            self.body_keys = [false; 4];
            self.body_input_dirty = true;
        }
    }
    fn toggle_pause(&mut self) {
        if self.won {
            return;
        }
        self.paused = !self.paused;
        self.status = if self.paused {
            "PAUSED. PRESS SPACE TO RESUME."
        } else {
            "RESUMED."
        }
        .into();
        self.accumulator = Duration::ZERO;
        self.previous = Instant::now();
    }
    fn restart_passage(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let initial = passage::initial()?;
        self.world = initial.world().clone();
        self.time = initial.time();
        self.contact_cache = None;
        self.selected = None;
        self.last_camera = None;
        self.pending_move = false;
        self.pending_source_toggle = false;
        self.pending_prune = None;
        self.body_keys = [false; 4];
        self.body_input_dirty = true;
        self.paused = false;
        self.won = false;
        self.accumulator = Duration::ZERO;
        self.previous = Instant::now();
        self.status = "RESTARTED. REACH THE MAGENTA GOAL.".into();
        Ok(())
    }
    fn scene(&self) -> Result<Scene, Box<dyn std::error::Error>> {
        if let Some(preview) = &self.authored {
            return Ok(preview.scene.clone());
        }
        let mut scene = Scene::from_world(&self.world)?;
        if self.passage {
            scene.highlight_branch(self.world.organisms()[0].id(), 1, [0.18, 1.0, 0.22]);
            if let Some((organism, child)) = passage::branch(self.selected, &self.world) {
                scene.highlight_branch(organism, child, [0.98, 0.96, 0.18]);
            }
        }
        if self.passage {
            passage::decorate(&mut scene)?;
        }
        if !self.life {
            scene.push(Primitive::axis_aligned_box(
                1,
                AxisAlignedBox::new(v(0.9, 0.9, 0.9))?,
                Transform::new(v(1.2, 0.0, 0.0), 1.0)?,
                [0.95, 0.48, 0.22],
            )?)?;
        }
        Ok(scene)
    }
    fn simulation_tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.paused {
            return Ok(());
        }
        if let Some(scenario) = self.benchmark_scenario {
            let tick = self.time.ticks().saturating_sub(45);
            match (scenario, tick) {
                (BenchmarkScenario::Growth | BenchmarkScenario::Interaction, 0) => {
                    self.pending_move = true;
                }
                (BenchmarkScenario::Interaction, 20 | 40) => self.pending_source_toggle = true,
                (BenchmarkScenario::Interaction, 60) => {
                    self.pending_prune = Some((self.world.organisms()[0].id(), 1));
                }
                (BenchmarkScenario::Interaction, 80) => self.set_body_key(0, true),
                (BenchmarkScenario::Interaction, 120) => self.set_body_key(0, false),
                (BenchmarkScenario::Interaction, 180) => {
                    self.restart_passage()?;
                    return Ok(());
                }
                _ => {}
            }
        }
        let mut events = if self.life && self.pending_move {
            let source_id = self.source_id.expect("life source exists");
            let source = self
                .world
                .sources()
                .iter()
                .find(|s| s.id() == source_id)
                .expect("life source exists");
            let position = if self.passage {
                if source.position().x() < 2.0 {
                    v(2.4, 3.7, 0.2)
                } else {
                    v(0.55, 1.1, 0.0)
                }
            } else if source.position().x() == 0.0 {
                v(2.0, 1.0, 0.0)
            } else {
                v(0.0, 2.0, 0.0)
            };
            vec![EnvironmentEvent {
                tick: self.time.ticks() + 1,
                order: 0,
                kind: EnvironmentEventKind::MoveSource {
                    id: source_id,
                    position,
                },
            }]
        } else {
            Vec::new()
        };
        if self.pending_source_toggle {
            let source = self.world.sources().first().expect("life source exists");
            events.push(EnvironmentEvent {
                tick: self.time.ticks() + 1,
                order: events.len() as u64,
                kind: EnvironmentEventKind::SetSourceActive {
                    id: source.id(),
                    active: !source.active(),
                },
            });
        }
        if let Some((organism, child)) = self.pending_prune {
            events.push(EnvironmentEvent {
                tick: self.time.ticks() + 1,
                order: events.len() as u64,
                kind: EnvironmentEventKind::PruneBranch { organism, child },
            });
        }
        if self.body_input_dirty {
            let x = f64::from(i32::from(self.body_keys[3]) - i32::from(self.body_keys[2]));
            let z = f64::from(i32::from(self.body_keys[0]) - i32::from(self.body_keys[1]));
            events.push(EnvironmentEvent {
                tick: self.time.ticks() + 1,
                order: events.len() as u64,
                kind: EnvironmentEventKind::SetBodyVelocity {
                    id: self.body_id.expect("life body exists"),
                    velocity: v(x * 2.0, 0.0, z * 2.0),
                },
            });
        }
        let previous_contact = self
            .world
            .body()
            .and_then(|body| body.contact())
            .map(|hit| hit.collider);
        let step_started = Instant::now();
        let result = if self.life {
            advance_life_cached(
                &mut self.world,
                &mut self.time,
                self.step,
                &events,
                &mut self.contact_cache,
            )
        } else {
            advance(&mut self.world, &mut self.time, self.step)
        };
        result?;
        if self.measure {
            self.step_samples
                .push(step_started.elapsed().as_secs_f64() * 1000.0);
        }
        self.pending_move = false;
        self.pending_source_toggle = false;
        if self.passage
            && events
                .iter()
                .any(|event| matches!(event.kind, EnvironmentEventKind::SetSourceActive { .. }))
        {
            self.status = if self.world.sources()[0].active() {
                "SOURCE ON. GROWTH RESUMES."
            } else {
                "SOURCE OFF. RESOURCE INPUT PAUSED."
            }
            .into();
        }
        if self.passage
            && events
                .iter()
                .any(|event| matches!(event.kind, EnvironmentEventKind::MoveSource { .. }))
        {
            self.status = if self.world.sources()[0].position().x() > 2.0 {
                "SOURCE AWAY. BLUE GROWTH CONTINUES.".into()
            } else {
                "SOURCE RETURNED. GREEN STEM CAN REGROW.".into()
            };
        }
        self.body_input_dirty = false;
        let contact = self
            .world
            .body()
            .and_then(|body| body.contact())
            .map(|hit| hit.collider);
        if contact != previous_contact && contact.is_some() {
            if self.passage {
                self.status = "BLOCKED. LOOK AROUND; CUT GREEN STEM.".into();
            }
            eprintln!(
                "contact tick={} {:?}",
                self.time.ticks(),
                self.world.body().and_then(|body| body.contact())
            );
        }
        if let Some((organism, child)) = self.pending_prune.take() {
            self.selected = None;
            self.status = if self.passage && organism != self.world.organisms()[0].id() {
                "BLUE CUT; GREEN STEM STILL BLOCKS".into()
            } else if self.passage && self.world.organisms()[0].node(1).is_some() {
                "UPPER BRANCH CUT. CUT LOWER GREEN STEM.".into()
            } else {
                "STEM CLEARED. REACH MAGENTA GOAL.".into()
            };
            eprintln!(
                "pruned tick={} organism={} child={}",
                self.time.ticks(),
                organism.value(),
                child
            );
        }
        if self.passage && passage::goal_reached(&self.world) {
            self.won = true;
            self.paused = true;
            self.status = "PASSAGE COMPLETE! PRESS R TO RESTART.".into();
        }
        Ok(())
    }
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let frame_started = Instant::now();
        if self.lost.load(Ordering::Relaxed) {
            eprintln!("GPU device error; exiting");
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(self.previous);
        self.previous = now;
        if self.measure && self.frames > 0 {
            self.interval_samples.push(elapsed.as_secs_f64() * 1000.0);
        }
        if !self.paused {
            self.accumulator += elapsed.min(Duration::from_millis(250));
            let mut steps = 0;
            while self.accumulator >= STEP && steps < 4 {
                if let Err(error) = self.simulation_tick() {
                    eprintln!("simulation failed: {error}");
                    event_loop.exit();
                    return;
                }
                if self.accumulator < STEP {
                    break;
                }
                self.accumulator -= STEP;
                steps += 1;
                if self.won {
                    break;
                }
            }
            // shortcut: drop excess wall-time debt after four fixed steps; use a separate simulation worker if sustained frame times exceed 67 ms.
            self.accumulator = self.accumulator.min(STEP * 4);
        }
        let snapshot_started = Instant::now();
        let scene = match self.scene() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("scene failed: {e}");
                event_loop.exit();
                return;
            }
        };
        if self.measure {
            self.snapshot_samples
                .push(snapshot_started.elapsed().as_secs_f64() * 1000.0);
        }
        let hud_lines = if self.passage {
            let selected = match self.selected {
                Some(SemanticTarget::Connection { organism, child }) => {
                    format!("BRANCH {}:{}", organism.value(), child)
                }
                Some(SemanticTarget::GrowthNode { organism, node }) => {
                    format!("NODE {}:{}", organism.value(), node)
                }
                Some(SemanticTarget::Source(_)) => "RESOURCE SOURCE".into(),
                Some(SemanticTarget::Body(_)) => "PLAYER".into(),
                Some(SemanticTarget::Sphere(_)) => "WALL".into(),
                None => "NONE".into(),
            };
            Some([
                "ORANGE PLAYER -> MAGENTA GOAL".into(),
                "WASD MOVE | ARROWS LOOK | CLICK GREEN STEM".into(),
                "M MOVE | O SWITCH | P CUT | SPACE PAUSE | R RESTART".into(),
                format!(
                    "SOURCE {} {} | STEM {} | SEL {selected}",
                    if self.world.sources()[0].position().x() > 2.0 {
                        "AWAY"
                    } else {
                        "NEAR"
                    },
                    if self.world.sources()[0].active() {
                        "ON"
                    } else {
                        "OFF"
                    },
                    if self.world.organisms()[0].node(1).is_some() {
                        "BLOCKS"
                    } else {
                        "CLEAR"
                    }
                ),
                format!(
                    "{}: {}",
                    if self.won {
                        "SUCCESS"
                    } else if self.paused {
                        "PAUSED"
                    } else {
                        "STATUS"
                    },
                    self.status
                ),
            ])
        } else {
            None
        };
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        let size = graphics.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        if !graphics.configure_surface(size.width, size.height, false) {
            return;
        }
        let height = if self.life {
            self.world
                .organisms()
                .iter()
                .flat_map(|organism| organism.nodes())
                .map(|node| node.position().y())
                .fold(2.0, f64::max)
        } else {
            0.0
        };
        let target = if let Some(preview) = &self.authored {
            preview.target
        } else if self.passage {
            v(0.0, 0.55, 0.0)
        } else {
            v(0.0, height / 2.0, 0.0)
        };
        let distance = if let Some(preview) = &self.authored {
            preview.distance
        } else if self.passage {
            5.3
        } else {
            (height * 1.1).max(7.0)
        };
        let origin = v(
            target.x() + self.yaw.sin() * distance,
            target.y() + self.elevation.sin() * distance,
            target.z() - self.yaw.cos() * self.elevation.cos() * distance,
        );
        let camera = match Camera::look_at(origin, target, v(0.0, 1.0, 0.0), 0.95) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("camera failed: {e}");
                event_loop.exit();
                return;
            }
        };
        self.last_camera = None;
        let ecology = if self.life {
            let counts: Vec<_> = self
                .world
                .organisms()
                .iter()
                .map(|organism| organism.nodes().len())
                .collect();
            let stock = self.world.sources()[0]
                .reservoir()
                .expect("finite life source");
            format!(
                " — nodes {counts:?} — resource {:.3} (allocated {:.3})",
                stock.stored(),
                stock.last_allocated()
            )
        } else {
            String::new()
        };
        graphics.window.set_title(&format!(
            "{} — tick {}{} — selected {:?} — contact {:?}{}",
            if self.passage {
                "The Passage"
            } else if self.authored.is_some() {
                "Genesis Authoring Preview"
            } else if self.life {
                "First Life"
            } else {
                "First Light"
            },
            self.time.ticks(),
            ecology,
            self.selected,
            self.world
                .body()
                .and_then(|body| body.contact())
                .map(|hit| hit.collider),
            if self.pending_prune.is_some() {
                " — prune queued"
            } else {
                ""
            }
        ));
        let acquire_started = Instant::now();
        let (frame, reconfigure_after) =
            match classify_surface(graphics.surface.get_current_texture()) {
                SurfaceFrame::Ready(frame, reconfigure_after) => {
                    graphics.surface_retries = 0;
                    (frame, reconfigure_after)
                }
                SurfaceFrame::Reconfigure => {
                    let Some(attempt) = next_surface_retry(graphics.surface_retries) else {
                        eprintln!("surface recovery failed after three reconfigurations; exiting");
                        event_loop.exit();
                        return;
                    };
                    graphics.surface_retries = attempt;
                    eprintln!("surface reconfiguration attempt {attempt}");
                    graphics.configure_surface(size.width, size.height, true);
                    return;
                }
                SurfaceFrame::Skip => return,
                SurfaceFrame::Fatal => {
                    eprintln!("surface validation failed; exiting");
                    event_loop.exit();
                    return;
                }
            };
        if self.measure {
            self.acquire_samples
                .push(acquire_started.elapsed().as_secs_f64() * 1000.0);
        }
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let started = Instant::now();
        let report = now.duration_since(self.stats_since) >= Duration::from_secs(2);
        // Sparse native timestamp readback limits measurement-induced frame stalls.
        let sampled_timer = if report || (self.measure && self.frames.is_multiple_of(30)) {
            graphics.timer.as_ref()
        } else {
            None
        };
        if let Err(error) = graphics.renderer.draw(
            &graphics.device,
            &graphics.queue,
            &scene,
            camera,
            DrawOptions {
                size: [size.width, size.height],
                normal_debug: self.normals,
                surface: Some(&view),
                timer: sampled_timer,
            },
        ) {
            eprintln!("render failed: {error}");
            event_loop.exit();
            return;
        }
        if self.authored.is_some()
            && let Some(upload) = graphics.renderer.last_upload_stats()
            && upload.bvh_build_ms > 0.0
        {
            let acceleration = graphics.renderer.acceleration_stats();
            eprintln!(
                "{}",
                serde_json::json!({
                    "kind": "authored-gpu-build",
                    "primitives": scene.primitives().len(),
                    "bvh_build_ms": upload.bvh_build_ms,
                    "upload_enqueue_ms": upload.enqueue_ms,
                    "primitive_bytes": upload.primitive_bytes,
                    "bvh_bytes": upload.bvh_bytes,
                    "bvh_nodes": acceleration.map(|item| item.node_count),
                    "bvh_depth": acceleration.map(|item| item.depth)
                })
            );
        }
        if self.measure {
            self.submit_samples
                .push(started.elapsed().as_secs_f64() * 1000.0);
            if let Some(upload) = graphics.renderer.last_upload_stats() {
                self.upload_samples.push(upload.enqueue_ms);
                self.bvh_build_samples.push(upload.bvh_build_ms);
            }
        }
        if let (Some(hud), Some(lines)) = (&graphics.hud, &hud_lines) {
            hud.draw(&graphics.device, &graphics.queue, &view, lines);
        }
        let present_started = Instant::now();
        graphics.queue.present(frame);
        if self.measure {
            self.present_samples
                .push(present_started.elapsed().as_secs_f64() * 1000.0);
            self.frame_samples
                .push(frame_started.elapsed().as_secs_f64() * 1000.0);
            if let Some(timer) = sampled_timer {
                match timer.read_ms(&graphics.device, &graphics.queue) {
                    Ok(ms) => self.gpu_samples.push(ms),
                    Err(error) => eprintln!("GPU timing unavailable: {error}"),
                }
            }
        }
        self.last_camera = Some(camera);
        self.cpu_total += started.elapsed();
        self.frames += 1;
        if report {
            let seconds = now.duration_since(self.stats_since).as_secs_f64();
            let gpu_time = if self.measure {
                Ok(None)
            } else {
                graphics
                    .timer
                    .as_ref()
                    .map(|timer| {
                        timer
                            .read_ms(&graphics.device, &graphics.queue)
                            .map(|ms| format!("{ms:.3}"))
                    })
                    .transpose()
            };
            let gpu_time = match gpu_time {
                Ok(Some(ms)) => ms,
                Ok(None) => "unavailable".to_string(),
                Err(error) => format!("error:{error}"),
            };
            let growth = if let Some(preview) = &self.authored {
                format!(
                    "authored={} segments={}",
                    preview.structure.id(),
                    scene.primitives().len()
                )
            } else if self.life {
                format!(
                    "nodes={} active={} bifurcations={}",
                    self.world
                        .organisms()
                        .iter()
                        .map(|organism| organism.nodes().len())
                        .sum::<usize>(),
                    self.world
                        .organisms()
                        .iter()
                        .map(|organism| organism.active_count())
                        .sum::<usize>(),
                    self.world
                        .organisms()
                        .iter()
                        .map(|organism| organism.branch_count())
                        .sum::<usize>()
                )
            } else {
                format!("radius={:.4}", self.world.entities()[0].sphere().radius())
            };
            eprintln!(
                "fps={:.1} cpu_submit_ms={:.2} gpu_compute_ms={} resolution={}x{} objects={} max_tests_per_frame={} ticks={} {} backend={:?}",
                f64::from(self.frames) / seconds,
                self.cpu_total.as_secs_f64() * 1000.0 / f64::from(self.frames),
                gpu_time,
                size.width,
                size.height,
                scene.primitives().len(),
                u64::from(size.width) * u64::from(size.height) * scene.primitives().len() as u64,
                self.time.ticks(),
                growth,
                graphics.adapter.get_info().backend
            );
            if self.measure {
                let (frame_p50, frame_p95) = median_p95(&self.frame_samples);
                let (step_p50, step_p95) = median_p95(&self.step_samples);
                let (snapshot_p50, snapshot_p95) = median_p95(&self.snapshot_samples);
                let (gpu_p50, gpu_p95) = median_p95(&self.gpu_samples);
                let (interval_p50, interval_p95) = median_p95(&self.interval_samples);
                let (acquire_p50, acquire_p95) = median_p95(&self.acquire_samples);
                let (submit_p50, submit_p95) = median_p95(&self.submit_samples);
                let (present_p50, present_p95) = median_p95(&self.present_samples);
                let (upload_p50, upload_p95) = median_p95(&self.upload_samples);
                let (build_p50, build_p95) = median_p95(&self.bvh_build_samples);
                let acceleration = graphics.renderer.acceleration_stats();
                let upload = graphics.renderer.last_upload_stats();
                eprintln!(
                    "passage_measure resolution={}x{} frame_n={} frame_ms_p50={frame_p50:.3} frame_ms_p95={frame_p95:.3} step_n={} step_ms_p50={step_p50:.3} step_ms_p95={step_p95:.3} snapshot_ms_p50={snapshot_p50:.3} snapshot_ms_p95={snapshot_p95:.3} gpu_n={} gpu_ms_p50={gpu_p50:.3} gpu_ms_p95={gpu_p95:.3} bvh_rebuilds={:?} last_bvh_build_ms={:?} last_upload_enqueue_ms={:?}",
                    size.width,
                    size.height,
                    self.frame_samples.len(),
                    self.step_samples.len(),
                    self.gpu_samples.len(),
                    acceleration.map(|a| a.rebuilds),
                    upload.map(|u| u.bvh_build_ms),
                    upload.map(|u| u.enqueue_ms)
                );
                eprintln!(
                    "{{\"kind\":\"native-frame-window\",\"resolution\":[{},{}],\"samples\":{},\"tick\":{},\"paused\":{},\"step_samples\":{},\"interval_ms\":[{interval_p50:.4},{interval_p95:.4},{:.4}],\"loop_ms\":[{frame_p50:.4},{frame_p95:.4},{:.4}],\"acquire_ms\":[{acquire_p50:.4},{acquire_p95:.4}],\"encode_submit_ms\":[{submit_p50:.4},{submit_p95:.4}],\"present_call_ms\":[{present_p50:.4},{present_p95:.4}],\"step_ms\":[{step_p50:.4},{step_p95:.4}],\"snapshot_ms\":[{snapshot_p50:.4},{snapshot_p95:.4}],\"upload_enqueue_ms\":[{upload_p50:.4},{upload_p95:.4}],\"bvh_build_ms\":[{build_p50:.4},{build_p95:.4}],\"gpu_compute_ms\":[{gpu_p50:.4},{gpu_p95:.4}],\"gpu_samples\":{},\"present_mode\":\"{:?}\",\"backend\":\"{:?}\",\"os\":\"{}\",\"profile\":\"{}\"}}",
                    size.width,
                    size.height,
                    self.frame_samples.len(),
                    self.time.ticks(),
                    self.paused,
                    self.step_samples.len(),
                    p99(&self.interval_samples),
                    p99(&self.frame_samples),
                    self.gpu_samples.len(),
                    graphics.config.present_mode,
                    graphics.adapter.get_info().backend,
                    std::env::consts::OS,
                    if cfg!(debug_assertions) {
                        "debug"
                    } else {
                        "release"
                    }
                );
                self.frame_samples.clear();
                self.interval_samples.clear();
                self.acquire_samples.clear();
                self.submit_samples.clear();
                self.present_samples.clear();
                self.upload_samples.clear();
                self.bvh_build_samples.clear();
                self.step_samples.clear();
                self.snapshot_samples.clear();
                self.gpu_samples.clear();
            }
            self.stats_since = now;
            self.frames = 0;
            self.cpu_total = Duration::ZERO;
        }
        if reconfigure_after {
            graphics.configure_surface(size.width, size.height, true);
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
                .with_title(if self.passage {
                    "The Passage — Genesis Experimental Demo"
                } else if self.authored.is_some() {
                    "Genesis Authoring Preview — Experimental"
                } else if self.life {
                    "First Life — Experimental / Research Stage"
                } else {
                    "First Light — Experimental / Research Stage"
                })
                .with_inner_size(match std::env::var("GENESIS_WINDOW_SIZE") {
                    Ok(value) => match requested_window_size(&value) {
                        Some(size) => size,
                        None => {
                            eprintln!("invalid GENESIS_WINDOW_SIZE; expected WIDTHxHEIGHT");
                            event_loop.exit();
                            return;
                        }
                    },
                    Err(std::env::VarError::NotPresent) => winit::dpi::PhysicalSize::new(1280, 720),
                    Err(error) => {
                        eprintln!("invalid GENESIS_WINDOW_SIZE: {error}");
                        event_loop.exit();
                        return;
                    }
                }),
        ) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("window creation failed: {error}");
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Graphics::open(window, self.lost.clone(), self.passage)) {
            Ok(graphics) => {
                self.graphics = Some(graphics);
                self.previous = Instant::now();
            }
            Err(error) => {
                eprintln!("GPU initialization failed: {error}");
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(graphics) = self.graphics.as_ref() else {
            return;
        };
        if graphics.window.id() != id {
            return;
        }
        if self.benchmark_scenario.is_some() {
            if let WindowEvent::KeyboardInput { event, .. } = &event {
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                {
                    event_loop.exit();
                }
                return;
            }
            if matches!(event, WindowEvent::MouseInput { .. }) {
                return;
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.last_camera = None;
                self.selected = None;
                if let Some(graphics) = self.graphics.as_mut() {
                    graphics.configure_surface(size.width, size.height, false);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::Focused(focused) => self.set_focus(focused),
            WindowEvent::CursorMoved { position, .. } => self.cursor = Some(position),
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if (self.life || self.authored.is_some()) && self.focused => self.pick_cursor(),
            WindowEvent::KeyboardInput { event, .. }
                if self.life
                    && self.focused
                    && (self.passage
                        || matches!(
                            event.physical_key,
                            PhysicalKey::Code(
                                KeyCode::KeyI | KeyCode::KeyK | KeyCode::KeyJ | KeyCode::KeyL
                            )
                        ))
                    && matches!(
                        event.physical_key,
                        PhysicalKey::Code(
                            KeyCode::KeyI
                                | KeyCode::KeyK
                                | KeyCode::KeyJ
                                | KeyCode::KeyL
                                | KeyCode::KeyW
                                | KeyCode::KeyS
                                | KeyCode::KeyA
                                | KeyCode::KeyD
                        )
                    ) =>
            {
                let index = match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyI | KeyCode::KeyW) => 0,
                    PhysicalKey::Code(KeyCode::KeyK | KeyCode::KeyS) => 1,
                    PhysicalKey::Code(KeyCode::KeyJ | KeyCode::KeyA) => 2,
                    PhysicalKey::Code(KeyCode::KeyL | KeyCode::KeyD) => 3,
                    _ => unreachable!(),
                };
                let pressed = event.state == ElementState::Pressed;
                self.set_body_key(index, pressed);
            }
            WindowEvent::KeyboardInput { event, .. }
                if self.focused && event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match key {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::Space if self.authored.is_none() => self.toggle_pause(),
                        KeyCode::KeyN => self.normals = !self.normals,
                        KeyCode::KeyM if self.life => {
                            self.pending_move = true;
                            self.status = "RESOURCE MOVE QUEUED".into();
                        }
                        KeyCode::KeyO if self.passage => {
                            self.pending_source_toggle = true;
                            self.status = "SOURCE SWITCH QUEUED".into();
                        }
                        KeyCode::KeyP if self.life => self.queue_prune(),
                        KeyCode::KeyR if self.passage => {
                            if let Err(error) = self.restart_passage() {
                                self.status = format!("RESTART FAILED: {error}");
                            }
                        }
                        KeyCode::KeyR if self.authored.is_some() => {
                            let preview = self.authored.as_mut().expect("checked above");
                            match preview.reload() {
                                Ok(true) => {
                                    preview.print_summary();
                                    self.last_camera = None;
                                    self.status =
                                        "AUTHORED REVISION CHANGED; SELECTION CLEARED".into();
                                    eprintln!(
                                        "authored definition reloaded; preview generation {}",
                                        preview.generation
                                    );
                                }
                                Ok(false) => {
                                    self.status = "AUTHORED SOURCE UNCHANGED".into();
                                    eprintln!("authored source unchanged; no rebuild");
                                }
                                Err(error) => {
                                    self.status = format!("RELOAD REJECTED: {error}");
                                    eprintln!("reload rejected; preview unchanged: {error}")
                                }
                            }
                        }
                        KeyCode::ArrowLeft | KeyCode::KeyA if !self.passage => {
                            self.yaw -= 0.15;
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowRight | KeyCode::KeyD if !self.passage => {
                            self.yaw += 0.15;
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowUp | KeyCode::KeyW if !self.passage => {
                            self.elevation = (self.elevation + 0.1).min(1.3);
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowDown | KeyCode::KeyS if !self.passage => {
                            self.elevation = (self.elevation - 0.1).max(-1.3);
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowLeft if self.passage => {
                            self.yaw -= 0.15;
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowRight if self.passage => {
                            self.yaw += 0.15;
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowUp if self.passage => {
                            self.elevation = (self.elevation + 0.1).min(1.3);
                            self.last_camera = None;
                            self.selected = None;
                        }
                        KeyCode::ArrowDown if self.passage => {
                            self.elevation = (self.elevation - 0.1).max(-1.3);
                            self.last_camera = None;
                            self.selected = None;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(graphics) = &self.graphics {
            graphics.window.request_redraw();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--passage-measure"] {
        return passage::measure_cpu();
    }
    let mut app = match args.as_slice() {
        [] => App::new(false, false)?,
        [flag] if flag == "--passage" => App::new(true, true)?,
        [flag] if flag == "--life" => App::new(true, false)?,
        [flag, path] if flag == "--authoring" => App::authoring(PathBuf::from(path))?,
        _ => return Err("usage: first-light [--life|--passage|--authoring FILE]".into()),
    };
    let (life, passage) = (app.life, app.passage);
    eprintln!(
        "{} controls: {} {}N normals; {}Esc exit",
        if passage {
            "The Passage"
        } else if life {
            "First Life"
        } else if app.authored.is_some() {
            "Genesis Authoring Preview"
        } else {
            "First Light"
        },
        if passage {
            "WASD move; arrows camera; R restart;"
        } else {
            "A/D or Left/Right orbit; W/S or Up/Down tilt;"
        },
        if app.authored.is_some() {
            ""
        } else {
            "Space pause; "
        },
        if app.authored.is_some() {
            "R reload definition; "
        } else if life && !passage {
            "M move resource source; Left click select; P prune selected branch; I/J/K/L move body; "
        } else if passage {
            "M move source; O switch source; Left click select; P prune branch; "
        } else {
            ""
        },
    );
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod app_tests {
    use super::*;
    use analytic_renderer::{GpuTraversal, Ray};
    use std::{path::Path, sync::mpsc};
    use world_simulation::advance_life;

    const BRANCH_A: &[u8] = include_bytes!("../../authoring/branch-a.json");
    const BRANCH_B: &[u8] = include_bytes!("../../authoring/branch-b.json");

    #[test]
    fn authored_preview_reload_is_atomic_and_matches_analytic_segments() {
        let mut preview = AuthoredPreview::from_structure(
            PathBuf::from("branch-a.json"),
            world_authoring::compile_json(BRANCH_A).unwrap(),
        )
        .unwrap();
        assert_eq!(preview.scene.primitives().len(), 26);
        for (primitive, segment) in preview
            .scene
            .primitives()
            .iter()
            .zip(preview.structure.segments())
        {
            assert_eq!(primitive.id, segment.id);
            assert_eq!(primitive.center(), segment.start);
            assert_eq!(primitive.dimensions(), segment.end);
            assert_eq!(primitive.capsule_radius(), segment.radius);
        }
        preview.select(1);
        let selected = preview.selection.unwrap();
        assert!(preview.resolve(selected));
        let before_nodes = analytic_renderer::Bvh::build(&preview.scene)
            .unwrap()
            .node_count();
        assert!(!preview.replace(BRANCH_A).unwrap());
        assert_eq!(preview.generation, 1);
        assert_eq!(preview.selection, Some(selected));
        assert!(preview.replace(b"{invalid").is_err());
        let mut over_budget: serde_json::Value = serde_json::from_slice(BRANCH_A).unwrap();
        over_budget["budgets"]["max_segments"] = serde_json::json!(1);
        assert!(
            preview
                .replace(&serde_json::to_vec(&over_budget).unwrap())
                .is_err()
        );
        assert_eq!(preview.structure.id(), "branch-a");
        assert_eq!(preview.scene.primitives().len(), 26);
        assert!(preview.resolve(selected));
        assert_eq!(
            analytic_renderer::Bvh::build(&preview.scene)
                .unwrap()
                .node_count(),
            before_nodes
        );
        assert!(preview.replace(BRANCH_B).unwrap());
        assert_eq!(preview.structure.id(), "branch-b");
        assert_eq!(preview.scene.primitives().len(), 15);
        assert_ne!(preview.structure.source(), BRANCH_A);
        assert_eq!(preview.generation, 2);
        assert!(preview.selection.is_none());
        assert!(!preview.resolve(selected));
        assert_ne!(
            analytic_renderer::Bvh::build(&preview.scene)
                .unwrap()
                .node_count(),
            before_nodes
        );
    }

    #[test]
    fn authored_file_reload_uses_exact_bytes_and_rolls_back() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/authoring-reload-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, BRANCH_A).unwrap();
        let mut preview = AuthoredPreview::load(path.clone()).unwrap();
        assert!(!preview.reload().unwrap());
        assert_eq!(preview.generation, 1);
        std::fs::write(&path, BRANCH_B).unwrap();
        assert!(preview.reload().unwrap());
        assert_eq!(preview.generation, 2);
        assert_eq!(preview.structure.source(), BRANCH_B);
        std::fs::write(&path, b"{invalid").unwrap();
        assert!(preview.reload().is_err());
        assert_eq!(preview.generation, 2);
        assert_eq!(preview.structure.source(), BRANCH_B);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "local release-profile unchanged/changed preview reload timing"]
    fn authored_reload_benchmark() {
        let mut preview =
            AuthoredPreview::from_structure(PathBuf::new(), compile_json(BRANCH_A).unwrap())
                .unwrap();
        let mut unchanged = Vec::new();
        let mut changed = Vec::new();
        for _ in 0..30 {
            let unchanged_source = if preview.structure.id() == "branch-a" {
                BRANCH_A
            } else {
                BRANCH_B
            };
            let start = Instant::now();
            assert!(!preview.replace(unchanged_source).unwrap());
            unchanged.push(start.elapsed().as_secs_f64() * 1000.0);
            let source = if preview.structure.id() == "branch-a" {
                BRANCH_B
            } else {
                BRANCH_A
            };
            let start = Instant::now();
            assert!(preview.replace(source).unwrap());
            changed.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let generation_after_reload = preview.generation;
        let mut swap = Vec::new();
        for _ in 0..30 {
            let source = if preview.structure.id() == "branch-a" {
                BRANCH_B
            } else {
                BRANCH_A
            };
            let next =
                AuthoredPreview::from_structure(PathBuf::new(), compile_json(source).unwrap())
                    .unwrap();
            analytic_renderer::Bvh::build(&next.scene).unwrap();
            let start = Instant::now();
            let old = std::mem::replace(&mut preview, next);
            drop(old);
            swap.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "{}",
            serde_json::json!({
                "kind": "authored-reload",
                "samples": 30,
                "unchanged_ms": median_p95(&unchanged),
                "changed_compile_scene_bvh_swap_ms": median_p95(&changed),
                "prepared_swap_and_drop_ms": median_p95(&swap),
                "generation_after_reload": generation_after_reload,
            })
        );
    }

    #[test]
    #[ignore = "local release-profile CPU authoring, snapshot, and BVH timing"]
    fn authored_scaling_benchmark() {
        for iterations in 1..=6 {
            let mut value: serde_json::Value = serde_json::from_slice(BRANCH_A).unwrap();
            value["iterations"] = serde_json::json!(iterations);
            value["budgets"]["max_symbols"] = serde_json::json!(65536);
            value["budgets"]["max_segments"] = serde_json::json!(1024);
            value["budgets"]["max_stack_depth"] = serde_json::json!(16);
            value["budgets"]["max_work"] = serde_json::json!(1000000);
            let source = serde_json::to_vec(&value).unwrap();
            let mut compile = Vec::new();
            let mut snapshot = Vec::new();
            let mut bvh_build = Vec::new();
            let mut result = None;
            for _ in 0..30 {
                let started = Instant::now();
                let structure = world_authoring::compile_json(&source).unwrap();
                compile.push(started.elapsed().as_secs_f64() * 1000.0);
                let symbols = structure.expanded_symbols();
                let started = Instant::now();
                let preview = AuthoredPreview::from_structure(PathBuf::new(), structure).unwrap();
                snapshot.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                let bvh = analytic_renderer::Bvh::build(&preview.scene).unwrap();
                bvh_build.push(started.elapsed().as_secs_f64() * 1000.0);
                result = Some((
                    symbols,
                    preview.structure.segments().len(),
                    bvh.node_count(),
                    bvh.depth(),
                ));
            }
            let (symbols, segments, nodes, depth) = result.unwrap();
            println!(
                "{}",
                serde_json::json!({
                    "kind": "authored-cpu-scale",
                    "iterations": iterations,
                    "input_bytes": source.len(),
                    "expanded_symbols": symbols,
                    "segments": segments,
                    "bvh_nodes": nodes,
                    "bvh_depth": depth,
                    "minimum_source_and_segment_bytes": source.len() + segments * std::mem::size_of::<world_authoring::Segment>(),
                    "samples": 30,
                    "compile_ms": median_p95(&compile),
                    "snapshot_ms": median_p95(&snapshot),
                    "bvh_build_ms": median_p95(&bvh_build)
                })
            );
        }
    }

    #[test]
    fn surface_fault_policy_and_drawable_limits() {
        for outcome in [
            wgpu::CurrentSurfaceTexture::Lost,
            wgpu::CurrentSurfaceTexture::Outdated,
        ] {
            assert!(matches!(
                classify_surface(outcome),
                SurfaceFrame::Reconfigure
            ));
        }
        for outcome in [
            wgpu::CurrentSurfaceTexture::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded,
        ] {
            assert!(matches!(classify_surface(outcome), SurfaceFrame::Skip));
        }
        assert!(matches!(
            classify_surface(wgpu::CurrentSurfaceTexture::Validation),
            SurfaceFrame::Fatal
        ));
        assert_eq!(next_surface_retry(0), Some(1));
        assert_eq!(next_surface_retry(2), Some(3));
        assert_eq!(next_surface_retry(3), None);
        for size in [(1280, 720), (1920, 1080), (2560, 1440), (1, 8192)] {
            assert!(drawable_size(size.0, size.1, 8192));
        }
        for size in [(0, 720), (720, 0), (8193, 1), (1, 8193)] {
            assert!(!drawable_size(size.0, size.1, 8192));
        }
        assert_eq!(
            requested_window_size("1920x1080"),
            Some(winit::dpi::PhysicalSize::new(1920, 1080))
        );
        for malformed in [
            "0x720",
            "2560x0",
            "8192x16385",
            "-1x720",
            "720X1280",
            "1x2x3",
        ] {
            assert_eq!(requested_window_size(malformed), None);
        }
    }

    #[test]
    fn benchmark_interaction_runs_and_restarts() {
        let mut app = App::new(true, true).unwrap();
        app.benchmark_scenario = Some(BenchmarkScenario::Interaction);
        for index in 0..=180 {
            app.simulation_tick().unwrap();
            match index {
                20 => assert!(!app.world.sources()[0].active()),
                40 => assert!(app.world.sources()[0].active()),
                60 => assert!(app.world.organisms()[0].node(1).is_none()),
                80 => assert!(app.body_keys[0]),
                120 => assert!(!app.body_keys[0]),
                180 => assert_eq!(app.time.ticks(), 45),
                _ => {}
            }
        }
        app.simulation_tick().unwrap();
        assert_eq!(app.time.ticks(), 46);
    }

    fn selected_branch(app: &App, organism_index: usize) -> SemanticTarget {
        let organism = &app.world.organisms()[organism_index];
        let node = &organism.nodes()[1];
        let position = node.position();
        let scene = app.scene().unwrap();
        let ray = Ray::new(
            v(position.x(), position.y(), position.z() - 0.6),
            v(0.0, 0.0, 1.0),
            0.0,
            2.0,
        )
        .unwrap();
        match scene.pick_ray(ray).unwrap() {
            PickOutcome::Hit(hit) => {
                assert_eq!(
                    passage::branch(Some(hit.target), &app.world),
                    Some((organism.id(), node.id()))
                );
                hit.target
            }
            other => panic!("branch was not pickable: {other:?}"),
        }
    }

    #[test]
    fn passage_input_pause_focus_and_restart() {
        let mut app = App::new(true, true).unwrap();
        let initial = app.world.clone();
        let initial_tick = app.time.ticks();
        app.set_body_key(0, true);
        app.simulation_tick().unwrap();
        assert!(app.world.body().unwrap().position().z() > initial.body().unwrap().position().z());
        app.toggle_pause();
        let paused = (app.time, app.world.clone());
        app.simulation_tick().unwrap();
        assert_eq!((app.time, app.world.clone()), paused);
        app.toggle_pause();
        app.set_focus(false);
        app.simulation_tick().unwrap();
        assert_eq!(
            app.world.body().unwrap().position(),
            paused.1.body().unwrap().position()
        );
        assert!(!app.body_keys.iter().any(|pressed| *pressed));
        app.restart_passage().unwrap();
        assert_eq!(app.world, initial);
        assert_eq!(app.time.ticks(), initial_tick);
        assert!(!app.paused && !app.won && app.selected.is_none());
        app.won = true;
        app.paused = true;
        app.toggle_pause();
        assert!(app.paused);
        app.restart_passage().unwrap();
        assert!(!app.paused && !app.won);
        println!("{{\"scenario\":\"input-pause-focus-restart\",\"result\":\"success\"}}");
    }

    #[test]
    fn passage_picking_actions_and_highlight() {
        let mut app = App::new(true, true).unwrap();
        let goal_ray = Ray::new(v(0.0, 0.55, 3.0), v(0.0, 0.0, -1.0), 0.001, 100.0).unwrap();
        assert!(matches!(
            app.scene().unwrap().pick_ray(goal_ray).unwrap(),
            PickOutcome::Indeterminate
        ));
        let wrong = selected_branch(&app, 1);
        app.selected = Some(wrong);
        app.queue_prune();
        app.simulation_tick().unwrap();
        assert!(app.world.organisms()[0].node(1).is_some());
        assert!(app.selected.is_none());
        let correct = selected_branch(&app, 0);
        let plain = app.scene().unwrap();
        app.selected = Some(correct);
        let highlighted = app.scene().unwrap();
        let changed: Vec<_> = plain
            .primitives()
            .iter()
            .zip(highlighted.primitives())
            .filter(|(before, after)| before.color() != after.color())
            .collect();
        assert!(!changed.is_empty());
        let (organism, child) = passage::branch(Some(correct), &app.world).unwrap();
        let mut invalid = plain.clone();
        assert!(!invalid.highlight_branch(organism, child, [f32::NAN, 0.0, 0.0]));
        assert!(
            invalid
                .primitives()
                .iter()
                .zip(plain.primitives())
                .all(|(a, b)| a.color() == b.color())
        );
        assert!(changed.iter().all(|(_, after)| {
            matches!(after.target(), Some(SemanticTarget::GrowthNode { organism: id, node }) if id == organism && node == child)
                || matches!(after.target(), Some(SemanticTarget::Connection { organism: id, child: node }) if id == organism && node == child)
        }));
        app.pending_move = true;
        app.queue_prune();
        app.set_body_key(0, true);
        app.simulation_tick().unwrap();
        assert!(app.world.sources()[0].position().x() > 2.0);
        assert!(app.world.organisms()[0].node(1).is_none());
        assert!(app.selected.is_none());
        app.selected = Some(correct);
        app.queue_prune();
        assert!(app.selected.is_none());
        assert!(app.pending_prune.is_none());
        app.pending_move = true;
        app.simulation_tick().unwrap();
        assert!(app.world.sources()[0].position().x() < 2.0);
        println!("{{\"scenario\":\"picking-prune-source-order\",\"result\":\"success\"}}");
    }

    #[test]
    fn passage_source_switch_is_tick_ordered_and_replayable() {
        let mut app = App::new(true, true).unwrap();
        let source = app.world.sources()[0].id();
        let mut replay_world = app.world.clone();
        let mut replay_time = app.time;
        let event_tick = replay_time.ticks() + 1;
        app.pending_source_toggle = true;
        app.simulation_tick().unwrap();
        advance_life(
            &mut replay_world,
            &mut replay_time,
            app.step,
            &[EnvironmentEvent {
                tick: event_tick,
                order: 0,
                kind: EnvironmentEventKind::SetSourceActive {
                    id: source,
                    active: false,
                },
            }],
        )
        .unwrap();
        assert_eq!(app.world, replay_world);
        assert_eq!(app.time, replay_time);
        assert!(!app.world.sources()[0].active());
        assert!(!app.pending_source_toggle);
        let before = app.world.organisms()[1].nodes().len();
        for _ in 0..60 {
            app.simulation_tick().unwrap();
        }
        assert_eq!(app.world.organisms()[1].nodes().len(), before);
        app.pending_source_toggle = true;
        app.simulation_tick().unwrap();
        assert!(app.world.sources()[0].active());
        for _ in 0..120 {
            app.simulation_tick().unwrap();
        }
        assert!(app.world.organisms()[1].nodes().len() > before);
        app.restart_passage().unwrap();
        assert!(app.world.sources()[0].active());
        assert!(!app.pending_source_toggle);
    }

    #[test]
    fn passage_mixed_runtime_stress_replays_and_restores() {
        let mut first = App::new(true, true).unwrap();
        let mut replay = App::new(true, true).unwrap();
        let initial = first.world.clone();
        let stable_ids: Vec<_> = first
            .world
            .organisms()
            .iter()
            .map(|tree| tree.id())
            .collect();
        let mut paused_time = None;
        for index in 0..300 {
            for app in [&mut first, &mut replay] {
                match index {
                    0 => {
                        app.pending_move = true;
                        app.set_body_key(0, true);
                    }
                    50 | 80 => app.pending_source_toggle = true,
                    100 => app.set_body_key(0, false),
                    120 => {
                        app.selected = Some(SemanticTarget::GrowthNode {
                            organism: app.world.organisms()[0].id(),
                            node: 1,
                        });
                        app.queue_prune();
                    }
                    150 | 154 => app.toggle_pause(),
                    220 => app.restart_passage().unwrap(),
                    _ => {}
                }
                app.simulation_tick().unwrap();
            }
            assert_eq!(
                first.world, replay.world,
                "world divergence at action {index}"
            );
            assert_eq!(first.time, replay.time, "time divergence at action {index}");
            if index == 150 {
                paused_time = Some(first.time);
            }
            if (151..154).contains(&index) {
                assert_eq!(Some(first.time), paused_time);
            }
            if index < 220 {
                assert_eq!(
                    first
                        .world
                        .organisms()
                        .iter()
                        .map(|tree| tree.id())
                        .collect::<Vec<_>>(),
                    stable_ids
                );
            }
            if index % 20 == 0 {
                first.world.validate().unwrap();
                let left = first.scene().unwrap();
                let right = replay.scene().unwrap();
                assert_eq!(left.primitives().len(), right.primitives().len());
                assert!(
                    left.primitives()
                        .iter()
                        .zip(right.primitives())
                        .all(|(a, b)| a.id == b.id && a.target() == b.target())
                );
                let saved = world_runtime::Runtime::new(first.world.clone(), first.step)
                    .save_bytes()
                    .unwrap();
                assert_eq!(
                    world_runtime::Runtime::load_bytes(&saved).unwrap().world(),
                    &first.world
                );
            }
        }
        assert_ne!(first.world, initial);
        assert_eq!(first.world.organisms().len(), 2);
    }

    #[test]
    fn growing_topology_keeps_only_live_semantic_selection() {
        let mut app = App::new(true, true).unwrap();
        let selected = selected_branch(&app, 1);
        let before = app.world.organisms()[1].nodes().len();
        app.selected = Some(selected);
        app.pending_move = true;
        for _ in 0..120 {
            app.simulation_tick().unwrap();
        }
        assert!(app.world.organisms()[1].nodes().len() > before);
        assert!(
            passage::branch(app.selected, &app.world).is_some(),
            "stable node identity should survive growth"
        );
        app.queue_prune();
        app.simulation_tick().unwrap();
        app.selected = Some(selected);
        assert!(
            passage::branch(app.selected, &app.world).is_none(),
            "removed branch must not remain actionable"
        );
        assert!(
            !app.scene()
                .unwrap()
                .primitives()
                .iter()
                .any(|p| p.color() == [0.98, 0.96, 0.18])
        );
        app.queue_prune();
        assert!(app.selected.is_none() && app.pending_prune.is_none());
        println!("{{\"scenario\":\"growth-and-stale-selection\",\"result\":\"success\"}}");
    }

    fn read_rgb(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &wgpu::Texture,
        size: [u32; 2],
    ) -> Vec<u8> {
        let [width, height] = size;
        let row_bytes = width * 4;
        let padded = row_bytes.div_ceil(256) * 256;
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("passage capture"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("passage copy"),
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
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = slice.get_mapped_range().unwrap();
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for row in mapped.chunks_exact(padded as usize) {
            for pixel in row[..row_bytes as usize].chunks_exact(4) {
                rgb.extend_from_slice(&pixel[..3]);
            }
        }
        rgb
    }

    fn write_bmp(path: &Path, rgb: &[u8], size: [u32; 2]) {
        let [width, height] = size;
        let stride = (width * 3).div_ceil(4) * 4;
        let data_size = stride * height;
        let mut bmp = Vec::with_capacity((54 + data_size) as usize);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&(54 + data_size).to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&54_u32.to_le_bytes());
        bmp.extend_from_slice(&40_u32.to_le_bytes());
        bmp.extend_from_slice(&width.to_le_bytes());
        bmp.extend_from_slice(&height.to_le_bytes());
        bmp.extend_from_slice(&1_u16.to_le_bytes());
        bmp.extend_from_slice(&24_u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        for y in (0..height).rev() {
            for pixel in
                rgb[(y * width * 3) as usize..((y + 1) * width * 3) as usize].chunks_exact(3)
            {
                bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
            bmp.resize(bmp.len() + (stride - width * 3) as usize, 0);
        }
        std::fs::write(path, bmp).unwrap();
    }

    #[test]
    #[ignore = "requires a native GPU; set GENESIS_AUTHORING_CAPTURE_DIR for real BMP captures"]
    fn authored_gpu_images_match_direct_and_bvh() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("native GPU required");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let renderer =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let size = [640, 480];
        let mut captures = Vec::new();
        let mut rebuilds = Vec::new();
        for (name, source) in [("branch-a", BRANCH_A), ("branch-b", BRANCH_B)] {
            let preview = AuthoredPreview::from_structure(
                PathBuf::from(name),
                world_authoring::compile_json(source).unwrap(),
            )
            .unwrap();
            let camera = Camera::look_at(
                v(
                    preview.target.x() + preview.distance * 0.35,
                    preview.target.y() + preview.distance * 0.25,
                    preview.target.z() - preview.distance * 0.9,
                ),
                preview.target,
                v(0.0, 1.0, 0.0),
                0.95,
            )
            .unwrap();
            let draw = |traversal| {
                let image = renderer
                    .draw_with_traversal(
                        &device,
                        &queue,
                        &preview.scene,
                        camera,
                        traversal,
                        DrawOptions {
                            size,
                            normal_debug: false,
                            surface: None,
                            timer: None,
                        },
                    )
                    .unwrap();
                read_rgb(&device, &queue, &image, size)
            };
            let direct = draw(GpuTraversal::Direct);
            let bvh = draw(GpuTraversal::Bvh);
            assert_eq!(direct, bvh, "{name} direct/BVH mismatch");
            let built = renderer.acceleration_stats().unwrap().rebuilds;
            assert_eq!(draw(GpuTraversal::Bvh), bvh);
            assert_eq!(renderer.acceleration_stats().unwrap().rebuilds, built);
            rebuilds.push(built);
            assert!(direct.chunks_exact(3).any(|pixel| pixel != &direct[..3]));
            if let Some(path) = std::env::var_os("GENESIS_AUTHORING_CAPTURE_DIR") {
                let path = PathBuf::from(path);
                std::fs::create_dir_all(&path).unwrap();
                write_bmp(&path.join(format!("{name}.bmp")), &direct, size);
            }
            captures.push(direct);
        }
        assert_ne!(
            captures[0], captures[1],
            "different rules rendered identically"
        );
        assert_eq!(rebuilds[1], rebuilds[0] + 1);
    }

    #[test]
    #[ignore = "requires a native GPU; run explicitly with GENESIS_PASSAGE_CAPTURE_DIR for BMP captures"]
    fn passage_gpu_state_images_match_direct_and_bvh() {
        let mut app = App::new(true, true).unwrap();
        let mut scenes = vec![("initial", app.scene().unwrap())];
        app.selected = Some(selected_branch(&app, 0));
        scenes.push(("selected", app.scene().unwrap()));
        app.pending_source_toggle = true;
        app.simulation_tick().unwrap();
        assert!(!app.world.sources()[0].active());
        scenes.push(("source-off", app.scene().unwrap()));
        app.pending_source_toggle = true;
        app.simulation_tick().unwrap();
        assert!(app.world.sources()[0].active());
        scenes.push(("source-on", app.scene().unwrap()));
        app.toggle_pause();
        scenes.push(("paused", app.scene().unwrap()));
        app.toggle_pause();
        app.pending_move = true;
        app.simulation_tick().unwrap();
        scenes.push(("source-moved", app.scene().unwrap()));
        app.queue_prune();
        app.simulation_tick().unwrap();
        scenes.push(("pruned", app.scene().unwrap()));
        app.set_body_key(0, true);
        for _ in 0..150 {
            if app.won {
                break;
            }
            app.simulation_tick().unwrap();
        }
        assert!(app.won);
        scenes.push(("victory", app.scene().unwrap()));
        app.restart_passage().unwrap();
        scenes.push(("restart", app.scene().unwrap()));
        let mut dense = WorldState::new(DeterministicSeed(17));
        for index in 0..16 {
            dense
                .spawn_sphere(
                    Sphere::new(0.4).unwrap(),
                    Transform::new(v(index as f64 * 0.02, 0.0, 0.0), 1.0).unwrap(),
                    0.0,
                )
                .unwrap();
        }
        scenes.push(("dense-overlap", Scene::from_world(&dense).unwrap()));
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("native GPU required");
        let info = adapter.get_info();
        eprintln!(
            "passage_gpu adapter={} backend={:?} driver={}",
            info.name, info.backend, info.driver
        );
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let renderer =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let camera = Camera::look_at(
            v(0.0, 0.55 + 0.25_f64.sin() * 5.3, -0.25_f64.cos() * 5.3),
            v(0.0, 0.55, 0.0),
            v(0.0, 1.0, 0.0),
            0.95,
        )
        .unwrap();
        let size = [640, 360];
        let capture_dir = std::env::var_os("GENESIS_PASSAGE_CAPTURE_DIR");
        if let Some(dir) = &capture_dir {
            std::fs::create_dir_all(dir).unwrap();
        }
        let mut frames = Vec::new();
        for (name, scene) in &scenes {
            let draw = |mode| {
                let image = renderer
                    .draw_with_traversal(
                        &device,
                        &queue,
                        scene,
                        camera,
                        mode,
                        DrawOptions {
                            size,
                            normal_debug: false,
                            surface: None,
                            timer: None,
                        },
                    )
                    .unwrap();
                read_rgb(&device, &queue, &image, size)
            };
            let direct = draw(GpuTraversal::Direct);
            let bvh = draw(GpuTraversal::Bvh);
            assert_eq!(direct, bvh, "{name}: direct/BVH image mismatch");
            let rays: Vec<_> = [120, 240, 320, 400, 520]
                .into_iter()
                .flat_map(|x| {
                    [90, 180, 270]
                        .into_iter()
                        .map(move |y| camera.ray(x, y, size[0], size[1]).unwrap())
                })
                .collect();
            let direct_hits = renderer.query(&device, &queue, scene, &rays).unwrap();
            let bvh_hits = renderer
                .query_with_traversal(&device, &queue, scene, &rays, GpuTraversal::Bvh)
                .unwrap();
            for (direct, bvh) in direct_hits.iter().zip(bvh_hits.iter()) {
                assert_eq!(
                    direct.map(|hit| hit.id),
                    bvh.map(|hit| hit.id),
                    "{name}: identity mismatch"
                );
                if let (Some(direct), Some(bvh)) = (direct, bvh) {
                    assert!(
                        (direct.distance - bvh.distance).abs() <= 1e-4,
                        "{name}: depth mismatch"
                    );
                }
            }
            if let Some(dir) = &capture_dir {
                write_bmp(
                    &Path::new(dir).join(format!("passage-{name}.bmp")),
                    &direct,
                    size,
                );
            }
            eprintln!(
                "passage_gpu state={name} size={}x{} mismatches=0",
                size[0], size[1]
            );
            frames.push(direct);
        }
        for (a, b) in [(0, 1), (0, 2), (0, 5), (0, 6), (0, 7)] {
            assert_ne!(frames[a], frames[b]);
        }
        assert_ne!(frames[2], frames[3], "source activity did not change image");
        assert_eq!(frames[3], frames[4], "pause changed render snapshot");
        assert_eq!(frames[0], frames[8], "restart differs from initial state");
        for size in [
            [1280, 720],
            [1920, 1080],
            [2560, 1440],
            [720, 1280],
            [1280, 720],
        ] {
            let draw = |traversal| {
                let image = renderer
                    .draw_with_traversal(
                        &device,
                        &queue,
                        &scenes[0].1,
                        camera,
                        traversal,
                        DrawOptions {
                            size,
                            normal_debug: false,
                            surface: None,
                            timer: None,
                        },
                    )
                    .unwrap();
                assert_eq!(image.width(), size[0]);
                assert_eq!(image.height(), size[1]);
                read_rgb(&device, &queue, &image, size)
            };
            let direct = draw(GpuTraversal::Direct);
            let bvh = draw(GpuTraversal::Bvh);
            assert_eq!(direct, bvh, "resized direct/BVH image mismatch: {size:?}");
            assert_eq!(direct.len(), (size[0] * size[1] * 3) as usize);
            assert!(direct.chunks_exact(3).any(|pixel| pixel != &direct[..3]));
            if let Some(dir) = &capture_dir
                && size == [720, 1280]
            {
                write_bmp(&Path::new(dir).join("passage-portrait.bmp"), &direct, size);
            }
        }
        let wide = camera.ray(480, 540, 1920, 1080).unwrap().direction();
        let tall = camera.ray(270, 960, 1080, 1920).unwrap().direction();
        assert!(wide.x().abs() / wide.z().abs() > 2.0 * tall.x().abs() / tall.z().abs());
    }

    #[test]
    #[ignore = "release-profile native GPU offscreen measurement; no window presentation"]
    fn passage_gpu_resolution_measurement() {
        let mut app = App::new(true, true).unwrap();
        let static_scene = app.scene().unwrap();
        app.pending_move = true;
        for _ in 0..120 {
            app.simulation_tick().unwrap();
        }
        let growing_scene = app.scene().unwrap();
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("native GPU required");
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: features,
            ..Default::default()
        }))
        .unwrap();
        let renderer =
            pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
        let timer = GpuTimer::new(&device);
        let camera = Camera::look_at(
            v(0.0, 0.55 + 0.25_f64.sin() * 5.3, -0.25_f64.cos() * 5.3),
            v(0.0, 0.55, 0.0),
            v(0.0, 1.0, 0.0),
            0.95,
        )
        .unwrap();
        eprintln!(
            "passage_offscreen adapter={:?} timestamp={} profile={}",
            adapter.get_info(),
            timer.is_some(),
            if cfg!(debug_assertions) {
                "dev"
            } else {
                "release"
            }
        );
        for (name, scene) in [("initial", &static_scene), ("growing", &growing_scene)] {
            for size in [[1280, 720], [1920, 1080], [2560, 1440]] {
                let mut total = Vec::new();
                let mut gpu = Vec::new();
                for i in 0..12 {
                    let start = Instant::now();
                    renderer
                        .draw_with_traversal(
                            &device,
                            &queue,
                            scene,
                            camera,
                            GpuTraversal::Bvh,
                            DrawOptions {
                                size,
                                normal_debug: false,
                                surface: None,
                                timer: timer.as_ref(),
                            },
                        )
                        .unwrap();
                    let gpu_ms = if let Some(timer) = &timer {
                        Some(timer.read_ms(&device, &queue).unwrap())
                    } else {
                        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                        None
                    };
                    if i >= 2 {
                        total.push(start.elapsed().as_secs_f64() * 1000.0);
                        if let Some(ms) = gpu_ms {
                            gpu.push(ms);
                        }
                    }
                }
                let (total_p50, total_p95) = median_p95(&total);
                let (gpu_p50, gpu_p95) = median_p95(&gpu);
                eprintln!(
                    "passage_offscreen scene={name} primitives={} size={}x{} total_ms_p50={total_p50:.3} total_ms_p95={total_p95:.3} gpu_ms_p50={gpu_p50:.3} gpu_ms_p95={gpu_p95:.3} samples={} gpu_samples={}",
                    scene.primitives().len(),
                    size[0],
                    size[1],
                    total.len(),
                    gpu.len()
                );
            }
        }
    }
}
