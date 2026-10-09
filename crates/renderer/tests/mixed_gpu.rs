//! Native GPU evidence for the bounded mixed-depth diagnostic path.
use analytic_field::Sphere;
use analytic_renderer::{
    Camera, GpuRenderer,
    field_graph_gpu::{GraphGpuRenderer, GraphTracePolicy},
    mixed::{MixedRenderOptions, MixedSource, render_mixed_rgba},
};
use spatial_math::{Transform, Vec3};
use world_state::{DeterministicSeed, WorldState};

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

#[test]
#[ignore = "requires native GPU; mixed analytic/implicit depth readback"]
fn mixed_depth_tracks_world_changes_and_retains_empty_paths() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let analytic =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let fields = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let mut world = WorldState::new(DeterministicSeed(30));
    let render = |world: &WorldState| {
        render_mixed_rgba(
            &analytic,
            &fields,
            &device,
            &queue,
            world,
            camera,
            MixedRenderOptions {
                size: [33, 33],
                policy: GraphTracePolicy::default(),
            },
        )
        .unwrap()
    };
    assert!(
        render_mixed_rgba(
            &analytic,
            &fields,
            &device,
            &queue,
            &world,
            camera,
            MixedRenderOptions {
                size: [33, 33],
                policy: GraphTracePolicy {
                    iterations: 0,
                    ..GraphTracePolicy::default()
                }
            }
        )
        .is_err()
    );
    let center = 16 + 16 * 33;
    assert_eq!(render(&world).sources[center], MixedSource::Miss);
    world
        .spawn_sphere(
            Sphere::new(0.6).unwrap(),
            Transform::new(Vec3::ZERO, 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    let source = br#"{"format_version":1,"id":"mixed-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.6}]}"#;
    let field = world
        .spawn_field(source, Transform::new(v(0.0, 0.0, 2.0), 1.0).unwrap())
        .unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    world
        .set_field_transform(field, Transform::new(v(0.0, 0.0, -2.0), 1.0).unwrap())
        .unwrap();
    assert_eq!(render(&world).sources[center], MixedSource::Field(field));
    world.set_field_enabled(field, false).unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    world.set_field_enabled(field, true).unwrap();
    let second = world
        .spawn_field(source, Transform::new(v(2.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    assert_eq!(world.field_definitions().len(), 1);
    assert_eq!(render(&world).sources[center], MixedSource::Field(field));
    world.remove_field(field).unwrap();
    assert!(matches!(
        render(&world).sources[center],
        MixedSource::Analytic(_)
    ));
    assert_ne!(field, second);
    println!(
        "mixed_depth adapter={} backend={:?} pixels=1089",
        adapter.get_info().name,
        adapter.get_info().backend
    );
}
