//! Experimental analytic sphere, box, and capsule renderer. World state owns geometry;
//! this crate converts a bounded snapshot to `f32` GPU data and never stores a mesh.

mod gpu;
pub use gpu::{DrawOptions, GpuRenderer, GpuResult, GpuTimer};

use analytic_field::{AxisAlignedBox, Capsule, Sphere};
use spatial_math::{Transform, Vec3};
use std::fmt;
use world_state::{MAX_NODES, WorldState};

/// Hard cap used by both CPU snapshots and the WGSL loop.
pub const MAX_OBJECTS: usize = 256;
/// Supported world-coordinate magnitude for the first GPU prototype.
pub const MAX_COORDINATE: f64 = 10_000.0;
/// Supported positive primitive dimension.
pub const MIN_DIMENSION: f64 = 0.0001;

/// A rejected rendering input or GPU operation.
#[derive(Debug)]
pub enum RenderError {
    /// Input exceeds the documented finite `f32` prototype range or is inconsistent.
    InvalidInput(&'static str),
    /// A primitive ID was already present in the snapshot.
    DuplicateId,
    /// More than 256 objects were supplied.
    TooManyObjects,
    /// GPU validation or execution failed.
    Gpu(String),
}
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RenderError {}

fn finite_range(value: f64) -> Result<f32, RenderError> {
    if !value.is_finite() || value.abs() > MAX_COORDINATE {
        return Err(RenderError::InvalidInput(
            "coordinate outside supported range",
        ));
    }
    Ok(value as f32)
}
fn components(v: Vec3) -> [f64; 3] {
    [v.x(), v.y(), v.z()]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x() * b.x() + a.y() * b.y() + a.z() * b.z()
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y() * b.z() - a.z() * b.y(),
        a.z() * b.x() - a.x() * b.z(),
        a.x() * b.y() - a.y() * b.x(),
    )
    .expect("bounded camera vectors")
}

/// A finite ray segment in world units. The direction is normalized.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    /// Ray origin.
    origin: Vec3,
    /// Unit travel direction.
    direction: Vec3,
    /// Inclusive near distance.
    near: f64,
    /// Inclusive far distance.
    far: f64,
}
impl Ray {
    /// Constructs a bounded ray. `near == 0` includes a ray starting on a surface.
    pub fn new(origin: Vec3, direction: Vec3, near: f64, far: f64) -> Result<Self, RenderError> {
        for x in components(origin) {
            finite_range(x)?;
        }
        if !near.is_finite()
            || !far.is_finite()
            || near < 0.0
            || far <= near
            || far > MAX_COORDINATE
        {
            return Err(RenderError::InvalidInput("invalid ray interval"));
        }
        let direction = direction
            .normalized()
            .map_err(|_| RenderError::InvalidInput("zero ray direction"))?;
        Ok(Self {
            origin,
            direction,
            near,
            far,
        })
    }
    /// Ray origin.
    pub fn origin(self) -> Vec3 {
        self.origin
    }
    /// Unit direction.
    pub fn direction(self) -> Vec3 {
        self.direction
    }
    /// Closed interval of valid hit distances.
    pub fn interval(self) -> (f64, f64) {
        (self.near, self.far)
    }
}

/// Orthographic-free perspective camera using a right-handed world and vertical field of view.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    origin: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    tan_half_fov: f64,
    near: f64,
    far: f64,
}
impl Camera {
    /// Looks from `origin` toward `target`; `up_hint` must not be parallel to the view.
    pub fn look_at(
        origin: Vec3,
        target: Vec3,
        up_hint: Vec3,
        vertical_fov: f64,
    ) -> Result<Self, RenderError> {
        for x in components(origin)
            .into_iter()
            .chain(components(target))
            .chain(components(up_hint))
        {
            finite_range(x)?;
        }
        if !vertical_fov.is_finite() || !(0.01..=3.0).contains(&vertical_fov) {
            return Err(RenderError::InvalidInput(
                "field of view outside 0.01..=3 radians",
            ));
        }
        let forward = target
            .checked_sub(origin)
            .map_err(|_| RenderError::InvalidInput("camera overflow"))?
            .normalized()
            .map_err(|_| RenderError::InvalidInput("camera target equals origin"))?;
        let right = cross(forward, up_hint)
            .normalized()
            .map_err(|_| RenderError::InvalidInput("camera up is parallel to view"))?;
        let up = cross(right, forward);
        Ok(Self {
            origin,
            forward,
            right,
            up,
            tan_half_fov: (vertical_fov / 2.0).tan(),
            near: 0.001,
            far: 1000.0,
        })
    }
    /// Generates the ray through a pixel center. Pixel `(0,0)` is the upper left.
    pub fn ray(self, x: u32, y: u32, width: u32, height: u32) -> Result<Ray, RenderError> {
        if width == 0 || height == 0 || x >= width || y >= height {
            return Err(RenderError::InvalidInput("pixel outside nonzero viewport"));
        }
        let aspect = f64::from(width) / f64::from(height);
        let sx = ((f64::from(x) + 0.5) / f64::from(width) * 2.0 - 1.0) * aspect * self.tan_half_fov;
        let sy = (1.0 - (f64::from(y) + 0.5) / f64::from(height) * 2.0) * self.tan_half_fov;
        let right = self
            .right
            .checked_scale(sx)
            .map_err(|_| RenderError::InvalidInput("camera ray overflow"))?;
        let up = self
            .up
            .checked_scale(sy)
            .map_err(|_| RenderError::InvalidInput("camera ray overflow"))?;
        let direction = self
            .forward
            .checked_add(right)
            .and_then(|v| v.checked_add(up))
            .map_err(|_| RenderError::InvalidInput("camera ray overflow"))?;
        Ray::new(self.origin, direction, self.near, self.far)
    }
}

/// Supported primitive kind. Both represent closed solids with analytic boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveKind {
    /// Analytic sphere.
    Sphere,
    /// Analytic axis-aligned box.
    Box,
    /// Analytic closed-segment capsule.
    Capsule,
}

/// One snapshot primitive, in world coordinates, with a stable caller-supplied ID.
#[derive(Clone, Copy, Debug)]
pub struct Primitive {
    /// Identity returned by intersection queries.
    pub id: u32,
    /// Analytic kind.
    pub kind: PrimitiveKind,
    center: Vec3,
    dimensions: Vec3,
    radius: f64,
    color: [f32; 3],
}
impl Primitive {
    fn new(
        id: u32,
        kind: PrimitiveKind,
        dimensions: Vec3,
        transform: Transform,
        color: [f32; 3],
    ) -> Result<Self, RenderError> {
        for x in components(transform.translation()) {
            finite_range(x)?;
        }
        for x in components(dimensions) {
            if !x.is_finite() || !(MIN_DIMENSION..=MAX_COORDINATE).contains(&x) {
                return Err(RenderError::InvalidInput(
                    "primitive dimension outside supported range",
                ));
            }
        }
        if color
            .iter()
            .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
        {
            return Err(RenderError::InvalidInput("color component outside 0..=1"));
        }
        Ok(Self {
            id,
            kind,
            center: transform.translation(),
            dimensions,
            radius: 0.0,
            color,
        })
    }
    /// Constructs a transformed sphere. The world radius is `local_radius * uniform_scale`.
    pub fn sphere(
        id: u32,
        sphere: Sphere,
        transform: Transform,
        color: [f32; 3],
    ) -> Result<Self, RenderError> {
        let radius = sphere.radius() * transform.scale();
        let dimensions = Vec3::new(radius, radius, radius)
            .map_err(|_| RenderError::InvalidInput("radius overflow"))?;
        Self::new(id, PrimitiveKind::Sphere, dimensions, transform, color)
    }
    /// Constructs a transformed AABB. Positive uniform scale preserves axis alignment.
    pub fn axis_aligned_box(
        id: u32,
        box_field: AxisAlignedBox,
        transform: Transform,
        color: [f32; 3],
    ) -> Result<Self, RenderError> {
        let dimensions = box_field
            .half_extent()
            .checked_scale(transform.scale())
            .map_err(|_| RenderError::InvalidInput("box extent overflow"))?;
        Self::new(id, PrimitiveKind::Box, dimensions, transform, color)
    }
    /// Constructs an analytic capsule from authoritative world-space endpoints.
    pub fn capsule(id: u32, capsule: Capsule, color: [f32; 3]) -> Result<Self, RenderError> {
        let (a, b, radius) = (capsule.a(), capsule.b(), capsule.radius());
        for value in components(a).into_iter().chain(components(b)) {
            finite_range(value)?;
        }
        if !(MIN_DIMENSION..=MAX_COORDINATE).contains(&radius) {
            return Err(RenderError::InvalidInput(
                "capsule radius outside supported range",
            ));
        }
        if color
            .iter()
            .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
        {
            return Err(RenderError::InvalidInput("color component outside 0..=1"));
        }
        Ok(Self {
            id,
            kind: PrimitiveKind::Capsule,
            center: a,
            dimensions: b,
            radius,
            color,
        })
    }
    /// Center in world coordinates.
    pub fn center(self) -> Vec3 {
        self.center
    }
    /// World radius (sphere), half extents (box), or endpoint B (capsule).
    pub fn dimensions(self) -> Vec3 {
        self.dimensions
    }
    /// Capsule radius; zero for other primitive kinds.
    pub fn capsule_radius(self) -> f64 {
        self.radius
    }
    /// Linear RGB surface color.
    pub fn color(self) -> [f32; 3] {
        self.color
    }
}

/// A bounded, renderer-owned snapshot. `WorldState` remains the authority for its spheres.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    primitives: Vec<Primitive>,
}
impl Scene {
    /// Copies world geometry into a bounded snapshot. IDs encode category, entity and node.
    pub fn from_world(world: &WorldState) -> Result<Self, RenderError> {
        world
            .validate()
            .map_err(|_| RenderError::InvalidInput("invalid world state"))?;
        let mut scene = Self::default();
        for entity in world.entities() {
            let id = u32::try_from(entity.id().value())
                .map_err(|_| RenderError::InvalidInput("entity ID exceeds GPU u32 range"))?;
            scene.push(Primitive::sphere(
                id,
                entity.sphere(),
                entity.transform(),
                [0.25, 0.67, 0.95],
            )?)?;
        }
        for organism in world.organisms() {
            for node in organism.nodes() {
                let id = 1_000_000
                    + u32::try_from(organism.id().value())
                        .map_err(|_| RenderError::InvalidInput("organism ID exceeds u32"))?
                        * MAX_NODES as u32
                    + node.id();
                scene.push(Primitive::sphere(
                    id,
                    Sphere::new(organism.parameters().node_radius())
                        .map_err(|_| RenderError::InvalidInput("invalid node radius"))?,
                    Transform::new(node.position(), 1.0)
                        .map_err(|_| RenderError::InvalidInput("invalid node position"))?,
                    [0.30, 0.85, 0.42],
                )?)?;
                if let Some(parent) = node.parent() {
                    let a = organism.nodes()[parent as usize].position();
                    let capsule = Capsule::new(
                        a,
                        node.position(),
                        organism.parameters().node_radius() * 0.55,
                    )
                    .map_err(|_| RenderError::InvalidInput("invalid growth connection"))?;
                    scene.push(Primitive::capsule(
                        2_000_000
                            + u32::try_from(organism.id().value()).map_err(|_| {
                                RenderError::InvalidInput("organism ID exceeds u32")
                            })? * MAX_NODES as u32
                            + node.id(),
                        capsule,
                        [0.22, 0.65, 0.34],
                    )?)?;
                }
            }
        }
        for source in world.sources() {
            let id = 3_000_000
                + u32::try_from(source.id().value())
                    .map_err(|_| RenderError::InvalidInput("source ID exceeds u32"))?;
            scene.push(Primitive::sphere(
                id,
                Sphere::new((source.radius() * 0.06).clamp(0.05, 1.0))
                    .map_err(|_| RenderError::InvalidInput("invalid source radius"))?,
                Transform::new(source.position(), 1.0)
                    .map_err(|_| RenderError::InvalidInput("invalid source position"))?,
                if source.active() {
                    [0.98, 0.76, 0.18]
                } else {
                    [0.35, 0.35, 0.38]
                },
            )?)?;
        }
        Ok(scene)
    }
    /// Adds a primitive, rejecting duplicate identity and excess workload.
    pub fn push(&mut self, primitive: Primitive) -> Result<(), RenderError> {
        if self.primitives.len() == MAX_OBJECTS {
            return Err(RenderError::TooManyObjects);
        }
        if self.primitives.iter().any(|p| p.id == primitive.id) {
            return Err(RenderError::DuplicateId);
        }
        self.primitives.push(primitive);
        Ok(())
    }
    /// Visible primitives in stable insertion order.
    pub fn primitives(&self) -> &[Primitive] {
        &self.primitives
    }
    /// Nearest analytic boundary intersection in the closed ray interval.
    pub fn intersect(&self, ray: Ray) -> Option<Hit> {
        let mut closest: Option<Hit> = None;
        for primitive in &self.primitives {
            if let Some(hit) = intersect_primitive(*primitive, ray)
                && closest.is_none_or(|previous| hit.distance < previous.distance)
            {
                closest = Some(hit);
            }
        }
        closest
    }
}

/// Analytic hit with an outward unit normal.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Stable snapshot identity.
    pub id: u32,
    /// World-space ray distance.
    pub distance: f64,
    /// Outward unit normal.
    pub normal: Vec3,
}

fn intersect_primitive(p: Primitive, ray: Ray) -> Option<Hit> {
    let o = ray.origin.checked_sub(p.center).ok()?;
    let d = ray.direction;
    match p.kind {
        PrimitiveKind::Sphere => {
            let radius = p.dimensions.x();
            let b = dot(o, d);
            let discriminant = b * b - (dot(o, o) - radius * radius);
            if discriminant < 0.0 {
                return None;
            }
            let root = discriminant.sqrt();
            let t0 = -b - root;
            let t1 = -b + root;
            let t = if t0 >= ray.near { t0 } else { t1 };
            if !(ray.near..=ray.far).contains(&t) {
                return None;
            }
            let normal = o
                .checked_add(d.checked_scale(t).ok()?)
                .ok()?
                .normalized()
                .ok()?;
            Some(Hit {
                id: p.id,
                distance: t,
                normal,
            })
        }
        PrimitiveKind::Box => {
            let origin = components(o);
            let dir = components(d);
            let extent = components(p.dimensions);
            let mut enter = f64::NEG_INFINITY;
            let mut exit = f64::INFINITY;
            let mut enter_normal = [0.0; 3];
            let mut exit_normal = [0.0; 3];
            for axis in 0..3 {
                if dir[axis] == 0.0 {
                    if origin[axis].abs() > extent[axis] {
                        return None;
                    }
                    continue;
                }
                let a = (-extent[axis] - origin[axis]) / dir[axis];
                let b = (extent[axis] - origin[axis]) / dir[axis];
                let (low, high, low_sign) = if a <= b { (a, b, -1.0) } else { (b, a, 1.0) };
                if low > enter {
                    enter = low;
                    enter_normal = [0.0; 3];
                    enter_normal[axis] = low_sign;
                }
                if high < exit {
                    exit = high;
                    exit_normal = [0.0; 3];
                    exit_normal[axis] = -low_sign;
                }
                if enter > exit {
                    return None;
                }
            }
            let (t, n) = if enter >= ray.near {
                (enter, enter_normal)
            } else {
                (exit, exit_normal)
            };
            if !(ray.near..=ray.far).contains(&t) {
                return None;
            }
            Some(Hit {
                id: p.id,
                distance: t,
                normal: Vec3::new(n[0], n[1], n[2]).ok()?,
            })
        }
        PrimitiveKind::Capsule => intersect_capsule(p, ray),
    }
}

fn intersect_capsule(p: Primitive, ray: Ray) -> Option<Hit> {
    let a = p.center;
    let b = p.dimensions;
    let axis = b.checked_sub(a).ok()?;
    let length = axis.length().ok()?;
    let unit = if length == 0.0 {
        Vec3::new(0.0, 1.0, 0.0).ok()?
    } else {
        axis.checked_scale(1.0 / length).ok()?
    };
    let from_a = ray.origin.checked_sub(a).ok()?;
    let parallel_o = dot(from_a, unit);
    let parallel_d = dot(ray.direction, unit);
    let radial_o = from_a
        .checked_sub(unit.checked_scale(parallel_o).ok()?)
        .ok()?;
    let radial_d = ray
        .direction
        .checked_sub(unit.checked_scale(parallel_d).ok()?)
        .ok()?;
    let mut closest: Option<Hit> = None;
    let mut accept = |t: f64, normal: Vec3| {
        if (ray.near..=ray.far).contains(&t) && closest.is_none_or(|hit| t < hit.distance) {
            closest = Some(Hit {
                id: p.id,
                distance: t,
                normal,
            });
        }
    };
    if length > 0.0 {
        let qa = dot(radial_d, radial_d);
        let qb = dot(radial_o, radial_d);
        let qc = dot(radial_o, radial_o) - p.radius * p.radius;
        let disc = qb * qb - qa * qc;
        if qa > 0.0 && disc >= 0.0 {
            for t in [(-qb - disc.sqrt()) / qa, (-qb + disc.sqrt()) / qa] {
                let along = parallel_o + t * parallel_d;
                if (0.0..=length).contains(&along) {
                    let radial = radial_o.checked_add(radial_d.checked_scale(t).ok()?).ok()?;
                    if let Ok(normal) = radial.normalized() {
                        accept(t, normal);
                    }
                }
            }
        }
    }
    for (center, start) in [(a, true), (b, false)] {
        if length == 0.0 && !start {
            break;
        }
        let offset = ray.origin.checked_sub(center).ok()?;
        let qb = dot(offset, ray.direction);
        let disc = qb * qb - (dot(offset, offset) - p.radius * p.radius);
        if disc < 0.0 {
            continue;
        }
        for t in [-qb - disc.sqrt(), -qb + disc.sqrt()] {
            if !(ray.near..=ray.far).contains(&t) {
                continue;
            }
            let point = ray
                .origin
                .checked_add(ray.direction.checked_scale(t).ok()?)
                .ok()?;
            let along = dot(point.checked_sub(a).ok()?, unit);
            if (length == 0.0 || (start && along <= 0.0) || (!start && along >= length))
                && let Ok(normal) = point.checked_sub(center).ok()?.normalized()
            {
                accept(t, normal);
            }
        }
    }
    closest
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }
    fn ray(o: Vec3, d: Vec3) -> Ray {
        Ray::new(o, d, 0.0, 100.0).unwrap()
    }
    #[test]
    fn analytic_edges_and_depth() {
        let mut s = Scene::default();
        s.push(
            Primitive::sphere(
                7,
                Sphere::new(1.0).unwrap(),
                Transform::identity(),
                [1.0, 0.0, 0.0],
            )
            .unwrap(),
        )
        .unwrap();
        s.push(
            Primitive::axis_aligned_box(
                8,
                AxisAlignedBox::new(v(1.0, 1.0, 1.0)).unwrap(),
                Transform::new(v(0.0, 0.0, 4.0), 1.0).unwrap(),
                [0.0, 1.0, 0.0],
            )
            .unwrap(),
        )
        .unwrap();
        let h = s
            .intersect(ray(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0)))
            .unwrap();
        assert_eq!((h.id, h.distance, h.normal), (7, 2.0, v(0.0, 0.0, -1.0)));
        assert_eq!(
            s.intersect(ray(v(-2.0, 1.0, 0.0), v(1.0, 0.0, 0.0)))
                .unwrap()
                .distance,
            2.0
        );
        assert_eq!(
            s.intersect(ray(Vec3::ZERO, v(1.0, 0.0, 0.0)))
                .unwrap()
                .distance,
            1.0
        );
        assert!(
            s.intersect(ray(v(0.0, 3.0, -3.0), v(0.0, 0.0, 1.0)))
                .is_none()
        );
        assert_eq!(
            s.push(s.primitives()[0]).unwrap_err().to_string(),
            RenderError::DuplicateId.to_string()
        );
    }
    #[test]
    fn box_parallel_surface_and_scale() {
        let p = Primitive::axis_aligned_box(
            5,
            AxisAlignedBox::new(v(1.0, 2.0, 3.0)).unwrap(),
            Transform::new(v(3.0, 0.0, 0.0), 2.0).unwrap(),
            [0.5; 3],
        )
        .unwrap();
        let mut s = Scene::default();
        s.push(p).unwrap();
        assert_eq!(
            s.intersect(ray(v(3.0, 0.0, -10.0), v(0.0, 0.0, 1.0)))
                .unwrap()
                .distance,
            4.0
        );
        assert!(
            s.intersect(ray(v(6.0, 0.0, -10.0), v(0.0, 0.0, 1.0)))
                .is_none()
        );
        assert_eq!(
            s.intersect(ray(v(3.0, 0.0, -6.0), v(0.0, 0.0, 1.0)))
                .unwrap()
                .distance,
            0.0
        );
    }
    #[test]
    fn capsule_side_caps_inside_tangent_and_degeneracy() {
        let mut scene = Scene::default();
        scene
            .push(
                Primitive::capsule(
                    17,
                    Capsule::new(v(0.0, 0.0, 0.0), v(0.0, 2.0, 0.0), 0.5).unwrap(),
                    [0.5; 3],
                )
                .unwrap(),
            )
            .unwrap();
        let cases = [
            (ray(v(0.0, 1.0, -3.0), v(0.0, 0.0, 1.0)), Some(2.5)),
            (ray(v(0.0, 3.0, 0.0), v(0.0, -1.0, 0.0)), Some(0.5)),
            (ray(v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)), Some(0.5)),
            (ray(v(0.5, 1.0, -2.0), v(0.0, 0.0, 1.0)), Some(2.0)),
            (ray(v(0.501, 1.0, -2.0), v(0.0, 0.0, 1.0)), None),
            (ray(v(0.0, 3.0, -3.0), v(0.0, 0.0, 1.0)), None),
            (ray(v(0.51, 1.0, -3.0), v(0.0, 1e-6, 1.0)), None),
            (ray(v(0.0, 2.0, -3.0), v(0.0, 0.0, 1.0)), Some(2.5)),
        ];
        for (ray, expected) in cases {
            let actual = scene.intersect(ray).map(|hit| hit.distance);
            match (actual, expected) {
                (Some(a), Some(e)) => assert!((a - e).abs() < 1e-10, "distance {a} vs {e}"),
                (None, None) => {}
                other => panic!("classification {other:?}"),
            }
        }
        let mut degenerate = Scene::default();
        degenerate
            .push(
                Primitive::capsule(
                    18,
                    Capsule::new(Vec3::ZERO, Vec3::ZERO, 0.5).unwrap(),
                    [0.5; 3],
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            degenerate
                .intersect(ray(v(0.0, 0.0, -2.0), v(0.0, 0.0, 1.0)))
                .unwrap()
                .distance,
            1.5
        );
        let mut short = Scene::default();
        short
            .push(
                Primitive::capsule(
                    19,
                    Capsule::new(Vec3::ZERO, v(0.0, 1e-8, 0.0), 0.5).unwrap(),
                    [0.5; 3],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            (short
                .intersect(ray(v(0.0, 0.0, -2.0), v(0.0, 0.0, 1.0)))
                .unwrap()
                .distance
                - 1.5)
                .abs()
                < 1e-10
        );
    }
    #[test]
    fn world_capacity_does_not_hide_snapshot_overflow() {
        use world_state::DeterministicSeed;
        let mut world = WorldState::new(DeterministicSeed(1));
        for _ in 0..256 {
            world
                .spawn_sphere(Sphere::new(0.1).unwrap(), Transform::identity(), 0.0)
                .unwrap();
        }
        world.spawn_source(Vec3::ZERO, 1.0, 1.0).unwrap();
        world.validate().unwrap();
        assert!(matches!(
            Scene::from_world(&world),
            Err(RenderError::TooManyObjects)
        ));
    }
    #[test]
    fn camera_and_validation() {
        let c = Camera::look_at(v(0.0, 0.0, -3.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 1.0).unwrap();
        assert_eq!(c.ray(1, 1, 3, 3).unwrap().direction, v(0.0, 0.0, 1.0));
        assert!(c.ray(0, 0, 0, 3).is_err());
        assert!(Camera::look_at(Vec3::ZERO, Vec3::ZERO, v(0.0, 1.0, 0.0), 1.0).is_err());
        assert!(
            Primitive::sphere(
                0,
                Sphere::new(1.0).unwrap(),
                Transform::new(Vec3::ZERO, 1e-8).unwrap(),
                [1.0; 3]
            )
            .is_err()
        );
        assert!(
            Primitive::sphere(
                1,
                Sphere::new(1.0).unwrap(),
                Transform::identity(),
                [f32::NAN, 0.0, 0.0]
            )
            .is_err()
        );
        assert!(Ray::new(Vec3::ZERO, v(0.0, 0.0, 1.0), 0.0, f64::INFINITY).is_err());
        assert!(Ray::new(Vec3::ZERO, v(0.0, 0.0, 1.0), 1.0, 1.0).is_err());
    }

    #[test]
    fn world_snapshot_tracks_fixed_step_growth() {
        use world_state::DeterministicSeed;
        let mut world = WorldState::new(DeterministicSeed(9));
        let id = world
            .spawn_sphere(
                Sphere::new(0.5).unwrap(),
                Transform::new(v(2.0, 0.0, 0.0), 2.0).unwrap(),
                0.25,
            )
            .unwrap();
        let before = Scene::from_world(&world).unwrap();
        assert_eq!(before.primitives()[0].id, id.value() as u32);
        assert_eq!(before.primitives()[0].dimensions().x(), 1.0);
        let mut time = world_simulation::SimulationTime::default();
        world_simulation::advance(
            &mut world,
            &mut time,
            world_simulation::SimulationStep::new(2.0).unwrap(),
        )
        .unwrap();
        let after = Scene::from_world(&world).unwrap();
        assert_eq!(time.ticks(), 1);
        assert_eq!(after.primitives()[0].id, before.primitives()[0].id);
        assert_eq!(after.primitives()[0].dimensions().x(), 2.0);
        let hit = after
            .intersect(ray(v(2.0, 0.0, -5.0), v(0.0, 0.0, 1.0)))
            .unwrap();
        assert_eq!(hit.distance, 3.0);
        assert_eq!(
            world
                .entity(id)
                .unwrap()
                .signed_distance(v(2.0, 0.0, -2.0))
                .unwrap(),
            0.0
        );
    }

    #[test]
    fn life_snapshot_tracks_authoritative_nodes_and_source() {
        use world_state::{DeterministicSeed, GrowthParameters};
        let mut world = WorldState::new(DeterministicSeed(3));
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.2, 0.4, 0.1, 1.0).unwrap(),
            )
            .unwrap();
        let source = world.spawn_source(v(0.0, 1.0, 0.0), 3.0, 1.0).unwrap();
        let before = Scene::from_world(&world).unwrap();
        assert_eq!(before.primitives().len(), 2);
        assert_eq!(
            before.primitives()[0].center(),
            world.organisms()[0].nodes()[0].position()
        );
        let mut time = world_simulation::SimulationTime::default();
        world_simulation::advance_life(
            &mut world,
            &mut time,
            world_simulation::SimulationStep::new(0.2).unwrap(),
            &[],
        )
        .unwrap();
        let after = Scene::from_world(&world).unwrap();
        assert_eq!(world.organisms()[0].nodes().len(), 2);
        assert_eq!(after.primitives().len(), 4);
        assert_eq!(
            after.primitives()[1].center(),
            world.organisms()[0].nodes()[1].position()
        );
        world_simulation::advance_life(
            &mut world,
            &mut time,
            world_simulation::SimulationStep::new(0.2).unwrap(),
            &[world_simulation::EnvironmentEvent {
                tick: 2,
                order: 0,
                kind: world_simulation::EnvironmentEventKind::MoveSource {
                    id: source,
                    position: v(2.0, 1.0, 0.0),
                },
            }],
        )
        .unwrap();
        let moved = Scene::from_world(&world).unwrap();
        assert_eq!(
            moved.primitives().last().unwrap().center(),
            world.sources()[0].position()
        );
        assert_eq!(
            moved.primitives().len(),
            world.organisms()[0].nodes().len() * 2 - 1 + world.sources().len()
        );
    }

    #[test]
    fn life_snapshot_ids_stay_disjoint_after_many_ordinary_entities() {
        use world_state::{DeterministicSeed, GrowthParameters};
        let mut world = WorldState::new(DeterministicSeed(1));
        for _ in 0..16 {
            world
                .spawn_sphere(Sphere::new(0.1).unwrap(), Transform::identity(), 0.0)
                .unwrap();
        }
        world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.3, 1.0, 1.0)
                    .unwrap()
                    .with_max_children(1)
                    .unwrap(),
            )
            .unwrap();
        world.spawn_source(v(0.0, 1.0, 0.0), 3.0, 1.0).unwrap();
        for _ in 0..18 {
            let len = world.organisms()[0].nodes().len();
            let samples = (0..len)
                .map(|i| (if i + 1 == len { 1.0 } else { 0.0 }, v(0.0, 1.0, 0.0)))
                .collect::<Vec<_>>();
            world.organisms_mut()[0].grow(&samples).unwrap();
        }
        assert_eq!(Scene::from_world(&world).unwrap().primitives().len(), 54);
    }

    #[test]
    fn camera_edges_scales_and_overlap() {
        let camera = Camera::look_at(v(0.0, 0.0, -3.0), Vec3::ZERO, v(0.0, 1.0, 0.0), 1.0).unwrap();
        assert!(camera.ray(0, 1, 3, 3).unwrap().direction.x() > 0.0);
        assert!(camera.ray(2, 1, 3, 3).unwrap().direction.x() < 0.0);
        let moved =
            Camera::look_at(v(3.0, 0.0, -3.0), v(3.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 1.0).unwrap();
        assert_eq!(moved.ray(1, 1, 3, 3).unwrap().origin(), v(3.0, 0.0, -3.0));
        let mut small = Scene::default();
        small
            .push(
                Primitive::sphere(
                    1,
                    Sphere::new(0.001).unwrap(),
                    Transform::identity(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            (small
                .intersect(ray(v(0.0, 0.0, -0.01), v(0.0, 0.0, 1.0)))
                .unwrap()
                .distance
                - 0.009)
                .abs()
                < 1e-12
        );
        let mut large = Scene::default();
        large
            .push(
                Primitive::sphere(
                    2,
                    Sphere::new(1000.0).unwrap(),
                    Transform::identity(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            large
                .intersect(Ray::new(v(0.0, 0.0, -3000.0), v(0.0, 0.0, 1.0), 0.0, 5000.0).unwrap())
                .unwrap()
                .distance,
            2000.0
        );
        let mut overlap = Scene::default();
        overlap
            .push(
                Primitive::sphere(
                    3,
                    Sphere::new(1.0).unwrap(),
                    Transform::identity(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        overlap
            .push(
                Primitive::sphere(
                    4,
                    Sphere::new(1.0).unwrap(),
                    Transform::new(v(0.0, 0.0, 2.0), 1.0).unwrap(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            overlap
                .intersect(ray(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0)))
                .unwrap()
                .id,
            3
        );
        assert_eq!(
            overlap
                .intersect(Ray::new(v(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0), 0.001, 100.0).unwrap())
                .unwrap()
                .id,
            4
        );
        let near = Ray::new(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0), 0.0, 1.5).unwrap();
        assert!(overlap.intersect(near).is_none());
        let coincident = Primitive::sphere(
            5,
            Sphere::new(1.0).unwrap(),
            Transform::identity(),
            [1.0; 3],
        )
        .unwrap();
        overlap.push(coincident).unwrap();
        assert_eq!(
            overlap
                .intersect(ray(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0)))
                .unwrap()
                .id,
            3
        );
    }
}
