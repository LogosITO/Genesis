struct Camera {
    origin: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    view: vec4<f32>, // width, height, near, far
    settings: vec4<f32>, // x = 0 lit, 1 normals
    scene: vec4<u32>, // primitive count, BVH node count, unused, BVH enabled
}
struct Primitive {
    center_kind: vec4<f32>, // w = 0 sphere, 1 AABB, 2 capsule; xyz = capsule A
    dimensions: vec4<f32>, // radius in x, box half extents, or capsule B xyz and radius w
    color: vec4<f32>,
    identity: vec4<u32>,
}
struct Ray {
    origin_near: vec4<f32>,
    direction_far: vec4<f32>,
}
struct GpuHit {
    distance_normal: vec4<f32>,
    identity: vec4<u32>, // valid, id, unused, unused
}
struct Hit {
    distance: f32,
    normal: vec3<f32>,
    id: u32,
    primitive_index: u32,
    valid: bool,
}
struct BvhNode {
    lower: vec4<f32>,
    upper: vec4<f32>,
    info: vec4<u32>, // subtree exit, leaf count, unused, unused
    leaf: vec4<u32>, // primitive indices for leaves of at most four
}
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> objects: array<Primitive>;
@group(0) @binding(2) var image: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<storage, read> query_rays: array<Ray>;
@group(0) @binding(4) var<storage, read_write> query_hits: array<GpuHit>;
@group(0) @binding(5) var<storage, read> nodes: array<BvhNode>;

fn no_hit() -> Hit {
    return Hit(0.0, vec3<f32>(0.0), 0xffffffffu, 0u, false);
}

fn sphere_hit(p: Primitive, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    let relative = origin - p.center_kind.xyz;
    let b = dot(relative, direction);
    let c = dot(relative, relative) - p.dimensions.x * p.dimensions.x;
    let disc = b * b - c;
    if (disc < 0.0) { return no_hit(); }
    let root = sqrt(disc);
    let entry = -b - root;
    let exit = -b + root;
    let t = select(exit, entry, entry >= near);
    if (t < near || t > far) { return no_hit(); }
    return Hit(t, normalize(relative + t * direction), p.identity.x, 0u, true);
}

fn box_hit(p: Primitive, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    let o = origin - p.center_kind.xyz;
    let e = p.dimensions.xyz;
    var entry = -3.402823e38;
    var exit = 3.402823e38;
    var entry_normal = vec3<f32>(0.0);
    var exit_normal = vec3<f32>(0.0);
    for (var axis = 0u; axis < 3u; axis++) {
        if (direction[axis] == 0.0) {
            if (abs(o[axis]) > e[axis]) { return no_hit(); }
            continue;
        }
        let a = (-e[axis] - o[axis]) / direction[axis];
        let b = ( e[axis] - o[axis]) / direction[axis];
        let low = min(a, b);
        let high = max(a, b);
        let sign = select(1.0, -1.0, a <= b);
        if (low > entry) {
            entry = low;
            entry_normal = vec3<f32>(0.0);
            entry_normal[axis] = sign;
        }
        if (high < exit) {
            exit = high;
            exit_normal = vec3<f32>(0.0);
            exit_normal[axis] = -sign;
        }
        if (entry > exit) { return no_hit(); }
    }
    let use_entry = entry >= near;
    let t = select(exit, entry, use_entry);
    if (t < near || t > far) { return no_hit(); }
    return Hit(t, select(exit_normal, entry_normal, use_entry), p.identity.x, 0u, true);
}

fn capsule_hit(p: Primitive, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    let a = p.center_kind.xyz;
    let b = p.dimensions.xyz;
    let radius = p.dimensions.w;
    let axis = b - a;
    let length = length(axis);
    let unit = select(vec3<f32>(0.0, 1.0, 0.0), axis / max(length, 1e-20), length > 0.0);
    let from_a = origin - a;
    let parallel_o = dot(from_a, unit);
    let parallel_d = dot(direction, unit);
    let radial_o = from_a - parallel_o * unit;
    let radial_d = direction - parallel_d * unit;
    var best = no_hit();
    if (length > 0.0) {
        let qa = dot(radial_d, radial_d);
        let qb = dot(radial_o, radial_d);
        let qc = dot(radial_o, radial_o) - radius * radius;
        let disc = qb * qb - qa * qc;
        if (qa > 0.0 && disc >= 0.0) {
            let root = sqrt(disc);
            for (var i = 0u; i < 2u; i++) {
                let t = select((-qb + root) / qa, (-qb - root) / qa, i == 0u);
                let along = parallel_o + t * parallel_d;
                if (t >= near && t <= far && along >= 0.0 && along <= length && (!best.valid || t < best.distance)) {
                    best = Hit(t, normalize(radial_o + t * radial_d), p.identity.x, 0u, true);
                }
            }
        }
    }
    for (var cap = 0u; cap < 2u; cap++) {
        if (length == 0.0 && cap == 1u) { break; }
        let center = select(b, a, cap == 0u);
        let offset = origin - center;
        let qb = dot(offset, direction);
        let disc = qb * qb - (dot(offset, offset) - radius * radius);
        if (disc < 0.0) { continue; }
        let root = sqrt(disc);
        for (var i = 0u; i < 2u; i++) {
            let t = select(-qb + root, -qb - root, i == 0u);
            let point = origin + t * direction;
            let along = dot(point - a, unit);
            if (t >= near && t <= far && (length == 0.0 || (cap == 0u && along <= 0.0) || (cap == 1u && along >= length)) && (!best.valid || t < best.distance)) {
                best = Hit(t, normalize(point - center), p.identity.x, 0u, true);
            }
        }
    }
    return best;
}

fn intersect_one(p: Primitive, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    if (p.center_kind.w == 0.0) { return sphere_hit(p, origin, direction, near, far); }
    if (p.center_kind.w == 1.0) { return box_hit(p, origin, direction, near, far); }
    return capsule_hit(p, origin, direction, near, far);
}

fn trace_direct(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    var closest = no_hit();
    var limit = far;
    for (var i = 0u; i < min(camera.scene.x, arrayLength(&objects)); i++) {
        var candidate = intersect_one(objects[i], origin, direction, near, limit);
        if (candidate.valid && (!closest.valid || candidate.distance < closest.distance)) {
            candidate.primitive_index = i;
            closest = candidate;
            limit = candidate.distance;
        }
    }
    return closest;
}

fn node_intersects(node: BvhNode, origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> bool {
    var entry = near;
    var exit = far;
    for (var axis = 0u; axis < 3u; axis++) {
        if (direction[axis] == 0.0) {
            if (origin[axis] < node.lower[axis] || origin[axis] > node.upper[axis]) { return false; }
            continue;
        }
        let a = (node.lower[axis] - origin[axis]) / direction[axis];
        let b = (node.upper[axis] - origin[axis]) / direction[axis];
        if (!(abs(a) <= 3.402823e38 && abs(b) <= 3.402823e38)) { return true; }
        let slack = 8e-6 * max(max(abs(a), abs(b)), 1.0);
        entry = max(entry, min(a, b) - slack);
        exit = min(exit, max(a, b) + slack);
        if (entry > exit) { return false; }
    }
    return true;
}

fn trace_bvh(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    let node_count = camera.scene.y;
    if (node_count == 0u || node_count > arrayLength(&nodes)) {
        return trace_direct(origin, direction, near, far);
    }
    var closest = no_hit();
    var cursor = 0u;
    for (var visited = 0u; visited < node_count; visited++) {
        if (cursor == node_count) { return closest; }
        if (cursor > node_count) { return trace_direct(origin, direction, near, far); }
        let node = nodes[cursor];
        let subtree_exit = node.info.x;
        let leaf_count = node.info.y;
        if (subtree_exit <= cursor || subtree_exit > node_count || leaf_count > 4u) {
            return trace_direct(origin, direction, near, far);
        }
        if (!node_intersects(node, origin, direction, near, far)) {
            cursor = subtree_exit;
            continue;
        }
        if (leaf_count == 0u) {
            if (cursor + 1u >= subtree_exit) { return trace_direct(origin, direction, near, far); }
            cursor += 1u;
            continue;
        }
        for (var slot = 0u; slot < leaf_count; slot++) {
            let primitive_index = node.leaf[slot];
            if (primitive_index >= camera.scene.x || primitive_index >= arrayLength(&objects)) {
                return trace_direct(origin, direction, near, far);
            }
            var candidate = intersect_one(objects[primitive_index], origin, direction, near, far);
            if (candidate.valid && (!closest.valid || candidate.distance < closest.distance ||
                (candidate.distance == closest.distance && primitive_index < closest.primitive_index))) {
                candidate.primitive_index = primitive_index;
                closest = candidate;
            }
        }
        cursor = subtree_exit;
    }
    if (cursor != node_count) { return trace_direct(origin, direction, near, far); }
    return closest;
}

fn trace(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    if (camera.scene.w == 1u) { return trace_bvh(origin, direction, near, far); }
    return trace_direct(origin, direction, near, far);
}

@compute @workgroup_size(8, 8)
fn render(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u32(camera.view.x) || gid.y >= u32(camera.view.y)) { return; }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / camera.view.xy;
    let sx = uv.x * 2.0 - 1.0;
    let sy = 1.0 - uv.y * 2.0;
    let direction = normalize(camera.forward.xyz + sx * camera.right.xyz + sy * camera.up.xyz);
    let hit = trace(camera.origin.xyz, direction, camera.view.z, camera.view.w);
    var color = vec3<f32>(0.035, 0.055, 0.09) + vec3<f32>(0.03, 0.04, 0.05) * (1.0 - uv.y);
    if (hit.valid) {
        if (camera.settings.x > 0.5) {
            color = hit.normal * 0.5 + vec3<f32>(0.5);
        } else {
            let base = objects[hit.primitive_index].color.xyz;
            let light = normalize(vec3<f32>(0.5, 0.8, -0.6));
            color = base * (0.18 + 0.82 * max(dot(hit.normal, light), 0.0));
        }
    }
    textureStore(image, vec2<i32>(gid.xy), vec4<f32>(color, 1.0));
}

@compute @workgroup_size(64)
fn query(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= arrayLength(&query_rays)) { return; }
    let ray = query_rays[gid.x];
    let hit = trace(ray.origin_near.xyz, ray.direction_far.xyz, ray.origin_near.w, ray.direction_far.w);
    query_hits[gid.x].distance_normal = vec4<f32>(hit.normal, hit.distance);
    query_hits[gid.x].identity = vec4<u32>(u32(hit.valid), hit.id, 0u, 0u);
}
