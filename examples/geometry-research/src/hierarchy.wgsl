// Research-only compact hierarchy interpreter. All geometry is analytic.
struct Definition { a_kind: vec4<f32>, b_radius: vec4<f32>, color: vec4<f32> }
struct Node { info: vec4<u32>, lower: vec4<f32>, upper: vec4<f32> }
struct Edge { child: vec4<u32>, at_scale: vec4<f32>, angle: vec4<f32> }
struct Parameters {
    origin: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    view: vec4<f32>, // width, height, near, far
    counts: vec4<u32>, // nodes, edges, definitions, root
    scale: vec4<f32>,
}
struct Pose { at_scale: vec4<f32>, angle: f32 }
struct Frame { node: u32, next: u32, pose: Pose }
struct Hit { t: f32, normal: vec3<f32>, id: u32, definition: u32, valid: bool }
struct Trace { hit: Hit, visits: u32, leaves: u32, peak: u32, culled: u32, exhausted: bool }
struct Diagnostic { normal_t: vec4<f32>, identity: vec4<u32>, work: vec4<u32> }

@group(0) @binding(0) var<uniform> params: Parameters;
@group(0) @binding(1) var<storage, read> definitions: array<Definition>;
@group(0) @binding(2) var<storage, read> nodes: array<Node>;
@group(0) @binding(3) var<storage, read> edges: array<Edge>;
@group(0) @binding(4) var<storage, read_write> diagnostics: array<Diagnostic>;
@group(0) @binding(5) var image: texture_storage_2d<rgba8unorm, write>;

fn no_hit() -> Hit { return Hit(0.0, vec3<f32>(0.0), 0xffffffffu, 0u, false); }
fn rotate(v: vec3<f32>, angle: f32) -> vec3<f32> {
    let s = sin(angle);
    let c = cos(angle);
    return vec3<f32>(c*v.x - s*v.y, s*v.x + c*v.y, v.z);
}
fn compose(parent: Pose, edge: Edge) -> Pose {
    return Pose(vec4<f32>(parent.at_scale.xyz + parent.at_scale.w * rotate(edge.at_scale.xyz, parent.angle), parent.at_scale.w * edge.at_scale.w), parent.angle + edge.angle.x);
}
fn point(pose: Pose, local: vec3<f32>) -> vec3<f32> {
    return pose.at_scale.xyz + pose.at_scale.w * rotate(local, pose.angle);
}
fn sphere_hit(center: vec3<f32>, radius: f32, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32, id: u32, definition: u32) -> Hit {
    let relative = origin - center;
    let b = dot(relative, direction);
    let c = dot(relative, relative) - radius * radius;
    let disc = b*b - c;
    if (disc < 0.0) { return no_hit(); }
    let root = sqrt(disc);
    let entry = -b - root;
    let exit = -b + root;
    let t = select(exit, entry, entry >= near);
    if (t < near || t > far) { return no_hit(); }
    return Hit(t, normalize(relative + t * direction), id, definition, true);
}
fn capsule_hit(a: vec3<f32>, b: vec3<f32>, radius: f32, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32, id: u32, definition: u32) -> Hit {
    let axis = b - a;
    let length_axis = length(axis);
    let unit = select(vec3<f32>(0.0, 1.0, 0.0), axis / max(length_axis, 1e-20), length_axis > 0.0);
    let from_a = origin - a;
    let parallel_o = dot(from_a, unit);
    let parallel_d = dot(direction, unit);
    let radial_o = from_a - parallel_o * unit;
    let radial_d = direction - parallel_d * unit;
    var best = no_hit();
    if (length_axis > 0.0) {
        let qa = dot(radial_d, radial_d);
        let qb = dot(radial_o, radial_d);
        let qc = dot(radial_o, radial_o) - radius * radius;
        let disc = qb*qb - qa*qc;
        if (qa > 0.0 && disc >= 0.0) {
            let root = sqrt(disc);
            for (var i = 0u; i < 2u; i++) {
                let t = select((-qb + root)/qa, (-qb - root)/qa, i == 0u);
                let along = parallel_o + t*parallel_d;
                if (t >= near && t <= far && along >= 0.0 && along <= length_axis && (!best.valid || t < best.t)) {
                    best = Hit(t, normalize(radial_o + t*radial_d), id, definition, true);
                }
            }
        }
    }
    for (var cap = 0u; cap < 2u; cap++) {
        if (length_axis == 0.0 && cap == 1u) { break; }
        let center = select(b, a, cap == 0u);
        let offset = origin - center;
        let qb = dot(offset, direction);
        let disc = qb*qb - (dot(offset, offset) - radius*radius);
        if (disc < 0.0) { continue; }
        let root = sqrt(disc);
        for (var i = 0u; i < 2u; i++) {
            let t = select(-qb + root, -qb - root, i == 0u);
            let along = dot(origin + t*direction - a, unit);
            if (t >= near && t <= far && (length_axis == 0.0 || (cap == 0u && along <= 0.0) || (cap == 1u && along >= length_axis)) && (!best.valid || t < best.t)) {
                best = Hit(t, normalize(origin + t*direction - center), id, definition, true);
            }
        }
    }
    return best;
}
fn leaf_hit(definition: Definition, pose: Pose, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32, id: u32, index: u32) -> Hit {
    let radius = definition.b_radius.w * pose.at_scale.w;
    let a = point(pose, definition.a_kind.xyz);
    if (definition.a_kind.w == 0.0) {
        return sphere_hit(a, radius, origin, direction, near, far, id, index);
    }
    let b = point(pose, definition.b_radius.xyz);
    return capsule_hit(a, b, radius, origin, direction, near, far, id, index);
}

// Visual f32 guard only; this is not a certified numerical exclusion.
fn outside(node: Node, pose: Pose, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> bool {
    if (node.lower.x > node.upper.x) { return true; }
    if (pose.at_scale.w < 1e-8) { return false; }
    let local_origin = rotate((origin - pose.at_scale.xyz) / pose.at_scale.w, -pose.angle);
    let local_direction = rotate(direction / pose.at_scale.w, -pose.angle);
    var entry = near;
    var exit = far;
    for (var axis = 0u; axis < 3u; axis++) {
        if (local_direction[axis] == 0.0) {
            if (local_origin[axis] < node.lower[axis] || local_origin[axis] > node.upper[axis]) { return true; }
            continue;
        }
        let a = (node.lower[axis] - local_origin[axis]) / local_direction[axis];
        let b = (node.upper[axis] - local_origin[axis]) / local_direction[axis];
        if (!(abs(a) <= 3.402823e38 && abs(b) <= 3.402823e38)) { return false; }
        let slack = 8e-6 * max(max(abs(a), abs(b)), 1.0);
        entry = max(entry, min(a, b) - slack);
        exit = min(exit, max(a, b) + slack);
        if (entry > exit) { return true; }
    }
    return false;
}

fn trace(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Trace {
    var result = Trace(no_hit(), 0u, 0u, 0u, 0u, false);
    if (params.counts.x == 0u || params.counts.x > arrayLength(&nodes) || params.counts.y > arrayLength(&edges) || params.counts.z > arrayLength(&definitions) || params.counts.w >= params.counts.x) {
        result.exhausted = true;
        return result;
    }
    var stack: array<Frame, 12>;
    stack[0] = Frame(params.counts.w, 0u, Pose(vec4<f32>(0.0, 0.0, 0.0, params.scale.x), 0.0));
    var depth = 1u;
    var ordinal = 0u;
    result.visits = 1u;
    result.peak = 1u;
    for (var work = 0u; work < 4096u; work++) {
        if (depth == 0u) { return result; }
        let frame = stack[depth - 1u];
        if (frame.node >= params.counts.x) { result.exhausted = true; return result; }
        let record = nodes[frame.node];
        let node = record.info;
        if (params.scale.y > 0.5 && frame.next == 0u && outside(record, frame.pose, origin, direction, near, select(far, result.hit.t, result.hit.valid))) {
            let skipped = select(node.w, 1u, node.x == 0u);
            if (skipped > 512u - ordinal) { result.exhausted = true; return result; }
            ordinal += skipped;
            result.culled += 1u;
            depth -= 1u;
            continue;
        }
        if (node.x == 0u) {
            if (node.w >= params.counts.z || result.leaves >= 512u) { result.exhausted = true; return result; }
            let candidate = leaf_hit(definitions[node.w], frame.pose, origin, direction, near, far, ordinal, node.w);
            if (candidate.valid && (!result.hit.valid || candidate.t < result.hit.t)) { result.hit = candidate; }
            result.leaves += 1u;
            ordinal += 1u;
            depth -= 1u;
            continue;
        }
        if (node.x > 2u || node.y > params.counts.y || node.z > 32u) {
            result.exhausted = true;
            return result;
        }
        if ((node.x == 1u && node.z > params.counts.y - node.y) || (node.x == 2u && node.y >= params.counts.y)) {
            result.exhausted = true;
            return result;
        }
        if (frame.next >= node.z) { depth -= 1u; continue; }
        if (depth >= 12u || result.visits >= 2048u) { result.exhausted = true; return result; }
        var parent = frame;
        var child_pose = parent.pose;
        var edge: Edge;
        if (node.x == 1u) {
            edge = edges[node.y + parent.next];
            child_pose = compose(parent.pose, edge);
        } else {
            edge = edges[node.y];
            if (parent.next > 0u) { child_pose = compose(parent.pose, edge); }
            parent.pose = child_pose;
        }
        parent.next += 1u;
        stack[depth - 1u] = parent;
        if (edge.child.x >= params.counts.x) { result.exhausted = true; return result; }
        stack[depth] = Frame(edge.child.x, 0u, child_pose);
        depth += 1u;
        result.visits += 1u;
        result.peak = max(result.peak, depth);
    }
    result.exhausted = true;
    return result;
}

@compute @workgroup_size(8, 8)
fn render(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u32(params.view.x) || gid.y >= u32(params.view.y)) { return; }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / params.view.xy;
    let sx = uv.x * 2.0 - 1.0;
    let sy = 1.0 - uv.y * 2.0;
    let direction = normalize(params.forward.xyz + sx*params.right.xyz + sy*params.up.xyz);
    let traced = trace(params.origin.xyz, direction, params.view.z, params.view.w);
    var color = vec3<f32>(0.035, 0.055, 0.09) + vec3<f32>(0.03, 0.04, 0.05) * (1.0 - uv.y);
    if (traced.exhausted) {
        color = vec3<f32>(1.0, 0.0, 1.0);
    } else if (traced.hit.valid) {
        let base = definitions[traced.hit.definition].color.xyz;
        let light = normalize(vec3<f32>(0.5, 0.8, -0.6));
        let diffuse = 0.18 + 0.82 * max(dot(traced.hit.normal, light), 0.0);
        color = base * diffuse;
    }
    let index = gid.y * u32(params.view.x) + gid.x;
    diagnostics[index] = Diagnostic(vec4<f32>(traced.hit.normal, traced.hit.t), vec4<u32>(u32(traced.hit.valid), traced.hit.id, traced.hit.definition, u32(traced.exhausted)), vec4<u32>(traced.visits, traced.leaves, traced.peak, traced.culled));
    textureStore(image, vec2<i32>(gid.xy), vec4<f32>(color, 1.0));
}
