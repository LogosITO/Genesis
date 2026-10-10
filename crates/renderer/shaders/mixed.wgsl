// The analytic intersection and Field Graph evaluator are prepended by Rust.
struct FieldConfig {
    origin: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    view: vec4<f32>,
    trace: vec4<f32>,
    info: vec4<u32>,
}
struct Node { links: vec4<u32>, param: vec4<f32> }
struct Outcome { distance_normal: vec4<f32>, info: vec4<u32> }
struct Frame { point: vec3<f32>, first: f32, index: u32, phase: u32 }
struct FieldInstance { position_scale: vec4<f32>, info: vec4<u32> }
struct MixedOptions { trace: vec4<f32>, info: vec4<u32> }
@group(1) @binding(0) var<storage, read> field_nodes: array<Node>;
@group(1) @binding(1) var<storage, read> field_instances: array<FieldInstance>;
@group(1) @binding(2) var<uniform> mixed_options: MixedOptions;
var<private> field_config: FieldConfig;

// FIELD_EVALUATOR_INSERTION_POINT

@compute @workgroup_size(8, 8)
fn render_mixed(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u32(camera.view.x) || gid.y >= u32(camera.view.y)) { return; }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / camera.view.xy;
    let direction = normalize(camera.forward.xyz + (uv.x * 2.0 - 1.0) * camera.right.xyz + (1.0 - uv.y * 2.0) * camera.up.xyz);
    var analytic = no_hit();
    if ((mixed_options.info.z & 1u) != 0u) {
        analytic = trace(camera.origin.xyz, direction, camera.view.z, camera.view.w);
    }
    var field_distance = camera.view.w;
    var field_normal = vec3<f32>(0.0);
    var field_hit = false;
    var uncertain = false;
    var exhausted = false;
    var ambiguous = false;
    if ((mixed_options.info.z & 2u) != 0u) {
        for (var i = 0u; i < mixed_options.info.x; i++) {
            let instance = field_instances[i];
            let scale = instance.position_scale.w;
            field_config.info = vec4<u32>(instance.info.x, mixed_options.info.y, mixed_options.info.w, 0u);
            field_config.trace = mixed_options.trace;
            let origin = (camera.origin.xyz - instance.position_scale.xyz) / scale;
            let result = field_trace(origin, direction, camera.view.z / scale, camera.view.w / scale);
            if (result.info.x == 2u) { uncertain = true; }
            if (result.info.x == 3u) { exhausted = true; }
            if (result.info.x == 1u) {
                let distance = result.distance_normal.w * scale;
                if (!finite(distance)) {
                    uncertain = true;
                } else {
                    let band = 1e-4 * max(1.0, max(abs(field_distance), abs(distance)));
                    if (field_hit && abs(distance - field_distance) <= band) { ambiguous = true; }
                    if (!field_hit || distance < field_distance) {
                        field_distance = distance;
                        field_normal = result.distance_normal.xyz;
                        field_hit = true;
                    }
                }
            }
        }
    }
    var color = vec3<f32>(0.035, 0.055, 0.09) + vec3<f32>(0.03, 0.04, 0.05) * (1.0 - uv.y);
    if (field_hit && analytic.valid) {
        let band = 1e-4 * max(1.0, max(abs(field_distance), abs(analytic.distance)));
        if (abs(field_distance - analytic.distance) <= band) { ambiguous = true; }
    }
    let use_field = field_hit && (!analytic.valid || field_distance < analytic.distance);
    if (analytic.valid && !use_field) {
        let base = objects[analytic.primitive_index].color.xyz;
        let light = normalize(vec3<f32>(0.5, 0.8, -0.6));
        let diffuse = 0.18 + 0.82 * max(dot(analytic.normal, light), 0.0);
        color = base * mix(diffuse, 1.0, objects[analytic.primitive_index].color.w);
        if (camera.settings.x > 0.5) { color = analytic.normal * 0.5 + vec3<f32>(0.5); }
    } else if (use_field) {
        let light = normalize(vec3<f32>(0.5, 0.8, -0.6));
        color = vec3<f32>(0.1, 0.65, 0.8) * (0.18 + 0.82 * max(dot(field_normal, light), 0.0));
        if (camera.settings.x > 0.5) { color = field_normal * 0.5 + vec3<f32>(0.5); }
    }
    if (uncertain || exhausted || ambiguous) {
        if ((mixed_options.info.z & 4u) != 0u || (!field_hit && !analytic.valid)) {
            if (uncertain) { color = vec3<f32>(0.9, 0.1, 0.1); }
            else if (exhausted) { color = vec3<f32>(0.9, 0.75, 0.05); }
            else { color = vec3<f32>(0.86, 0.14, 0.78); }
        } else {
            // Normal display retains the candidate but warns that ordering is not certified.
            color = mix(color, vec3<f32>(0.9, 0.38, 0.08), 0.4);
        }
    }
    textureStore(image, vec2<i32>(gid.xy), vec4<f32>(color, 1.0));
}
