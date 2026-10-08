struct Camera {
    origin: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    view: vec4<f32>, // width, height, near, far
    settings: vec4<f32>, // x = 0 lit, 1 normals; y = object count
}
struct Primitive {
    center_kind: vec4<f32>, // w = 0 sphere, 1 AABB
    dimensions: vec4<f32>, // radius in x, or box half extents
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
    valid: bool,
}
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> objects: array<Primitive>;
@group(0) @binding(2) var image: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<storage, read> query_rays: array<Ray>;
@group(0) @binding(4) var<storage, read_write> query_hits: array<GpuHit>;

fn no_hit() -> Hit {
    return Hit(0.0, vec3<f32>(0.0), 0xffffffffu, false);
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
    return Hit(t, normalize(relative + t * direction), p.identity.x, true);
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
    return Hit(t, select(exit_normal, entry_normal, use_entry), p.identity.x, true);
}

fn trace(origin: vec3<f32>, direction: vec3<f32>, near: f32, far: f32) -> Hit {
    var closest = no_hit();
    var limit = far;
    for (var i = 0u; i < min(u32(camera.settings.y), 256u); i++) {
        let p = objects[i];
        var candidate = no_hit();
        if (p.center_kind.w == 0.0) {
            candidate = sphere_hit(p, origin, direction, near, limit);
        } else {
            candidate = box_hit(p, origin, direction, near, limit);
        }
        if (candidate.valid && (!closest.valid || candidate.distance < closest.distance)) {
            closest = candidate;
            limit = candidate.distance;
        }
    }
    return closest;
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
            var base = vec3<f32>(1.0);
            for (var i = 0u; i < min(u32(camera.settings.y), 256u); i++) {
                if (objects[i].identity.x == hit.id) { base = objects[i].color.xyz; break; }
            }
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
