//! Native GPU evidence for the bounded mixed-depth diagnostic path.
use analytic_field::Sphere;
use analytic_renderer::{
    Camera, DrawOptions, GpuFieldExecution, GpuFieldPath, GpuFieldRenderMode, GpuRenderer,
    MixedDrawOptions, MixedGpuIdentity,
    field_graph_gpu::{GraphGpuRenderer, GraphTracePolicy, WorldFieldGpuOutcome},
    material::{Material, MaterialPattern, MaterialSet},
    mixed::{MixedRenderOptions, MixedSource, render_mixed_rgba},
};
use spatial_math::{Transform, Vec3};
use std::{sync::mpsc, time::Duration};
use world_authoring::field_graph::{ParameterBinding, ParameterComponent};

#[test]
#[ignore = "requires native GPU; read native mixed identity, distance and status"]
fn native_mixed_diagnostic_selects_nearest_mathematical_object() {
    let adapter = pollster::block_on(
        test_instance().request_adapter(&wgpu::RequestAdapterOptions::default()),
    )
    .unwrap();
    eprintln!("mixed_diagnostic_adapter={:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let mut world = WorldState::new(DeterministicSeed(411));
    let source = br#"{"format_version":1,"id":"diagnostic-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":1.0}]}"#;
    let field = world.spawn_field(source, Transform::identity()).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -4.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let diagnose = |world: &WorldState, camera: Camera| {
        renderer
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
                    show_analytic: true,
                    show_fields: true,
                    diagnostic: true,
                    materials: None,
                    simulation_seconds: 0.0,
                    background: [0.0; 3],
                    light_direction: [0.0, 1.0, 0.0],
                },
                GpuFieldRenderMode {
                    execution: GpuFieldExecution::Interpreter,
                    spatial_guard: true,
                },
            )
            .unwrap()[0]
    };
    let implicit = diagnose(&world, camera);
    assert!(implicit.hit && implicit.field && !implicit.uncertain && !implicit.exhausted);
    assert_eq!(
        implicit.identity,
        Some(MixedGpuIdentity::FieldInstance(field.value()))
    );
    assert_eq!(
        implicit.definition,
        Some(world.field_instances()[0].revision())
    );
    assert!(
        (implicit.distance.unwrap() - 3.0).abs() < 0.003,
        "{implicit:?}"
    );
    world
        .spawn_sphere(
            Sphere::new(0.5).unwrap(),
            Transform::new(v(0.0, 0.0, -2.0), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    let analytic = diagnose(&world, camera);
    assert!(analytic.hit && !analytic.field && !analytic.uncertain && !analytic.exhausted);
    assert_eq!(
        analytic.identity,
        Some(MixedGpuIdentity::AnalyticPrimitive(0))
    );
    assert_eq!(analytic.definition, None);
    assert!(
        (analytic.distance.unwrap() - 1.5).abs() < 0.001,
        "{analytic:?}"
    );
    let inside_camera =
        Camera::look_at(Vec3::ZERO, v(0.0, 0.0, 1.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let mut only_field = WorldState::new(DeterministicSeed(412));
    only_field
        .spawn_field(source, Transform::identity())
        .unwrap();
    let inside = diagnose(&only_field, inside_camera);
    assert!(
        !inside.hit && inside.uncertain && !inside.exhausted,
        "{inside:?}"
    );
    let box_source = br#"{"format_version":1,"id":"diagnostic-box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.4,0.5,0.3]}]}"#;
    let mut box_world = WorldState::new(DeterministicSeed(413));
    let box_id = box_world
        .spawn_field(box_source, Transform::new(v(1.0, 0.0, 0.0), 2.0).unwrap())
        .unwrap();
    let box_camera =
        Camera::look_at(v(1.0, 0.0, -4.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let box_hit = diagnose(&box_world, box_camera);
    assert_eq!(
        box_hit.identity,
        Some(MixedGpuIdentity::FieldInstance(box_id.value()))
    );
    assert!(!box_hit.uncertain && !box_hit.exhausted, "{box_hit:?}");
    assert!(
        (box_hit.distance.unwrap() - 3.4).abs() < 0.003,
        "{box_hit:?}"
    );
    let normal = box_hit.normal.unwrap();
    assert!(normal[2] < -0.99, "{box_hit:?}");
    let source = br#"{"format_version":1,"id":"two-occurrences","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.5}]}"#;
    let mut shared_world = WorldState::new(DeterministicSeed(414));
    let left = shared_world
        .spawn_field(source, Transform::new(v(-1.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    let right = shared_world
        .spawn_field(source, Transform::new(v(1.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    let rule = world_state::ParameterRule {
        binding: ParameterBinding {
            node: "s".into(),
            component: ParameterComponent::SphereRadius,
        },
        uptake: 1.0,
        gain_per_unit: 0.5,
        maximum: 1.0,
        positive_site: true,
        feedback: true,
    };
    shared_world
        .enable_field_parameters(left, vec![rule.clone()])
        .unwrap();
    shared_world
        .enable_field_parameters(right, vec![rule])
        .unwrap();
    shared_world.grow_field_parameters(right, &[0.5]).unwrap();
    let left_camera =
        Camera::look_at(v(-1.0, 0.0, -4.0), v(-1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let right_camera =
        Camera::look_at(v(1.0, 0.0, -4.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
    let first = diagnose(&shared_world, left_camera);
    let second = diagnose(&shared_world, right_camera);
    assert_eq!(
        first.identity,
        Some(MixedGpuIdentity::FieldInstance(left.value()))
    );
    assert_eq!(
        second.identity,
        Some(MixedGpuIdentity::FieldInstance(right.value()))
    );
    assert!((first.distance.unwrap() - 3.5).abs() < 0.003, "{first:?}");
    assert!(
        (second.distance.unwrap() - 3.25).abs() < 0.003,
        "{second:?}"
    );
    eprintln!(
        "mixed_spatial_oracle={}",
        serde_json::json!({
            "implicit_sphere":{"distance":implicit.distance,"identity":format!("{:?}",implicit.identity),"uncertain":implicit.uncertain},
            "nearer_analytic":{"distance":analytic.distance,"identity":format!("{:?}",analytic.identity),"uncertain":analytic.uncertain},
            "inside_start":{"hit":inside.hit,"uncertain":inside.uncertain,"exhausted":inside.exhausted},
            "transformed_box":{"distance":box_hit.distance,"identity":format!("{:?}",box_hit.identity),"normal":box_hit.normal},
            "shared_source_left":{"distance":first.distance,"identity":format!("{:?}",first.identity)},
            "shared_source_right":{"distance":second.distance,"identity":format!("{:?}",second.identity)},
        })
    );
}

#[test]
#[ignore = "requires native GPU; verify native specialization, cache and live revision fallback"]
fn native_compiler_revision_lifecycle() {
    let instance = test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    println!("NATIVE_COMPILER_BACKEND {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let size = [640, 360];
    let source = include_bytes!("../../../examples/field-graph/definitions/balanced-16.json");
    let revised = br#"{"format_version":1,"id":"revised","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.7}]}"#;
    let mut world = WorldState::new(DeterministicSeed(207));
    let first = world.spawn_field(source, Transform::identity()).unwrap();
    let second = world
        .spawn_field(source, Transform::new(v(1.2, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    world
        .spawn_sphere(
            Sphere::new(0.3).unwrap(),
            Transform::new(v(-1.4, 0.0, 0.0), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    let mut materials = MaterialSet::default();
    let draw = |world: &WorldState, camera: Camera, materials: &MaterialSet, execution, guard| {
        let image = renderer
            .draw_mixed_with_execution(
                &device,
                &queue,
                world,
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
                    diagnostic: true,
                    materials: Some(materials),
                    simulation_seconds: 0.0,
                    background: [0.05, 0.075, 0.115],
                    light_direction: [0.5, 0.8, -0.6],
                },
                GpuFieldRenderMode {
                    execution,
                    spatial_guard: guard,
                },
            )
            .unwrap();
        rgba(&device, &queue, &image, size)
    };
    let reference = draw(
        &world,
        camera,
        &materials,
        GpuFieldExecution::Interpreter,
        true,
    );
    let first_auto = draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Interpreter
    );
    assert_eq!(reference, first_auto);
    let wait_ready = |world: &WorldState, materials: &MaterialSet| {
        for _ in 0..500 {
            let image = draw(world, camera, materials, GpuFieldExecution::Auto, true);
            let stats = renderer.compiler_stats().unwrap();
            if stats.active == GpuFieldPath::Specialized {
                return image;
            }
            assert!(
                stats.last_error.is_none(),
                "compiler failed: {:?}",
                stats.last_error
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("native compiler did not become ready in 5 seconds");
    };
    let compiled = wait_ready(&world, &materials);
    let captures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/performance-research-2-captures");
    std::fs::create_dir_all(&captures).unwrap();
    write_bmp(&captures.join("native-interpreter.bmp"), &reference, size);
    write_bmp(&captures.join("native-specialized.bmp"), &compiled, size);
    let max_diff = reference
        .iter()
        .zip(&compiled)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        max_diff <= 1,
        "native interpreter/compiler color drift: {max_diff}"
    );
    let direct = draw(
        &world,
        camera,
        &materials,
        GpuFieldExecution::Interpreter,
        false,
    );
    let compiled_direct = draw(&world, camera, &materials, GpuFieldExecution::Auto, false);
    assert!(
        direct
            .iter()
            .zip(&compiled_direct)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    assert_eq!(
        direct, reference,
        "visual bounds guard changed the supported scene"
    );
    let requests = renderer.compiler_stats().unwrap().requests;
    let moved_camera =
        Camera::look_at(v(0.4, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    draw(
        &world,
        moved_camera,
        &materials,
        GpuFieldExecution::Auto,
        true,
    );
    materials.set_field(
        first,
        Material::new(
            [0.8, 0.2, 0.1],
            [0.0; 3],
            [0.0; 3],
            MaterialPattern::Constant,
        )
        .unwrap(),
    );
    draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
    world
        .set_field_transform(second, Transform::new(v(1.3, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
    assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
    world.replace_field(first, revised).unwrap();
    draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Interpreter
    );
    assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
    world.replace_field(second, revised).unwrap();
    let revised_reference = draw(
        &world,
        camera,
        &materials,
        GpuFieldExecution::Interpreter,
        true,
    );
    let revised_compiled = wait_ready(&world, &materials);
    assert!(
        revised_reference
            .iter()
            .zip(&revised_compiled)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    assert_eq!(renderer.compiler_stats().unwrap().requests, requests + 1);
    assert!(world.replace_field(first, b"invalid graph").is_err());
    draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Specialized
    );
    let rapid_c = br#"{"format_version":1,"id":"rapid-c","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.6}]}"#;
    let rapid_d = br#"{"format_version":1,"id":"rapid-d","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.5}]}"#;
    for source in [rapid_c.as_slice(), rapid_d.as_slice()] {
        world.replace_field(first, source).unwrap();
        world.replace_field(second, source).unwrap();
        draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Interpreter
        );
    }
    let final_reference = draw(
        &world,
        camera,
        &materials,
        GpuFieldExecution::Interpreter,
        true,
    );
    let final_compiled = wait_ready(&world, &materials);
    assert!(
        final_reference
            .iter()
            .zip(&final_compiled)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    let requests_after_edits = renderer.compiler_stats().unwrap().requests;
    world.replace_field(first, source).unwrap();
    world.replace_field(second, source).unwrap();
    draw(&world, camera, &materials, GpuFieldExecution::Auto, true);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Specialized
    );
    assert_eq!(
        renderer.compiler_stats().unwrap().requests,
        requests_after_edits
    );
    let mut tiny_world = WorldState::new(DeterministicSeed(208));
    tiny_world
        .spawn_field(revised, Transform::new(Vec3::ZERO, 0.01).unwrap())
        .unwrap();
    let tiny_camera =
        Camera::look_at(v(0.0, 0.0, -0.06), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let no_materials = MaterialSet::default();
    let tiny_reference = draw(
        &tiny_world,
        tiny_camera,
        &no_materials,
        GpuFieldExecution::Interpreter,
        false,
    );
    let mut tiny_compiled = Vec::new();
    for _ in 0..200 {
        tiny_compiled = draw(
            &tiny_world,
            tiny_camera,
            &no_materials,
            GpuFieldExecution::Auto,
            true,
        );
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Specialized
    );
    assert!(
        tiny_reference
            .iter()
            .zip(&tiny_compiled)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
}

#[test]
#[ignore = "requires native GPU; compare structural shader against interpreter across parameter revisions"]
fn structural_parameters_reuse_pipeline_for_two_revisions() {
    let instance = test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let size = [160, 90];
    let camera = Camera::look_at(v(0.0, 0.0, -5.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let first_source = br#"{"format_version":1,"id":"first","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.7}]}"#;
    let second_source = br#"{"format_version":1,"id":"second","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.5}]}"#;
    let revised_source = br#"{"format_version":1,"id":"revised","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.3}]}"#;
    let mut world = WorldState::new(DeterministicSeed(301));
    let first = world
        .spawn_field(
            first_source,
            Transform::new(v(-0.8, 0.0, 0.0), 1.0).unwrap(),
        )
        .unwrap();
    world
        .spawn_field(
            second_source,
            Transform::new(v(0.8, 0.0, 0.0), 1.0).unwrap(),
        )
        .unwrap();
    world
        .spawn_sphere(
            Sphere::new(0.2).unwrap(),
            Transform::new(v(0.0, -0.8, 0.0), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    let draw = |world: &WorldState, execution| {
        let image = renderer
            .draw_mixed_with_execution(
                &device,
                &queue,
                world,
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
                    diagnostic: true,
                    materials: None,
                    simulation_seconds: 0.0,
                    background: [0.0; 3],
                    light_direction: [0.5, 0.8, -0.6],
                },
                GpuFieldRenderMode {
                    execution,
                    spatial_guard: true,
                },
            )
            .unwrap();
        rgba(&device, &queue, &image, size)
    };
    let reference = draw(&world, GpuFieldExecution::Interpreter);
    for _ in 0..200 {
        let candidate = draw(&world, GpuFieldExecution::Structural);
        if renderer.compiler_stats().unwrap().active == GpuFieldPath::Structural {
            assert!(
                reference
                    .iter()
                    .zip(candidate)
                    .all(|(a, b)| a.abs_diff(b) <= 1)
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Structural
    );
    let captures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/performance-research-3-review");
    std::fs::create_dir_all(&captures).unwrap();
    write_bmp(
        &captures.join("structural-before.bmp"),
        &draw(&world, GpuFieldExecution::Structural),
        size,
    );
    let requests = renderer.compiler_stats().unwrap().requests;
    world.replace_field(first, revised_source).unwrap();
    let revised_reference = draw(&world, GpuFieldExecution::Interpreter);
    assert_ne!(reference, revised_reference);
    let revised = draw(&world, GpuFieldExecution::Structural);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Structural
    );
    assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
    assert!(
        revised_reference
            .iter()
            .zip(revised)
            .all(|(a, b)| a.abs_diff(b) <= 1)
    );
    write_bmp(
        &captures.join("structural-after.bmp"),
        &draw(&world, GpuFieldExecution::Structural),
        size,
    );
    for (source, x) in [
        (br#"{"format_version":1,"id":"third","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.4}]}"#.as_slice(), -1.5),
        (br#"{"format_version":1,"id":"fourth","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.6}]}"#.as_slice(), 1.5),
    ] {
        world
            .spawn_field(source, Transform::new(v(x, 0.4, 0.0), 1.0).unwrap())
            .unwrap();
    }
    assert_eq!(world.field_definitions().len(), 4);
    let four_reference = draw(&world, GpuFieldExecution::Interpreter);
    let four_structural = draw(&world, GpuFieldExecution::Structural);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Structural
    );
    assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
    assert!(
        four_reference
            .iter()
            .zip(four_structural)
            .all(|(a, b)| a.abs_diff(b) <= 1)
    );
    let box_source = br#"{"format_version":1,"id":"box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.3,0.4,0.5]}]}"#;
    world.replace_field(first, box_source).unwrap();
    let fallback_reference = draw(&world, GpuFieldExecution::Interpreter);
    let fallback = draw(&world, GpuFieldExecution::Structural);
    assert_eq!(
        renderer.compiler_stats().unwrap().active,
        GpuFieldPath::Interpreter
    );
    assert_eq!(fallback_reference, fallback);
    assert_eq!(renderer.compiler_stats().unwrap().requests, requests);
}

#[test]
#[ignore = "requires native GPU; heterogeneous compiler image and edit parity"]
fn combined_topologies_preserve_mixed_image_and_fallback() {
    let instance = test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -5.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let size = [160, 90];
    let sphere = br#"{"format_version":1,"id":"sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.7}]}"#;
    let box_graph = br#"{"format_version":1,"id":"box","root":"b","nodes":[{"op":"box","id":"b","half_extent":[0.4,0.5,0.6]}]}"#;
    let mut world = WorldState::new(DeterministicSeed(406));
    let first = world
        .spawn_field(sphere, Transform::new(v(-0.65, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    let second = world
        .spawn_field(box_graph, Transform::new(v(0.65, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    world
        .spawn_sphere(
            Sphere::new(0.25).unwrap(),
            Transform::new(v(0.0, -0.7, 0.5), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    let mut materials = MaterialSet::default();
    materials.set_field(
        first,
        Material::new(
            [0.8, 0.1, 0.1],
            [0.0; 3],
            [0.0; 3],
            MaterialPattern::Constant,
        )
        .unwrap(),
    );
    materials.set_field(
        second,
        Material::new(
            [0.1, 0.8, 0.1],
            [0.0; 3],
            [0.0; 3],
            MaterialPattern::Constant,
        )
        .unwrap(),
    );
    let draw = |world: &WorldState,
                materials: &MaterialSet,
                execution,
                camera: Camera,
                size: [u32; 2],
                policy: GraphTracePolicy,
                guard| {
        let image = renderer
            .draw_mixed_with_execution(
                &device,
                &queue,
                world,
                camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size,
                        normal_debug: false,
                        surface: None,
                        timer: None,
                    },
                    policy,
                    show_analytic: true,
                    show_fields: true,
                    diagnostic: true,
                    materials: Some(materials),
                    simulation_seconds: 0.0,
                    background: [0.0; 3],
                    light_direction: [0.5, 0.8, -0.6],
                },
                GpuFieldRenderMode {
                    execution,
                    spatial_guard: guard,
                },
            )
            .unwrap();
        rgba(&device, &queue, &image, size)
    };
    let captures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/performance-research-4-review");
    std::fs::create_dir_all(&captures).unwrap();
    for (step, source) in [sphere.as_slice(), br#"{"format_version":1,"id":"sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.5}]}"#].iter().enumerate() {
        if step > 0 {
            world.replace_field(first, source).unwrap();
        }
        let reference = draw(&world, &materials, GpuFieldExecution::Interpreter, camera, size, GraphTracePolicy::default(), true);
        let mut compiled = None;
        for _ in 0..200 {
            let candidate = draw(&world, &materials, GpuFieldExecution::Combined, camera, size, GraphTracePolicy::default(), true);
            let state = renderer.compiler_stats().unwrap();
            assert!(state.last_error.is_none(), "combined compiler: {:?}", state.last_error);
            if state.active == GpuFieldPath::Combined {
                compiled = Some(candidate);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let compiled = compiled.expect("combined shader did not become ready");
        assert!(reference.iter().zip(&compiled).all(|(a, b)| a.abs_diff(*b) <= 1));
        write_bmp(&captures.join(format!("combined-{step}.bmp")), &compiled, size);
    }
    assert_eq!(renderer.compiler_stats().unwrap().requests, 1);
    let smooth = br#"{"format_version":1,"id":"smooth","root":"u","nodes":[{"op":"sphere","id":"a","radius":0.35},{"op":"translate","id":"b","child":"a","offset":[0.35,0,0]},{"op":"smooth_union","id":"u","left":"a","right":"b","width":0.2}]}"#;
    let transformed = br#"{"format_version":1,"id":"transformed","root":"u","nodes":[{"op":"box","id":"b","half_extent":[0.3,0.3,0.3]},{"op":"scale","id":"s","child":"b","factor":1.2},{"op":"translate","id":"t","child":"s","offset":[0.4,0,0]},{"op":"union","id":"u","left":"b","right":"t"}]}"#;
    for (source, x, color) in [
        (smooth.as_slice(), -1.5, [0.1, 0.2, 0.9]),
        (transformed.as_slice(), 1.5, [0.9, 0.7, 0.1]),
    ] {
        let id = world
            .spawn_field(source, Transform::new(v(x, 0.7, 0.0), 1.0).unwrap())
            .unwrap();
        materials.set_field(
            id,
            Material::new(color, [0.0; 3], [0.0; 3], MaterialPattern::Constant).unwrap(),
        );
    }
    let reference = draw(
        &world,
        &materials,
        GpuFieldExecution::Interpreter,
        camera,
        size,
        GraphTracePolicy::default(),
        true,
    );
    let mut compiled = None;
    for _ in 0..200 {
        let candidate = draw(
            &world,
            &materials,
            GpuFieldExecution::Combined,
            camera,
            size,
            GraphTracePolicy::default(),
            true,
        );
        let state = renderer.compiler_stats().unwrap();
        assert!(
            state.last_error.is_none(),
            "combined compiler: {:?}",
            state.last_error
        );
        if state.active == GpuFieldPath::Combined {
            compiled = Some(candidate);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let compiled = compiled.expect("four-topology shader did not become ready");
    assert!(
        reference
            .iter()
            .zip(&compiled)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    write_bmp(&captures.join("combined-four.bmp"), &compiled, size);
    assert_eq!(renderer.compiler_stats().unwrap().requests, 2);
    let broad_size = [640, 360];
    let inside = Camera::look_at(
        v(-0.65, 0.0, 0.0),
        v(-0.65, 0.0, 1.0),
        v(0.0, 1.0, 0.0),
        0.9,
    )
    .unwrap();
    for (label, scale, view, policy, guard) in [
        ("small", 0.01, camera, GraphTracePolicy::default(), true),
        ("medium", 0.1, camera, GraphTracePolicy::default(), false),
        ("ordinary", 1.0, camera, GraphTracePolicy::default(), true),
        ("inside", 1.0, inside, GraphTracePolicy::default(), true),
        ("large", 100.0, camera, GraphTracePolicy::default(), true),
        (
            "exhausted",
            1.0,
            camera,
            GraphTracePolicy {
                iterations: 1,
                ..GraphTracePolicy::default()
            },
            false,
        ),
    ] {
        world
            .set_field_transform(first, Transform::new(v(-0.65, 0.0, 0.0), scale).unwrap())
            .unwrap();
        let reference = draw(
            &world,
            &materials,
            GpuFieldExecution::Interpreter,
            view,
            broad_size,
            policy,
            guard,
        );
        let compiled = draw(
            &world,
            &materials,
            GpuFieldExecution::Combined,
            view,
            broad_size,
            policy,
            guard,
        );
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Combined
        );
        let max_diff = reference
            .iter()
            .zip(&compiled)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max_diff <= 1, "{label} image drift: {max_diff}");
        let direct = draw(
            &world,
            &materials,
            GpuFieldExecution::Combined,
            view,
            broad_size,
            policy,
            !guard,
        );
        let guard_diff = direct
            .iter()
            .zip(&compiled)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(guard_diff <= 1, "{label} visual guard drift: {guard_diff}");
        println!(
            "COMBINED_PARITY {label} pixels={} max_channel_delta={max_diff} guard_delta={guard_diff}",
            broad_size[0] * broad_size[1]
        );
        if label == "ordinary" {
            write_bmp(
                &captures.join("combined-interpreter-640.bmp"),
                &reference,
                broad_size,
            );
            write_bmp(
                &captures.join("combined-compiled-640.bmp"),
                &compiled,
                broad_size,
            );
        }
    }
    assert_eq!(renderer.compiler_stats().unwrap().requests, 2);
}
use world_state::{DeterministicSeed, WorldState};

fn test_instance() -> wgpu::Instance {
    if std::env::var("GENESIS_TEST_GPU_BACKEND").as_deref() == Ok("dx12") {
        let mut options = wgpu::InstanceDescriptor::new_without_display_handle();
        options.backends = wgpu::Backends::DX12;
        if let Ok(dxc_path) = std::env::var("GENESIS_TEST_DXC_PATH") {
            options.backend_options.dx12.shader_compiler =
                wgpu::Dx12Compiler::DynamicDxc { dxc_path };
        }
        wgpu::Instance::new(options)
    } else {
        wgpu::Instance::default()
    }
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z).unwrap()
}

fn rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &wgpu::Texture,
    size: [u32; 2],
) -> Vec<u8> {
    let padded = (size[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mixed native test readback"),
        size: u64::from(padded) * u64::from(size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mixed native test copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
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
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = slice.get_mapped_range().unwrap();
    let result = mapped
        .chunks_exact(padded as usize)
        .flat_map(|row| row[..(size[0] * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}

fn pixel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &wgpu::Texture,
    size: [u32; 2],
) -> [u8; 4] {
    let pixels = rgba(device, queue, image, size);
    let offset = ((size[1] / 2 * size[0] + size[0] / 2) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn write_bmp(path: &std::path::Path, pixels: &[u8], size: [u32; 2]) {
    let [width, height] = size;
    let stride = (width * 3).div_ceil(4) * 4;
    let data_bytes = stride * height;
    let mut bmp = Vec::with_capacity((54 + data_bytes) as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(54 + data_bytes).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(height as i32).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    for y in (0..height).rev() {
        let row_start = bmp.len();
        for pixel in
            pixels[(y * width * 4) as usize..((y + 1) * width * 4) as usize].chunks_exact(4)
        {
            bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        bmp.resize(row_start + stride as usize, 0);
    }
    std::fs::write(path, bmp).unwrap();
}

#[test]
#[ignore = "requires native GPU; compare bounded mixed viewport against direct GPU field queries"]
fn bounded_mixed_viewport_matches_direct_field_queries() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let reference = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.9).unwrap();
    let size = [256, 256];
    let rays = (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| camera.ray(x, y, size[0], size[1]).unwrap()))
        .collect::<Vec<_>>();
    let source = br#"{"format_version":1,"id":"parity","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.9}]}"#;
    let mut world = WorldState::new(DeterministicSeed(104));
    world.spawn_field(source, Transform::identity()).unwrap();
    for case in 0..3 {
        if case == 1 {
            world
                .spawn_field(source, Transform::new(v(0.5, 0.0, 0.0), 1.0).unwrap())
                .unwrap();
        } else if case == 2 {
            let id = world.field_instances()[0].id();
            world
                .set_field_transform(id, Transform::new(v(40.0, 0.0, 0.0), 1.0).unwrap())
                .unwrap();
        }
        let direct = reference
            .query_world_fields(
                &device,
                &queue,
                &world,
                camera,
                &rays,
                GraphTracePolicy::default(),
            )
            .unwrap();
        let draw_case = || {
            renderer.draw_mixed(
                &device,
                &queue,
                &world,
                camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size,
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
                    light_direction: [0.5, 0.8, -0.6],
                },
            )
        };
        let mut image = draw_case().unwrap();
        for _ in 0..200 {
            if renderer.compiler_stats().unwrap().active == GpuFieldPath::Specialized {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            image = draw_case().unwrap();
        }
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Specialized
        );
        let pixels = rgba(&device, &queue, &image, size);
        let mut compared = 0usize;
        for (index, (outcome, pixel)) in direct.iter().zip(pixels.chunks_exact(4)).enumerate() {
            let lit = pixel[..3] != [0, 0, 0];
            match outcome {
                WorldFieldGpuOutcome::Hit { .. } => {
                    assert!(lit, "case {case} direct hit lost at pixel {index}")
                }
                WorldFieldGpuOutcome::Miss => assert!(
                    !lit,
                    "case {case} direct miss changed at pixel {index}: {pixel:?}"
                ),
                WorldFieldGpuOutcome::Uncertain { .. } | WorldFieldGpuOutcome::Exhausted { .. } => {
                    assert!(
                        lit,
                        "case {case} direct uncertainty hidden at pixel {index}"
                    );
                }
            }
            compared += 1;
        }
        assert!(compared > 65_000);
        println!(
            "bounded_parity case={case} compared={compared} adapter={}",
            adapter.get_info().name
        );
    }
    let mut tangent_world = WorldState::new(DeterministicSeed(105));
    tangent_world
        .spawn_field(source, Transform::identity())
        .unwrap();
    for y in [1.0 + 1e-8, 1.05, 1.2] {
        let tangent_camera =
            Camera::look_at(v(0.0, y, -6.0), v(0.0, y, 0.0), v(0.0, 1.0, 0.0), 0.9).unwrap();
        let ray = tangent_camera.ray(0, 0, 1, 1).unwrap();
        let direct = reference
            .query_world_fields(
                &device,
                &queue,
                &tangent_world,
                tangent_camera,
                &[ray],
                GraphTracePolicy::default(),
            )
            .unwrap();
        let image = renderer
            .draw_mixed(
                &device,
                &queue,
                &tangent_world,
                tangent_camera,
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
                    light_direction: [0.5, 0.8, -0.6],
                },
            )
            .unwrap();
        assert_eq!(
            renderer.compiler_stats().unwrap().active,
            GpuFieldPath::Specialized
        );
        let lit = rgba(&device, &queue, &image, [1, 1])[..3] != [0, 0, 0];
        match direct[0] {
            WorldFieldGpuOutcome::Hit { .. } => assert!(lit, "near-tangent hit lost at y={y}"),
            WorldFieldGpuOutcome::Miss => assert!(!lit, "near-tangent miss changed at y={y}"),
            _ => assert!(lit, "uncertainty hidden at y={y}"),
        }
    }
    for (x, scale) in [(1000.0, 0.1), (-1000.0, 100.0)] {
        let mut scaled_world = WorldState::new(DeterministicSeed(106));
        scaled_world
            .spawn_field(source, Transform::new(v(x, 0.0, 0.0), scale).unwrap())
            .unwrap();
        let scaled_camera = Camera::look_at(
            v(x, 0.0, -6.0 * scale),
            v(x, 0.0, 0.0),
            v(0.0, 1.0, 0.0),
            0.9,
        )
        .unwrap();
        let rays = (0..size[1])
            .flat_map(|y| {
                (0..size[0])
                    .map(move |column| scaled_camera.ray(column, y, size[0], size[1]).unwrap())
            })
            .collect::<Vec<_>>();
        let direct = reference
            .query_world_fields(
                &device,
                &queue,
                &scaled_world,
                scaled_camera,
                &rays,
                GraphTracePolicy::default(),
            )
            .unwrap();
        let image = renderer
            .draw_mixed(
                &device,
                &queue,
                &scaled_world,
                scaled_camera,
                MixedDrawOptions {
                    draw: DrawOptions {
                        size,
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
                    light_direction: [0.5, 0.8, -0.6],
                },
            )
            .unwrap();
        let pixels = rgba(&device, &queue, &image, size);
        for (index, (outcome, pixel)) in direct.iter().zip(pixels.chunks_exact(4)).enumerate() {
            let lit = pixel[..3] != [0, 0, 0];
            match outcome {
                WorldFieldGpuOutcome::Miss => {
                    assert!(!lit, "scale {scale} direct miss changed at {index}")
                }
                _ => assert!(
                    lit,
                    "scale {scale} direct result hidden at {index}: {outcome:?}"
                ),
            }
        }
        println!("bounded_parity scale={scale} compared={}", rays.len());
    }
}

#[test]
#[ignore = "requires native GPU; actual mixed compute and image readback"]
fn native_mixed_depth_matches_reference_at_center() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let field_renderer = pollster::block_on(GraphGpuRenderer::new(&device)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let size = [33, 33];
    let source = br#"{"format_version":1,"id":"native-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.6}]}"#;
    let mut world = WorldState::new(DeterministicSeed(34));
    world
        .spawn_sphere(Sphere::new(0.6).unwrap(), Transform::identity(), 0.0)
        .unwrap();
    let field = world
        .spawn_field(source, Transform::new(v(0.0, 0.0, 2.0), 1.0).unwrap())
        .unwrap();
    let native = |world: &WorldState| {
        let image = renderer
            .draw_mixed(
                &device,
                &queue,
                world,
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
                    diagnostic: true,
                    materials: None,
                    simulation_seconds: 0.0,
                    background: [0.05, 0.075, 0.115],
                    light_direction: [0.5, 0.8, -0.6],
                },
            )
            .unwrap();
        pixel(&device, &queue, &image, size)
    };
    let empty = WorldState::new(DeterministicSeed(35));
    let empty_pixel = native(&empty);
    assert!(empty_pixel[0] < 40 && empty_pixel[2] < 50);
    let behind = native(&world);
    let reference = render_mixed_rgba(
        &renderer,
        &field_renderer,
        &device,
        &queue,
        &world,
        camera,
        MixedRenderOptions {
            size,
            policy: GraphTracePolicy::default(),
        },
    )
    .unwrap();
    assert!(matches!(
        reference.sources[16 + 16 * 33],
        MixedSource::Analytic(_)
    ));
    world
        .set_field_transform(field, Transform::new(v(0.0, 0.0, -2.0), 1.0).unwrap())
        .unwrap();
    let front = native(&world);
    let reference = render_mixed_rgba(
        &renderer,
        &field_renderer,
        &device,
        &queue,
        &world,
        camera,
        MixedRenderOptions {
            size,
            policy: GraphTracePolicy::default(),
        },
    )
    .unwrap();
    assert_eq!(reference.sources[16 + 16 * 33], MixedSource::Field(field));
    assert_ne!(behind, front, "GPU depth compositor ignored front field");
    assert!(front[2] > front[0], "front field should be blue: {front:?}");
    world.set_field_enabled(field, false).unwrap();
    assert_eq!(native(&world), behind);
    let hidden = renderer
        .draw_mixed(
            &device,
            &queue,
            &world,
            camera,
            MixedDrawOptions {
                draw: DrawOptions {
                    size,
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
                background: [0.05, 0.075, 0.115],
                light_direction: [0.5, 0.8, -0.6],
            },
        )
        .unwrap();
    assert_eq!(pixel(&device, &queue, &hidden, size), empty_pixel);
    world.set_field_enabled(field, true).unwrap();
    world
        .spawn_sphere(
            Sphere::new(0.8).unwrap(),
            Transform::new(v(3.0, 0.0, 0.0), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    world
        .spawn_field(
            include_bytes!("../../../examples/field-graph/definitions/smooth-bridge.json"),
            Transform::new(v(-2.5, 0.0, 0.0), 0.6).unwrap(),
        )
        .unwrap();
    let size = [512, 288];
    let capture_camera =
        Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 1.0).unwrap();
    let image = renderer
        .draw_mixed(
            &device,
            &queue,
            &world,
            capture_camera,
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
                diagnostic: true,
                materials: None,
                simulation_seconds: 0.0,
                background: [0.05, 0.075, 0.115],
                light_direction: [0.5, 0.8, -0.6],
            },
        )
        .unwrap();
    let pixels = rgba(&device, &queue, &image, size);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/mixed-captures/native-mixed.bmp");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_bmp(&path, &pixels, size);
    world
        .spawn_field(source, Transform::new(v(0.0, 0.0, -6.0), 1.0).unwrap())
        .unwrap();
    let diagnostic = native(&world);
    assert!(
        diagnostic[0] > diagnostic[1] * 3 && diagnostic[0] > diagnostic[2] * 3,
        "inside-start uncertainty must remain visible: {diagnostic:?}"
    );
}

#[test]
#[ignore = "requires native GPU; CPU/WGSL material parity and actual captures"]
fn mathematical_materials_match_cpu_and_reload_without_geometry_upload() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer =
        pollster::block_on(GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let camera = Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 0.8).unwrap();
    let source = br#"{"format_version":1,"id":"material-sphere","root":"s","nodes":[{"op":"sphere","id":"s","radius":0.9}]}"#;
    let mut world = WorldState::new(DeterministicSeed(36));
    let field = world.spawn_field(source, Transform::identity()).unwrap();
    let mut materials = MaterialSet::default();
    let gradient = Material::new(
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0; 3],
        MaterialPattern::Gradient {
            axis: [1.0, 0.0, 0.0],
            frequency: 1.0,
            phase: 0.5,
            speed: 0.0,
        },
    )
    .unwrap();
    materials.set_field(field, gradient);
    let size = [65, 65];
    let draw = |world: &WorldState, materials: &MaterialSet, camera: Camera, size: [u32; 2]| {
        let image = renderer
            .draw_mixed(
                &device,
                &queue,
                world,
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
                    diagnostic: true,
                    materials: Some(materials),
                    simulation_seconds: 0.0,
                    background: [0.05, 0.075, 0.115],
                    light_direction: [0.5, 0.8, -0.6],
                },
            )
            .unwrap();
        rgba(&device, &queue, &image, size)
    };
    let center = (32 + 32 * size[0]) as usize * 4;
    let pixels = draw(&world, &materials, camera, size);
    let ray = camera.ray(32, 32, size[0], size[1]).unwrap();
    let hit = analytic_renderer::field_graph_gpu::GraphGpuRenderer::new(&device);
    let field_renderer = pollster::block_on(hit).unwrap();
    let field_hit = field_renderer
        .query_world_fields(
            &device,
            &queue,
            &world,
            camera,
            &[ray],
            GraphTracePolicy::default(),
        )
        .unwrap()[0];
    let (distance, normal) = match field_hit {
        analytic_renderer::field_graph_gpu::WorldFieldGpuOutcome::Hit {
            distance, normal, ..
        } => (distance, normal),
        other => panic!("expected field hit: {other:?}"),
    };
    let point = [0.0, 0.0, -6.0 + distance];
    let expected = gradient.sample(point, 0.0).unwrap().lit(normal).unwrap();
    for channel in 0..3 {
        assert!(
            (f32::from(pixels[center + channel]) / 255.0 - expected[channel]).abs() < 0.025,
            "channel {channel}: GPU {:?}, CPU {expected:?}",
            &pixels[center..center + 4]
        );
    }
    let light = [0.0, 0.0, -1.0];
    let custom_image = renderer
        .draw_mixed(
            &device,
            &queue,
            &world,
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
                diagnostic: true,
                materials: Some(&materials),
                simulation_seconds: 0.0,
                background: [0.05, 0.075, 0.115],
                light_direction: light,
            },
        )
        .unwrap();
    let custom_pixels = rgba(&device, &queue, &custom_image, size);
    let custom_expected = gradient
        .sample(point, 0.0)
        .unwrap()
        .lit_with_light(normal, light)
        .unwrap();
    for channel in 0..3 {
        assert!(
            (f32::from(custom_pixels[center + channel]) / 255.0 - custom_expected[channel]).abs()
                < 0.025
        );
    }
    let before = renderer.field_cache_stats().unwrap();
    let bands = Material::new(
        [0.05, 0.05, 0.05],
        [0.0, 0.0, 1.0],
        [0.0; 3],
        MaterialPattern::Bands {
            axis: [1.0, 0.0, 0.0],
            frequency: 1.0,
            phase: 0.6,
            speed: 0.0,
        },
    )
    .unwrap();
    materials.set_field(field, bands);
    let bands_pixels = draw(&world, &materials, camera, size);
    assert_ne!(pixels[center..center + 4], bands_pixels[center..center + 4]);
    let after = renderer.field_cache_stats().unwrap();
    assert_eq!(before.node_uploads, after.node_uploads);
    assert_eq!(before.node_bytes, after.node_bytes);
    assert!(after.material_uploads > before.material_uploads);
    let emissive = Material::new(
        [0.02, 0.02, 0.08],
        [0.0; 3],
        [0.8, 0.1, 0.0],
        MaterialPattern::Constant,
    )
    .unwrap();
    materials.set_field(field, emissive);
    let emissive_pixels = draw(&world, &materials, camera, size);
    assert!(emissive_pixels[center] > bands_pixels[center] + 80);
    let second = world
        .spawn_field(source, Transform::new(v(2.0, 0.0, 0.0), 1.0).unwrap())
        .unwrap();
    materials.set_field(second, bands);
    assert_eq!(world.field_definitions().len(), 1);
    let sphere = world
        .spawn_sphere(
            Sphere::new(0.75).unwrap(),
            Transform::new(v(-2.0, 0.0, 0.0), 1.0).unwrap(),
            0.0,
        )
        .unwrap();
    materials.set_sphere(sphere, gradient);
    let capture_size = [512, 288];
    let capture_camera =
        Camera::look_at(v(0.0, 0.0, -6.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 1.0).unwrap();
    let image = draw(&world, &materials, capture_camera, capture_size);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/mixed-captures/materials.bmp");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_bmp(&path, &image, capture_size);
    let analytic_camera =
        Camera::look_at(v(-2.0, 0.0, -6.0), v(-2.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.8).unwrap();
    let analytic_pixels = draw(&world, &materials, analytic_camera, size);
    let ray = analytic_camera.ray(32, 32, 65, 65).unwrap();
    let hit = renderer
        .query(
            &device,
            &queue,
            &analytic_renderer::Scene::from_world(&world).unwrap(),
            &[ray],
        )
        .unwrap()[0]
        .unwrap();
    let point = [-2.0, 0.0, -6.0 + hit.distance];
    let expected = gradient
        .sample(point, 0.0)
        .unwrap()
        .lit(hit.normal)
        .unwrap();
    for channel in 0..3 {
        assert!(
            (f32::from(analytic_pixels[center + channel]) / 255.0 - expected[channel]).abs()
                < 0.025,
            "analytic channel {channel}: GPU {:?}, CPU {expected:?}",
            &analytic_pixels[center..center + 4]
        );
    }
    for x in [8.0, 9.0, 10.0, 11.0] {
        world
            .spawn_sphere(
                Sphere::new(0.3).unwrap(),
                Transform::new(v(x, 0.0, 0.0), 1.0).unwrap(),
                0.0,
            )
            .unwrap();
    }
    draw(&world, &materials, camera, size);
    let bvh_before = renderer.acceleration_stats().unwrap().rebuilds;
    let nodes_before = renderer.field_cache_stats().unwrap().node_uploads;
    materials.set_sphere(sphere, emissive);
    draw(&world, &materials, camera, size);
    assert_eq!(renderer.acceleration_stats().unwrap().rebuilds, bvh_before);
    assert_eq!(
        renderer.field_cache_stats().unwrap().node_uploads,
        nodes_before
    );
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
