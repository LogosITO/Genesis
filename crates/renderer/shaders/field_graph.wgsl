// Direct interpretation of validated graph nodes; no primitive-list expansion.
struct Config {
    origin: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    view: vec4<f32>,   // width, height, near, far
    trace: vec4<f32>,  // tolerance, safety, normal delta
    info: vec4<u32>,   // root, node count, iterations
}
struct Node {
    links: vec4<u32>, // opcode, child/left, right
    param: vec4<f32>,
}
struct Ray { origin_near: vec4<f32>, direction_far: vec4<f32> }
struct Outcome { distance_normal: vec4<f32>, info: vec4<u32> }
struct Frame { point: vec3<f32>, first: f32, index: u32, phase: u32 }

@group(0) @binding(0) var<uniform> config: Config;
@group(0) @binding(1) var<storage, read> nodes: array<Node>;
@group(0) @binding(2) var image: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<storage, read> rays: array<Ray>;
@group(0) @binding(4) var<storage, read_write> outcomes: array<Outcome>;

fn finite(v: f32) -> bool { return abs(v) < 1e20; }

fn sample(point: vec3<f32>) -> f32 {
    var stack: array<Frame, 16>;
    var depth = 1u;
    var visits = 0u;
    var last = 1e30;
    stack[0] = Frame(point, 0.0, config.info.x, 0u);
    for (var work = 0u; work < 256u; work++) {
        if (depth == 0u) { return last; }
        let slot = depth - 1u;
        let frame = stack[slot];
        if (frame.index >= config.info.y || frame.index >= arrayLength(&nodes)) { return 1e30; }
        let node = nodes[frame.index];
        if (frame.phase == 0u) {
            visits += 1u;
            if (visits > 64u) { return 1e30; }
            switch node.links.x {
                case 0u: {
                    last = length(frame.point) - node.param.x;
                    depth -= 1u;
                }
                case 1u: {
                    let q = abs(frame.point) - node.param.xyz;
                    last = length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
                    depth -= 1u;
                }
                case 2u, 3u, 4u: {
                    if (depth >= 16u || node.links.y >= config.info.y) { return 1e30; }
                    stack[slot].phase = 1u;
                    var child_point = frame.point;
                    if (node.links.x == 2u) { child_point -= node.param.xyz; }
                    if (node.links.x == 3u) { child_point /= node.param.x; }
                    stack[depth] = Frame(child_point, 0.0, node.links.y, 0u);
                    depth += 1u;
                }
                default: { return 1e30; }
            }
        } else if (frame.phase == 1u) {
            if (!finite(last)) { return 1e30; }
            if (node.links.x == 4u) {
                if (depth >= 16u || node.links.z >= config.info.y) { return 1e30; }
                stack[slot].first = last;
                stack[slot].phase = 2u;
                stack[depth] = Frame(frame.point, 0.0, node.links.z, 0u);
                depth += 1u;
            } else {
                if (node.links.x == 3u) { last *= node.param.x; }
                depth -= 1u;
            }
        } else {
            if (!finite(frame.first) || !finite(last)) { return 1e30; }
            last = min(frame.first, last);
            depth -= 1u;
        }
    }
    return 1e30;
}

fn trace(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Outcome {
    var distance = near;
    for (var step = 0u; step < config.info.z; step++) {
        let value = sample(origin + direction * distance);
        if (!finite(value)) { return Outcome(vec4<f32>(0.0), vec4<u32>(2u, step, 1u, 0u)); }
        if (value < 0.0) { return Outcome(vec4<f32>(0.0), vec4<u32>(2u, step, 2u, 0u)); }
        if (value <= config.trace.x) {
            let p = origin + direction * distance;
            let e = config.trace.z;
            // shortcut: finite differences are display-only; add analytic derivatives before contact-grade normals.
            let gradient = vec3<f32>(
                sample(p + vec3<f32>(e,0.0,0.0)) - sample(p - vec3<f32>(e,0.0,0.0)),
                sample(p + vec3<f32>(0.0,e,0.0)) - sample(p - vec3<f32>(0.0,e,0.0)),
                sample(p + vec3<f32>(0.0,0.0,e)) - sample(p - vec3<f32>(0.0,0.0,e)));
            if (!finite(length(gradient)) || length(gradient) < 1e-10) {
                return Outcome(vec4<f32>(0.0), vec4<u32>(2u, step, 3u, 0u));
            }
            return Outcome(vec4<f32>(normalize(gradient), distance), vec4<u32>(1u, step, 0u, 0u));
        }
        let next = distance + value * config.trace.y;
        if (!finite(next) || next <= distance) { return Outcome(vec4<f32>(0.0), vec4<u32>(2u, step, 4u, 0u)); }
        if (next > far) { return Outcome(vec4<f32>(0.0), vec4<u32>(0u, step, 0u, 0u)); }
        distance = next;
    }
    return Outcome(vec4<f32>(0.0), vec4<u32>(3u, config.info.z, 0u, 0u));
}

@compute @workgroup_size(8, 8)
fn render(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u32(config.view.x) || gid.y >= u32(config.view.y)) { return; }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / config.view.xy;
    let direction = normalize(config.forward.xyz + (uv.x * 2.0 - 1.0) * config.right.xyz + (1.0 - uv.y * 2.0) * config.up.xyz);
    let result = trace(config.origin.xyz, direction, config.view.z, config.view.w);
    var color = vec3<f32>(0.035, 0.055, 0.09);
    if (result.info.x == 1u) {
        let light = normalize(vec3<f32>(0.5, 0.8, -0.6));
        color = vec3<f32>(0.1, 0.65, 0.8) * (0.2 + 0.8 * max(dot(result.distance_normal.xyz, light), 0.0));
    }
    if (result.info.x == 2u) { color = vec3<f32>(0.9, 0.1, 0.1); }
    if (result.info.x == 3u) { color = vec3<f32>(0.9, 0.75, 0.05); }
    textureStore(image, vec2<i32>(gid.xy), vec4<f32>(color, 1.0));
}

@compute @workgroup_size(64)
fn query(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= arrayLength(&rays)) { return; }
    let ray = rays[gid.x];
    outcomes[gid.x] = trace(ray.origin_near.xyz, normalize(ray.direction_far.xyz), ray.origin_near.w, ray.direction_far.w);
}
