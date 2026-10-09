//! Experimental CPU BVH over renderer snapshots. Bounds are acceleration data, not world geometry.

use super::{Hit, Primitive, PrimitiveKind, Ray, RenderError, Scene, intersect_primitive};
use spatial_math::Vec3;

/// Outward-rounded world-space bounds of one analytic primitive.
#[derive(Clone, Copy, Debug)]
pub struct PrimitiveBounds {
    min: [f64; 3],
    max: [f64; 3],
}
impl PrimitiveBounds {
    /// Lower corner, rounded one representable `f64` value outward on each axis.
    pub fn min(self) -> Vec3 {
        Vec3::new(self.min[0], self.min[1], self.min[2]).expect("finite primitive bounds")
    }
    /// Upper corner, rounded one representable `f64` value outward on each axis.
    pub fn max(self) -> Vec3 {
        Vec3::new(self.max[0], self.max[1], self.max[2]).expect("finite primitive bounds")
    }
    /// Whether a finite world point lies in the closed bounds.
    pub fn contains(self, point: Vec3) -> bool {
        let p = [point.x(), point.y(), point.z()];
        (0..3).all(|axis| p[axis] >= self.min[axis] && p[axis] <= self.max[axis])
    }
    /// Whether two closed bounds overlap.
    pub fn overlaps(self, other: Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= other.max[axis] && other.min[axis] <= self.max[axis])
    }
    fn union(self, other: Self) -> Self {
        let mut out = self;
        for axis in 0..3 {
            out.min[axis] = out.min[axis].min(other.min[axis]);
            out.max[axis] = out.max[axis].max(other.max[axis]);
        }
        out
    }
    fn center(self, axis: usize) -> f64 {
        (self.min[axis] + self.max[axis]) * 0.5
    }
    fn intersects(self, ray: Ray, limit: f64) -> bool {
        let origin = [ray.origin.x(), ray.origin.y(), ray.origin.z()];
        let direction = [ray.direction.x(), ray.direction.y(), ray.direction.z()];
        let mut near = ray.near;
        let mut far = limit;
        for axis in 0..3 {
            if direction[axis] == 0.0 {
                if origin[axis] < self.min[axis] || origin[axis] > self.max[axis] {
                    return false;
                }
                continue;
            }
            let a = (self.min[axis] - origin[axis]) / direction[axis];
            let b = (self.max[axis] - origin[axis]) / direction[axis];
            if !a.is_finite() || !b.is_finite() {
                return true;
            }
            near = near.max(a.min(b).next_down());
            far = far.min(a.max(b).next_up());
            if near > far {
                return false;
            }
        }
        true
    }
}

impl Primitive {
    /// Conservative `f64` bounds for the sphere, AABB, or capsule after supported transforms.
    pub fn bounds(self) -> PrimitiveBounds {
        let (low, high, radius) = match self.kind {
            PrimitiveKind::Capsule => (self.center, self.dimensions, self.radius),
            _ => (self.center, self.center, 0.0),
        };
        let a = [low.x(), low.y(), low.z()];
        let b = [high.x(), high.y(), high.z()];
        let extents = match self.kind {
            PrimitiveKind::Sphere => [self.dimensions.x(); 3],
            PrimitiveKind::Box => [
                self.dimensions.x(),
                self.dimensions.y(),
                self.dimensions.z(),
            ],
            PrimitiveKind::Capsule => [radius; 3],
        };
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for axis in 0..3 {
            min[axis] = (a[axis].min(b[axis]) - extents[axis]).next_down();
            max[axis] = (a[axis].max(b[axis]) + extents[axis]).next_up();
        }
        PrimitiveBounds { min, max }
    }
}

struct Node {
    bounds: PrimitiveBounds,
    left: usize,
    right: usize,
    start: usize,
    count: usize,
}

/// Deterministic median-split CPU BVH borrowing one immutable renderer snapshot.
pub struct Bvh<'a> {
    scene: &'a Scene,
    indices: Vec<usize>,
    nodes: Vec<Node>,
    depth: usize,
}
impl<'a> Bvh<'a> {
    /// Builds a binary hierarchy; leaves contain at most four analytic primitives.
    pub fn build(scene: &'a Scene) -> Result<Self, RenderError> {
        let count = scene.primitives().len();
        if count > u32::MAX as usize / 2 {
            return Err(RenderError::TooManyObjects);
        }
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(count)
            .map_err(|_| RenderError::TooManyObjects)?;
        indices.extend(0..count);
        let mut nodes = Vec::new();
        nodes
            .try_reserve(count.saturating_mul(2))
            .map_err(|_| RenderError::TooManyObjects)?;
        let mut bvh = Self {
            scene,
            indices,
            nodes,
            depth: 0,
        };
        if count > 0 {
            bvh.build_node(0, count, 1);
        }
        Ok(bvh)
    }
    fn build_node(&mut self, start: usize, end: usize, depth: usize) -> usize {
        self.depth = self.depth.max(depth);
        let bounds = self.indices[start..end]
            .iter()
            .map(|&i| self.scene.primitives[i].bounds())
            .reduce(PrimitiveBounds::union)
            .expect("nonempty node");
        let node = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            left: 0,
            right: 0,
            start,
            count: end - start,
        });
        if end - start > 4 {
            let span = [0, 1, 2].map(|axis| bounds.max[axis] - bounds.min[axis]);
            let axis = (0..3)
                .max_by(|&a, &b| span[a].total_cmp(&span[b]).then_with(|| b.cmp(&a)))
                .unwrap();
            self.indices[start..end].sort_by(|&a, &b| {
                self.scene.primitives[a]
                    .bounds()
                    .center(axis)
                    .total_cmp(&self.scene.primitives[b].bounds().center(axis))
                    .then_with(|| a.cmp(&b))
            });
            let middle = start + (end - start) / 2;
            let left = self.build_node(start, middle, depth + 1);
            let right = self.build_node(middle, end, depth + 1);
            self.nodes[node].left = left;
            self.nodes[node].right = right;
            self.nodes[node].count = 0;
        }
        node
    }
    /// Number of allocated BVH nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    /// Maximum root-to-leaf node count; zero for an empty snapshot.
    pub fn depth(&self) -> usize {
        self.depth
    }
    /// Resident CPU bytes for index and node allocations, excluding the borrowed scene.
    pub fn allocated_bytes(&self) -> usize {
        self.indices.capacity() * std::mem::size_of::<usize>()
            + self.nodes.capacity() * std::mem::size_of::<Node>()
    }
    /// Returns the same nearest analytic hit and insertion-order tie winner as `Scene::intersect`.
    pub fn intersect(&self, ray: Ray) -> Option<Hit> {
        let mut best = None;
        if !self.nodes.is_empty() {
            self.visit(0, ray, &mut best);
        }
        best.map(|(_, hit)| hit)
    }
    fn visit(&self, node_index: usize, ray: Ray, best: &mut Option<(usize, Hit)>) {
        let node = &self.nodes[node_index];
        if !node
            .bounds
            .intersects(ray, best.as_ref().map_or(ray.far, |(_, hit)| hit.distance))
        {
            return;
        }
        if node.count == 0 {
            self.visit(node.left, ray, best);
            self.visit(node.right, ray, best);
        } else {
            for &index in &self.indices[node.start..node.start + node.count] {
                if let Some(hit) = intersect_primitive(self.scene.primitives[index], ray)
                    && best.as_ref().is_none_or(|(previous_index, previous)| {
                        hit.distance < previous.distance
                            || (hit.distance == previous.distance && index < *previous_index)
                    })
                {
                    *best = Some((index, hit));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use analytic_field::{AxisAlignedBox, Capsule, Sphere};
    use spatial_math::Transform;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }

    #[test]
    fn primitive_bounds_contain_supported_shapes() {
        let sphere = Primitive::sphere(
            1,
            Sphere::new(2.0).unwrap(),
            Transform::new(v(3.0, -4.0, 5.0), 1.5).unwrap(),
            [1.0; 3],
        )
        .unwrap();
        let bounds = sphere.bounds();
        assert!(bounds.contains(v(0.0, -4.0, 5.0)));
        assert!(bounds.contains(v(6.0, -4.0, 5.0)));
        assert!(!bounds.contains(v(6.01, -4.0, 5.0)));

        let box_primitive = Primitive::axis_aligned_box(
            2,
            AxisAlignedBox::new(v(1.0, 2.0, 3.0)).unwrap(),
            Transform::new(v(-2.0, 1.0, 0.0), 2.0).unwrap(),
            [1.0; 3],
        )
        .unwrap();
        let box_bounds = box_primitive.bounds();
        assert!(box_bounds.contains(v(-4.0, -3.0, -6.0)));
        assert!(box_bounds.contains(v(0.0, 5.0, 6.0)));
        assert!(bounds.overlaps(box_bounds));

        let capsule = Primitive::capsule(
            3,
            Capsule::new(v(-2.0, 1.0, 4.0), v(3.0, -1.0, -2.0), 0.5).unwrap(),
            [1.0; 3],
        )
        .unwrap();
        let capsule_bounds = capsule.bounds();
        assert!(capsule_bounds.contains(v(-2.5, 1.0, 4.0)));
        assert!(capsule_bounds.contains(v(3.5, -1.0, -2.0)));
        assert!(!capsule_bounds.contains(v(3.51, 0.0, 0.0)));

        let degenerate = Primitive::capsule(
            4,
            Capsule::new(v(1.0, 2.0, 3.0), v(1.0, 2.0, 3.0), 0.001).unwrap(),
            [1.0; 3],
        )
        .unwrap();
        assert!(degenerate.bounds().contains(v(1.001, 2.0, 3.0)));
    }

    #[test]
    fn bvh_matches_direct_hits_and_insertion_order_ties() {
        let mut scene = Scene::default();
        scene
            .push(
                Primitive::sphere(
                    11,
                    Sphere::new(1.0).unwrap(),
                    Transform::identity(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        scene
            .push(
                Primitive::sphere(
                    12,
                    Sphere::new(1.0).unwrap(),
                    Transform::identity(),
                    [1.0; 3],
                )
                .unwrap(),
            )
            .unwrap();
        for index in 0..16 {
            scene
                .push(
                    Primitive::sphere(
                        100 + index,
                        Sphere::new(0.3).unwrap(),
                        Transform::new(v((index as f64 - 8.0) * 0.7, 0.0, 2.0), 1.0).unwrap(),
                        [1.0; 3],
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let bvh = Bvh::build(&scene).unwrap();
        assert!(bvh.node_count() > 1);
        assert!(bvh.depth() > 1);
        assert_eq!(
            bvh.intersect(Ray::new(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0), 0.0, 10.0).unwrap())
                .unwrap()
                .id,
            11
        );
        for x in -40..=40 {
            for y in -5..=5 {
                let ray = Ray::new(
                    v(x as f64 * 0.15, y as f64 * 0.15, -3.0),
                    v(0.0, 0.0, 1.0),
                    0.0,
                    10.0,
                )
                .unwrap();
                let direct = scene.intersect(ray);
                let accelerated = bvh.intersect(ray);
                assert_eq!(direct.map(|hit| hit.id), accelerated.map(|hit| hit.id));
                if let (Some(direct), Some(accelerated)) = (direct, accelerated) {
                    assert_eq!(direct.distance, accelerated.distance);
                    assert_eq!(direct.normal, accelerated.normal);
                }
            }
        }
        assert!(
            Bvh::build(&Scene::default())
                .unwrap()
                .intersect(Ray::new(v(0.0, 0.0, -3.0), v(0.0, 0.0, 1.0), 0.0, 10.0).unwrap())
                .is_none()
        );
    }
}
