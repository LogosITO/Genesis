//! Executable 0.1 vertical slice.
use analytic_field::Sphere;
use spatial_math::{Transform, Vec3};
use world_runtime::Runtime;
use world_simulation::SimulationStep;
use world_state::{DeterministicSeed, WorldState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = WorldState::new(DeterministicSeed(42));
    let id = world.spawn_sphere(Sphere::new(1.0)?, Transform::identity(), 0.25)?;
    let mut runtime = Runtime::new(world, SimulationStep::new(0.5)?);
    for _ in 0..4 {
        runtime.tick()?;
    }
    let entity = runtime.world().entity(id).expect("stable ID must exist");
    let sample = entity.signed_distance(Vec3::new(2.0, 0.0, 0.0)?)?;
    println!(
        "{{\"entity_id\":{},\"ticks\":{},\"radius\":{},\"signed_distance\":{}}}",
        id.value(),
        runtime.time().ticks(),
        entity.sphere().radius(),
        sample
    );
    Ok(())
}
