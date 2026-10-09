//! Native First Light viewport. World simulation advances in fixed steps; rendering samples it.

mod hud;
mod passage;

use analytic_field::{AxisAlignedBox, Sphere};
use analytic_renderer::{
    Camera, DrawOptions, GpuRenderer, GpuTimer, PickOutcome, Primitive, Scene, SemanticTarget,
};
use spatial_math::{Transform, Vec3};
use std::{
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

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).expect("finite scene coordinate")
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
            .await?;
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
            .await?;
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
        })
    }
    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if width > self.device.limits().max_texture_dimension_2d
            || height > self.device.limits().max_texture_dimension_2d
        {
            eprintln!("resize skipped: {width}x{height} exceeds GPU texture limit");
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}

struct App {
    graphics: Option<Graphics>,
    lost: Arc<AtomicBool>,
    world: WorldState,
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
    frame_samples: Vec<f64>,
    step_samples: Vec<f64>,
    snapshot_samples: Vec<f64>,
    gpu_samples: Vec<f64>,
    source_id: Option<EntityId>,
    body_id: Option<EntityId>,
    body_keys: [bool; 4],
    body_input_dirty: bool,
    pending_move: bool,
    pending_prune: Option<(EntityId, u32)>,
    selected: Option<SemanticTarget>,
    cursor: Option<winit::dpi::PhysicalPosition<f64>>,
    last_camera: Option<Camera>,
}
impl App {
    fn new(life: bool, passage: bool) -> Result<Self, Box<dyn std::error::Error>> {
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
            time,
            step: SimulationStep::new(STEP.as_secs_f64())?,
            contact_cache: None,
            paused: false,
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
                "M MOVE SOURCE; P CUT STEM; REACH GOAL.".into()
            } else {
                String::new()
            },
            measure: std::env::var_os("GENESIS_MEASURE").is_some(),
            frame_samples: Vec::new(),
            step_samples: Vec::new(),
            snapshot_samples: Vec::new(),
            gpu_samples: Vec::new(),
            source_id,
            body_id,
            body_keys: [false; 4],
            body_input_dirty: false,
            pending_move: false,
            pending_prune: None,
            selected: None,
            cursor: None,
            last_camera: None,
        })
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
            self.status = "CLICK INSIDE THE WINDOW".into();
            eprintln!("selection: click outside viewport");
            return;
        }
        let scene = match Scene::from_world(&self.world) {
            Ok(scene) => scene,
            Err(error) => {
                eprintln!("selection snapshot failed: {error}");
                return;
            }
        };
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
                self.status = if passage::branch(self.selected, &self.world).is_some() {
                    "BRANCH SELECTED. PRESS P TO CUT."
                } else {
                    "TARGET SELECTED; ONLY BRANCHES CAN BE CUT."
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
                self.status = "NO TARGET UNDER CURSOR".into();
                eprintln!("selection: miss");
            }
            Ok(PickOutcome::Ambiguous { first, second }) => {
                self.selected = None;
                self.status = "OVERLAPPING TARGETS; TRY ANOTHER ANGLE".into();
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
    fn scene(&self) -> Result<Scene, Box<dyn std::error::Error>> {
        let mut scene = Scene::from_world(&self.world)?;
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
        if !self.paused {
            self.accumulator += elapsed.min(Duration::from_millis(250));
            let mut steps = 0;
            while self.accumulator >= STEP && steps < 4 {
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
                if let Err(error) = result {
                    eprintln!("simulation failed: {error}");
                    event_loop.exit();
                    return;
                }
                if self.measure {
                    self.step_samples
                        .push(step_started.elapsed().as_secs_f64() * 1000.0);
                }
                self.pending_move = false;
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
                        self.status = "BLOCKED. MOVE SOURCE AND CUT LOWER STEM.".into();
                    }
                    eprintln!(
                        "contact tick={} {:?}",
                        self.time.ticks(),
                        self.world.body().and_then(|body| body.contact())
                    );
                }
                if let Some((organism, child)) = self.pending_prune.take() {
                    self.selected = None;
                    self.status = if self.passage && self.world.organisms()[0].node(1).is_some() {
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
            let counts: Vec<_> = self
                .world
                .organisms()
                .iter()
                .map(|tree| tree.nodes().len())
                .collect();
            Some([
                "THE PASSAGE - REACH THE MAGENTA GOAL".into(),
                "WASD MOVE | ARROWS CAMERA | CLICK SELECT".into(),
                "P CUT | M RESOURCE | SPACE PAUSE | R RESTART".into(),
                format!(
                    "SEL: {selected} | NODES {}/{} | TICK {}",
                    counts[0],
                    counts[1],
                    self.time.ticks()
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
        let target = if self.passage {
            v(0.0, 0.55, 0.0)
        } else {
            v(0.0, height / 2.0, 0.0)
        };
        let distance = if self.passage {
            5.3
        } else {
            (height * 1.1).max(7.0)
        };
        let origin = v(
            self.yaw.sin() * distance,
            target.y() + self.elevation.sin() * distance,
            -self.yaw.cos() * self.elevation.cos() * distance,
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
        let frame = match graphics.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                graphics.resize(size.width, size.height);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("surface lost or invalid; exiting");
                event_loop.exit();
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let started = Instant::now();
        let report = now.duration_since(self.stats_since) >= Duration::from_secs(2);
        let sampled_timer = if report || self.measure {
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
        if let (Some(hud), Some(lines)) = (&graphics.hud, &hud_lines) {
            hud.draw(&graphics.device, &graphics.queue, &view, lines);
        }
        graphics.queue.present(frame);
        if self.measure {
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
            let growth = if self.life {
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
                self.frame_samples.clear();
                self.step_samples.clear();
                self.snapshot_samples.clear();
                self.gpu_samples.clear();
            }
            self.stats_since = now;
            self.frames = 0;
            self.cpu_total = Duration::ZERO;
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
                } else if self.life {
                    "First Life — Experimental / Research Stage"
                } else {
                    "First Light — Experimental / Research Stage"
                })
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0)),
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.last_camera = None;
                self.selected = None;
                if let Some(graphics) = self.graphics.as_mut() {
                    graphics.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::Focused(false) if self.life => {
                self.focused = false;
                self.body_keys = [false; 4];
                self.body_input_dirty = true;
            }
            WindowEvent::Focused(true) => self.focused = true,
            WindowEvent::CursorMoved { position, .. } => self.cursor = Some(position),
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if self.life && self.focused => self.pick_cursor(),
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
                if self.body_keys[index] != pressed {
                    self.body_keys[index] = pressed;
                    self.body_input_dirty = true;
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if self.focused && event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match key {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::Space => {
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
                        KeyCode::KeyN => self.normals = !self.normals,
                        KeyCode::KeyM if self.life => {
                            self.pending_move = true;
                            self.status = "RESOURCE MOVE QUEUED".into();
                        }
                        KeyCode::KeyP if self.life => self.queue_prune(),
                        KeyCode::KeyR if self.passage => match passage::initial() {
                            Ok(initial) => {
                                self.world = initial.world().clone();
                                self.time = initial.time();
                                self.contact_cache = None;
                                self.selected = None;
                                self.last_camera = None;
                                self.pending_move = false;
                                self.pending_prune = None;
                                self.body_keys = [false; 4];
                                self.body_input_dirty = true;
                                self.paused = false;
                                self.won = false;
                                self.accumulator = Duration::ZERO;
                                self.previous = Instant::now();
                                self.status = "RESTARTED. REACH THE MAGENTA GOAL.".into();
                            }
                            Err(error) => self.status = format!("RESTART FAILED: {error}"),
                        },
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
    if std::env::args().nth(1).as_deref() == Some("--passage-measure") {
        return passage::measure_cpu();
    }
    let (life, passage) = match std::env::args().nth(1).as_deref() {
        Some("--passage") => (true, true),
        Some("--life") => (true, false),
        None => (false, false),
        Some(_) => return Err("usage: first-light [--life|--passage]".into()),
    };
    eprintln!(
        "{} controls: {} Space pause; N normals; {}Esc exit",
        if passage {
            "The Passage"
        } else if life {
            "First Life"
        } else {
            "First Light"
        },
        if passage {
            "WASD move; arrows camera; R restart;"
        } else {
            "A/D or Left/Right orbit; W/S or Up/Down tilt;"
        },
        if life && !passage {
            "M move resource source; Left click select; P prune selected branch; I/J/K/L move body; "
        } else if passage {
            "M move source; Left click select; P prune branch; "
        } else {
            ""
        },
    );
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::new(life, passage)?)?;
    Ok(())
}
