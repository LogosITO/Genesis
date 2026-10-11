//! Native cache-churn probe with sequential worlds, each within the one-definition limit.

use analytic_renderer::{
    Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
    MixedDrawOptions, MixedGpuIdentity, field_graph_gpu::GraphTracePolicy,
};
use spatial_math::{Transform, Vec3};
use std::time::Duration;
use world_state::{DeterministicSeed, WorldState};

fn world(radius: f64) -> WorldState {
    let source = format!(
        r#"{{"format_version":1,"id":"qa1_churn","root":"s","nodes":[{{"op":"sphere","id":"s","radius":{radius}}}]}}"#
    );
    let mut world = WorldState::new(DeterministicSeed(0x5141));
    world
        .spawn_field(source.as_bytes(), Transform::identity())
        .unwrap();
    world
}

#[test]
#[ignore = "native GPU; cache eviction and stale-pipeline diagnostic readback"]
fn exact_pipeline_churn_preserves_current_revision() {
    let adapter = pollster::block_on(
        wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let camera = Camera::look_at(
        Vec3::new(0.0, 0.0, -4.0).unwrap(),
        Vec3::ZERO,
        Vec3::new(0.0, 1.0, 0.0).unwrap(),
        0.9,
    )
    .unwrap();
    let draw = |world: &WorldState, radius: f64| {
        let result = renderer
            .diagnose_mixed_camera(
                &device,
                &queue,
                world,
                camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size: [1, 1],
                        normal_debug: false,
                        surface: None,
                        timer: None,
                    },
                    policy: GraphTracePolicy::default(),
                    show_analytic: false,
                    show_fields: true,
                    diagnostic: true,
                    materials: None,
                    simulation_seconds: 0.0,
                    background: [0.0; 3],
                    light_direction: [0.0, 1.0, 0.0],
                },
                GpuFieldRenderMode {
                    execution: GpuFieldExecution::Auto,
                    spatial_guard: true,
                },
            )
            .unwrap()[0];
        assert!(
            result.hit && !result.uncertain && !result.exhausted,
            "{radius}: {result:?}"
        );
        assert_eq!(result.identity, Some(MixedGpuIdentity::FieldInstance(0)));
        assert_eq!(
            result.definition,
            Some(world.field_instances()[0].revision())
        );
        assert!(
            (f64::from(result.distance.unwrap()) - (4.0 - radius)).abs() < 0.003,
            "{radius}: {result:?}"
        );
        assert!(result.normal.unwrap()[2] < -0.99, "{radius}: {result:?}");
        assert!(renderer.compiler_stats().unwrap().cached_pipelines <= 4);
        result.path
    };
    let mut fallback = 0;
    for radius in [0.5, 0.6, 0.7] {
        let current = world(radius);
        fallback += usize::from(draw(&current, radius) == GpuFieldPath::Interpreter);
    }
    for radius in [0.7, 0.8, 0.9, 1.0, 1.1, 1.2] {
        let current = world(radius);
        let mut ready = false;
        for _ in 0..300 {
            if draw(&current, radius) == GpuFieldPath::Specialized {
                ready = true;
                break;
            }
            fallback += 1;
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            ready,
            "radius {radius} never compiled: {:?}",
            renderer.compiler_stats()
        );
        let stats = renderer.compiler_stats().unwrap();
        println!(
            "QA1_CACHE radius={radius} requests={} hits={} resident={} fallback_frames={fallback} source_ms={:?} pipeline_ms={:?}",
            stats.requests,
            stats.cache_hits,
            stats.cached_pipelines,
            stats.source_ms,
            stats.pipeline_ms
        );
    }
    let before = renderer.compiler_stats().unwrap().requests;
    let first = world(0.5);
    let first_path = draw(&first, 0.5);
    let after = renderer.compiler_stats().unwrap();
    assert_eq!(
        first_path,
        GpuFieldPath::Interpreter,
        "oldest key was not evicted"
    );
    assert_eq!(after.requests, before + 1, "evicted key was not recompiled");
    println!(
        "QA1_CACHE_EVICTION before_requests={before} after_requests={} fallback_frames={fallback} adapter={:?}",
        after.requests,
        adapter.get_info()
    );
}
