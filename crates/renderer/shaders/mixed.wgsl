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
struct FieldInstance {
    position_scale: vec4<f32>,
    info: vec4<u32>,
    bounds_min: vec4<f32>,
    bounds_max: vec4<f32>,
}
struct MixedOptions {
    trace: vec4<f32>,
    info: vec4<u32>,
    background: vec4<f32>,
    light: vec4<f32>,
}
struct MaterialGpu {
    base: vec4<f32>,
    alternate: vec4<f32>,
    emission: vec4<f32>,
    axis_speed: vec4<f32>,
    params: vec4<f32>,
}
@group(1) @binding(0) var<storage, read> field_nodes: array<Node>;
@group(1) @binding(1) var<storage, read> field_instances: array<FieldInstance>;
@group(1) @binding(2) var<uniform> mixed_options: MixedOptions;
@group(1) @binding(3) var<storage, read> materials: array<MaterialGpu>;
@group(1) @binding(4) var<storage, read> analytic_material_indices: array<u32>;
var<private> field_config: FieldConfig;

// FIELD_EVALUATOR_INSERTION_POINT

fn misses_field_bounds(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32,
    lower: vec3<f32>, upper: vec3<f32>) -> bool {
    var lo = near;
    var hi = far;
    for (var axis = 0u; axis < 3u; axis++) {
        if (direction[axis] == 0.0) {
            if (origin[axis] < lower[axis] || origin[axis] > upper[axis]) { return true; }
            continue;
        }
        let a = (lower[axis] - origin[axis]) / direction[axis];
        let b = (upper[axis] - origin[axis]) / direction[axis];
        if (!finite(a) || !finite(b)) { return false; }
        lo = max(lo, min(a, b));
        hi = min(hi, max(a, b));
        if (lo > hi) { return true; }
    }
    return false;
}

fn shade_material(index: u32, point: vec3<f32>, normal: vec3<f32>, fallback: vec3<f32>, emphasis: f32) -> vec3<f32> {
    let light = normalize(mixed_options.light.xyz);
    let diffuse = 0.18 + 0.82 * max(dot(normal, light), 0.0);
    if (index == 0xffffffffu || index >= arrayLength(&materials)) {
        return fallback * mix(diffuse, 1.0, emphasis);
    }
    let material = materials[index];
    var base = material.base.xyz;
    let mode = material.params.z;
    if (mode > 0.5) {
        let coordinate = dot(point, material.axis_speed.xyz) * material.params.x
            + material.params.y + mixed_options.trace.w * material.axis_speed.w;
        if (mode < 1.5) {
            base = mix(base, material.alternate.xyz, clamp(coordinate, 0.0, 1.0));
        } else {
            base = select(base, material.alternate.xyz, fract(coordinate) >= 0.5);
        }
    }
    return clamp(base * diffuse + material.emission.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
}

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
    var field_material = 0xffffffffu;
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
            if (misses_field_bounds(origin, direction, camera.view.z / scale,
                camera.view.w / scale, instance.bounds_min.xyz, instance.bounds_max.xyz)) { continue; }
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
                        field_material = instance.info.w;
                        field_hit = true;
                    }
                }
            }
        }
    }
    var color = mixed_options.background.xyz;
    if (field_hit && analytic.valid) {
        let band = 1e-4 * max(1.0, max(abs(field_distance), abs(analytic.distance)));
        if (abs(field_distance - analytic.distance) <= band) { ambiguous = true; }
    }
    let use_field = field_hit && (!analytic.valid || field_distance < analytic.distance);
    if (analytic.valid && !use_field) {
        let primitive = objects[analytic.primitive_index];
        color = shade_material(analytic_material_indices[analytic.primitive_index],
            camera.origin.xyz + direction * analytic.distance, analytic.normal,
            primitive.color.xyz, primitive.color.w);
        if (camera.settings.x > 0.5) { color = analytic.normal * 0.5 + vec3<f32>(0.5); }
    } else if (use_field) {
        color = shade_material(field_material, camera.origin.xyz + direction * field_distance,
            field_normal, vec3<f32>(0.1, 0.65, 0.8), 0.0);
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
