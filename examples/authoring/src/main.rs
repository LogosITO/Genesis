//! CPU-only inspection of an external, bounded mathematical structure.

use std::{path::Path, time::Instant};
use world_authoring::compile_file;

fn v(x: f64, y: f64, z: f64) -> spatial_math::Vec3 {
    spatial_math::Vec3::new(x, y, z).expect("finite fixture coordinate")
}

fn world_scenario(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use analytic_renderer::{PickOutcome, Ray, Scene, SemanticTarget};
    use spatial_math::Transform;
    use world_runtime::Runtime;
    use world_simulation::{
        EnvironmentEventKind, SimulationStep,
        contact::{ContactScene, SweepOutcome},
    };
    use world_state::{DeterministicSeed, WorldState};

    let source = std::fs::read(path)?;
    let mut world = WorldState::new(DeterministicSeed(11));
    let first = world.spawn_authored(&source, Transform::identity(), true)?;
    let second = world.spawn_authored(&source, Transform::new(v(3.0, 0.0, 0.0), 1.0)?, true)?;
    let scene = Scene::from_world(&world)?;
    let selected = match scene.pick_ray(Ray::new(
        v(0.0, 0.5, -2.0),
        v(0.0, 0.0, 1.0),
        0.0,
        10.0,
    )?)? {
        PickOutcome::Hit(hit) if matches!(hit.target, SemanticTarget::Authored(reference) if reference.instance == first) => {
            hit.target
        }
        other => return Err(format!("authored selection failed: {other:?}").into()),
    };
    let contact = matches!(
        ContactScene::from_world(&world)?.sweep(v(0.0, 0.5, -2.0), v(0.0, 0.0, 4.0), 0.1)?,
        SweepOutcome::Hit(_)
    );
    if !contact {
        return Err("solid authored capsule did not block movement".into());
    }
    let mut runtime = Runtime::new(world, SimulationStep::new(0.01)?);
    runtime.schedule(
        1,
        EnvironmentEventKind::SetAuthoredTransform {
            id: second,
            transform: Transform::new(v(4.0, 0.0, 0.0), 1.0)?,
        },
    )?;
    runtime.schedule(
        2,
        EnvironmentEventKind::SetAuthoredEnabled {
            id: first,
            enabled: false,
        },
    )?;
    runtime.tick()?;
    let bytes = runtime.save_bytes()?;
    let mut loaded = Runtime::load_bytes(&bytes)?;
    if runtime != loaded
        || Scene::from_world(runtime.world())?.primitives().len()
            != Scene::from_world(loaded.world())?.primitives().len()
    {
        return Err("save/load changed authoritative or rendered state".into());
    }
    runtime.tick()?;
    loaded.tick()?;
    if runtime != loaded
        || loaded
            .world()
            .authored_segment(match selected {
                SemanticTarget::Authored(reference) => reference,
                _ => unreachable!(),
            })
            .is_some()
    {
        return Err("replay or stale-reference check failed".into());
    }
    let remaining = Scene::from_world(loaded.world())?;
    Ok(serde_json::json!({
        "kind": "authored-world",
        "definition_revision": loaded.world().authored_definitions()[0].revision().hex(),
        "instances": [first.value(), second.value()],
        "shared_definitions": loaded.world().authored_definitions().len(),
        "initial_primitives": scene.primitives().len(),
        "selected_instance": first.value(),
        "initial_contact": contact,
        "saved_bytes": bytes.len(),
        "replay_equal": runtime == loaded,
        "tick": loaded.time().ticks(),
        "remaining_primitives": remaining.primitives().len(),
        "disabled_contact": matches!(ContactScene::from_world(loaded.world())?.sweep(v(0.0, 0.5, -2.0), v(0.0, 0.0, 4.0), 0.1)?, SweepOutcome::Miss),
    }))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: authoring-inspect FILE.json [--measure|--world]")?;
    let mode = match args.next() {
        None => 0,
        Some(flag) if flag == "--measure" => 1,
        Some(flag) if flag == "--world" => 2,
        _ => return Err("usage: authoring-inspect FILE.json [--measure|--world]".into()),
    };
    if args.next().is_some() {
        return Err("usage: authoring-inspect FILE.json [--measure|--world]".into());
    }
    if mode == 2 {
        println!("{}", world_scenario(Path::new(&path))?);
        return Ok(());
    }
    let started = Instant::now();
    let structure = compile_file(Path::new(&path))?;
    let mut summary = serde_json::json!({
        "kind": "authored-structure",
        "id": structure.id(),
        "source_format_version": structure.format_version(),
        "revision": structure.revision(),
        "content_sha256": structure.content_revision().hex(),
        "compiler_semantics_version": world_authoring::COMPILER_SEMANTICS_VERSION,
        "input_bytes": structure.source().len(),
        "expanded_symbols": structure.expanded_symbols(),
        "segments": structure.segments().len(),
        "max_stack": structure.max_stack_used(),
        "work": structure.work()
    });
    if mode == 1 {
        summary["load_compile_ms"] = serde_json::json!(started.elapsed().as_secs_f64() * 1000.0);
    }
    println!("{summary}");
    Ok(())
}
