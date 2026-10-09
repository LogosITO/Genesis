//! Analytic swept-sphere contact against authoritative spheres and capsules.

use spatial_math::{MathError, Vec3};
use world_state::{BodyContact, CONNECTION_RADIUS_RATIO, ColliderId, WorldError, WorldState};

/// A bounded collision query result. An uncertain root is never reported as a miss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SweepOutcome {
    /// Earliest contact, with stable collider identity.
    Hit(BodyContact),
    /// No supported solid touched the sweep.
    Miss,
    /// Floating-point roots were too close to a tangent to classify safely.
    Indeterminate,
}

/// Contact query failure before a result can be classified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactError {
    /// Invalid world or numeric input.
    World(WorldError),
    /// A query exceeded the supported coordinate or work domain.
    OutOfRange,
    /// A near-tangent quadratic root could not be classified safely.
    Indeterminate,
}
impl std::fmt::Display for ContactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ContactError {}
impl From<WorldError> for ContactError {
    fn from(error: WorldError) -> Self {
        Self::World(error)
    }
}
impl From<MathError> for ContactError {
    fn from(error: MathError) -> Self {
        Self::World(error.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Solid {
    id: ColliderId,
    a: Vec3,
    b: Vec3,
    radius: f64,
    bounds: Bounds,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}
impl Bounds {
    fn solid(a: Vec3, b: Vec3, radius: f64) -> Self {
        let aa = coords(a);
        let bb = coords(b);
        Self {
            min: std::array::from_fn(|i| (aa[i].min(bb[i]) - radius).next_down()),
            max: std::array::from_fn(|i| (aa[i].max(bb[i]) + radius).next_up()),
        }
    }
    fn sweep(start: Vec3, end: Vec3, radius: f64) -> Self {
        Self::solid(start, end, radius)
    }
    fn union(self, rhs: Self) -> Self {
        Self {
            min: std::array::from_fn(|i| self.min[i].min(rhs.min[i])),
            max: std::array::from_fn(|i| self.max[i].max(rhs.max[i])),
        }
    }
    fn overlaps(self, rhs: Self) -> bool {
        (0..3).all(|i| self.min[i] <= rhs.max[i] && rhs.min[i] <= self.max[i])
    }
    fn centre(self, axis: usize) -> f64 {
        self.min[axis] / 2.0 + self.max[axis] / 2.0
    }
}
#[derive(Clone, Debug)]
struct Node {
    bounds: Bounds,
    left: usize,
    right: usize,
    start: usize,
    count: usize,
}

/// Snapshot of supported world solids. The BVH only rejects disjoint conservative bounds.
/// Reuse requires exact authoritative collider equality; no GPU dependency exists.
#[derive(Clone, Debug)]
pub struct ContactScene {
    solids: Vec<Solid>,
    indices: Vec<usize>,
    nodes: Vec<Node>,
}
impl ContactScene {
    /// Builds exact sphere and capsule narrow-phase data from a validated world.
    pub fn from_world(world: &WorldState) -> Result<Self, ContactError> {
        let solids = Self::collect_solids(world)?;
        Ok(Self::from_solids(solids))
    }
    fn collect_solids(world: &WorldState) -> Result<Vec<Solid>, ContactError> {
        world.validate()?;
        let mut solids = Vec::new();
        for entity in world.entities() {
            let centre = entity.transform().translation();
            let radius = entity.sphere().radius() * entity.transform().scale();
            if !radius.is_finite() || radius <= 0.0 {
                return Err(ContactError::OutOfRange);
            }
            solids.push(Solid {
                id: ColliderId::Sphere(entity.id()),
                a: centre,
                b: centre,
                radius,
                bounds: Bounds::solid(centre, centre, radius),
            });
        }
        for organism in world.organisms() {
            let radius = organism.parameters().node_radius();
            for node in organism.nodes() {
                let p = node.position();
                solids.push(Solid {
                    id: ColliderId::Node {
                        organism: organism.id(),
                        node: node.id(),
                    },
                    a: p,
                    b: p,
                    radius,
                    bounds: Bounds::solid(p, p, radius),
                });
                if let Some(parent) = node.parent() {
                    let a = organism
                        .node(parent)
                        .ok_or(ContactError::OutOfRange)?
                        .position();
                    let r = radius * CONNECTION_RADIUS_RATIO;
                    solids.push(Solid {
                        id: ColliderId::Connection {
                            organism: organism.id(),
                            child: node.id(),
                        },
                        a,
                        b: p,
                        radius: r,
                        bounds: Bounds::solid(a, p, r),
                    });
                }
            }
        }
        Ok(solids)
    }
    fn from_solids(solids: Vec<Solid>) -> Self {
        let count = solids.len();
        let mut scene = Self {
            solids,
            indices: (0..count).collect(),
            nodes: Vec::new(),
        };
        if count > 0 {
            scene.build(0, count);
        }
        scene
    }
    /// Reuses the BVH only when every authoritative collider identity and shape is unchanged.
    /// Returns whether a full rebuild was necessary.
    pub fn refresh(&mut self, world: &WorldState) -> Result<bool, ContactError> {
        let solids = Self::collect_solids(world)?;
        if self.solids == solids {
            return Ok(false);
        }
        *self = Self::from_solids(solids);
        Ok(true)
    }
    fn build(&mut self, start: usize, end: usize) -> usize {
        let bounds = self.indices[start..end]
            .iter()
            .map(|&i| self.solids[i].bounds)
            .reduce(Bounds::union)
            .expect("nonempty node");
        let index = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            left: 0,
            right: 0,
            start,
            count: end - start,
        });
        if end - start > 4 {
            let axis = (0..3)
                .max_by(|&a, &b| {
                    (bounds.max[a] - bounds.min[a])
                        .total_cmp(&(bounds.max[b] - bounds.min[b]))
                        .then_with(|| b.cmp(&a))
                })
                .expect("three axes");
            self.indices[start..end].sort_by(|&a, &b| {
                self.solids[a]
                    .bounds
                    .centre(axis)
                    .total_cmp(&self.solids[b].bounds.centre(axis))
                    .then_with(|| a.cmp(&b))
            });
            let mid = start + (end - start) / 2;
            let left = self.build(start, mid);
            let right = self.build(mid, end);
            self.nodes[index].left = left;
            self.nodes[index].right = right;
            self.nodes[index].count = 0;
        }
        index
    }
    /// Number of supported solid primitives.
    pub fn solid_count(&self) -> usize {
        self.solids.len()
    }
    /// CPU BVH node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    /// Sweeps a sphere through one finite displacement using the BVH broad phase.
    pub fn sweep(
        &self,
        start: Vec3,
        displacement: Vec3,
        radius: f64,
    ) -> Result<SweepOutcome, ContactError> {
        self.query(start, displacement, radius, true)
    }
    /// Direct reference query for differential testing.
    pub fn sweep_direct(
        &self,
        start: Vec3,
        displacement: Vec3,
        radius: f64,
    ) -> Result<SweepOutcome, ContactError> {
        self.query(start, displacement, radius, false)
    }
    fn query(
        &self,
        start: Vec3,
        displacement: Vec3,
        radius: f64,
        accelerated: bool,
    ) -> Result<SweepOutcome, ContactError> {
        if !radius.is_finite() || !(0.001..=10.0).contains(&radius) {
            return Err(ContactError::OutOfRange);
        }
        let end = start.checked_add(displacement)?;
        if coords(start)
            .into_iter()
            .chain(coords(end))
            .any(|x| x.abs() > 1000.0)
        {
            return Err(ContactError::OutOfRange);
        }
        let swept = Bounds::sweep(start, end, radius);
        let candidates = self.candidates(swept, accelerated);
        let mut nearest: Option<BodyContact> = None;
        for index in candidates {
            let solid = self.solids[index];
            if !swept.overlaps(solid.bounds) {
                continue;
            }
            match sweep_solid(start, displacement, radius, solid)? {
                SweepOutcome::Hit(hit)
                    if nearest.is_none_or(|prior| hit.fraction < prior.fraction) =>
                {
                    nearest = Some(hit)
                }
                SweepOutcome::Indeterminate => return Ok(SweepOutcome::Indeterminate),
                _ => {}
            }
        }
        Ok(nearest.map_or(SweepOutcome::Miss, SweepOutcome::Hit))
    }
    fn candidates(&self, swept: Bounds, accelerated: bool) -> Vec<usize> {
        let mut candidates = Vec::new();
        if accelerated && !self.nodes.is_empty() {
            let mut stack = vec![0];
            while let Some(index) = stack.pop() {
                let node = &self.nodes[index];
                if !swept.overlaps(node.bounds) {
                    continue;
                }
                if node.count == 0 {
                    stack.push(node.right);
                    stack.push(node.left);
                } else {
                    candidates
                        .extend_from_slice(&self.indices[node.start..node.start + node.count]);
                }
            }
            candidates.sort_unstable();
        } else {
            candidates.extend(0..self.solids.len());
        }
        candidates
    }
}

fn coords(v: Vec3) -> [f64; 3] {
    [v.x(), v.y(), v.z()]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x() * b.x() + a.y() * b.y() + a.z() * b.z()
}
fn nearest_segment(point: Vec3, a: Vec3, b: Vec3) -> Result<Vec3, ContactError> {
    let axis = b.checked_sub(a)?;
    let length2 = dot(axis, axis);
    let t = if length2 == 0.0 {
        0.0
    } else {
        (dot(point.checked_sub(a)?, axis) / length2).clamp(0.0, 1.0)
    };
    Ok(a.checked_add(axis.checked_scale(t)?)?)
}
fn roots(a: f64, b: f64, c: f64) -> Result<Option<[f64; 2]>, ContactError> {
    if a == 0.0 {
        return Ok(None);
    }
    let term = 4.0 * a * c;
    let disc = b.mul_add(b, -term);
    if !disc.is_finite() {
        return Err(ContactError::OutOfRange);
    }
    let uncertainty = 64.0 * f64::EPSILON * (b * b + term.abs());
    if disc < 0.0 {
        if disc >= -uncertainty {
            return Err(ContactError::Indeterminate);
        }
        return Ok(None);
    }
    if disc > 0.0 && disc <= uncertainty {
        return Err(ContactError::Indeterminate);
    }
    let root = disc.sqrt();
    let q = -0.5 * (b + root.copysign(b));
    let mut pair = if q == 0.0 {
        [-b / (2.0 * a); 2]
    } else {
        [q / a, c / q]
    };
    pair.sort_by(f64::total_cmp);
    Ok(Some(pair))
}
fn sweep_solid(
    start: Vec3,
    movement: Vec3,
    body_radius: f64,
    solid: Solid,
) -> Result<SweepOutcome, ContactError> {
    let radius = body_radius + solid.radius;
    if !radius.is_finite() {
        return Err(ContactError::OutOfRange);
    }
    let closest = nearest_segment(start, solid.a, solid.b)?;
    let offset = start.checked_sub(closest)?;
    let distance = offset.length()?;
    if distance < radius || (distance == radius && dot(offset, movement) < 0.0) {
        return Ok(SweepOutcome::Hit(BodyContact {
            collider: solid.id,
            fraction: 0.0,
            normal: if distance == 0.0 {
                None
            } else {
                Some(offset.normalized()?)
            },
            initial_overlap: distance < radius,
        }));
    }
    let mut first: Option<(f64, Vec3)> = None;
    for centre in [solid.a, solid.b] {
        let m = start.checked_sub(centre)?;
        if let Some(pair) = roots(
            dot(movement, movement),
            2.0 * dot(m, movement),
            dot(m, m) - radius * radius,
        )? {
            for t in pair {
                if (0.0..=1.0).contains(&t)
                    && !(t == 0.0 && dot(m, movement) >= 0.0)
                    && first.is_none_or(|(old, _)| t < old)
                {
                    let position = start.checked_add(movement.checked_scale(t)?)?;
                    first = Some((t, position.checked_sub(centre)?));
                }
            }
        }
    }
    let axis = solid.b.checked_sub(solid.a)?;
    let length = axis.length()?;
    if length > 0.0 {
        let unit = axis.checked_scale(1.0 / length)?;
        let m = start.checked_sub(solid.a)?;
        let m_perp = m.checked_sub(unit.checked_scale(dot(m, unit))?)?;
        let v_perp = movement.checked_sub(unit.checked_scale(dot(movement, unit))?)?;
        if let Some(pair) = roots(
            dot(v_perp, v_perp),
            2.0 * dot(m_perp, v_perp),
            dot(m_perp, m_perp) - radius * radius,
        )? {
            for t in pair {
                let projection = dot(m, unit) + t * dot(movement, unit);
                if (0.0..=1.0).contains(&t)
                    && (0.0..=length).contains(&projection)
                    && !(t == 0.0 && dot(m_perp, v_perp) >= 0.0)
                    && first.is_none_or(|(old, _)| t < old)
                {
                    first = Some((t, m_perp.checked_add(v_perp.checked_scale(t)?)?));
                }
            }
        }
    }
    match first {
        Some((fraction, normal)) if normal != Vec3::ZERO => Ok(SweepOutcome::Hit(BodyContact {
            collider: solid.id,
            fraction,
            normal: Some(normal.normalized()?),
            initial_overlap: false,
        })),
        Some(_) => Ok(SweepOutcome::Indeterminate),
        None => Ok(SweepOutcome::Miss),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use analytic_field::Sphere;
    use spatial_math::Transform;
    use world_state::{DeterministicSeed, GrowthParameters};

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z).unwrap()
    }
    fn sphere_world() -> WorldState {
        let mut world = WorldState::new(DeterministicSeed(1));
        world
            .spawn_sphere(Sphere::new(1.0).unwrap(), Transform::identity(), 0.0)
            .unwrap();
        world
    }
    #[test]
    fn sphere_sweep_covers_high_speed_tangent_overlap_and_zero_motion() {
        let scene = ContactScene::from_world(&sphere_world()).unwrap();
        let SweepOutcome::Hit(hit) = scene
            .sweep(v(-3.0, 0.0, 0.0), v(6.0, 0.0, 0.0), 0.5)
            .unwrap()
        else {
            panic!("expected hit")
        };
        assert_eq!(hit.fraction, 0.25);
        assert_eq!(hit.normal, Some(v(-1.0, 0.0, 0.0)));
        assert!(!hit.initial_overlap);
        assert!(matches!(
            scene
                .sweep(v(-3.0, 1.5, 0.0), v(6.0, 0.0, 0.0), 0.5)
                .unwrap(),
            SweepOutcome::Hit(_)
        ));
        assert!(matches!(
            scene.sweep(v(-3.0, 0.0, 0.0), Vec3::ZERO, 0.5).unwrap(),
            SweepOutcome::Miss
        ));
        let SweepOutcome::Hit(overlap) = scene.sweep(Vec3::ZERO, Vec3::ZERO, 0.5).unwrap() else {
            panic!("expected overlap")
        };
        assert!(overlap.initial_overlap);
        assert_eq!(overlap.normal, None);
        assert!(scene.sweep(Vec3::ZERO, Vec3::ZERO, f64::NAN).is_err());
        assert!(
            scene
                .sweep(v(999.0, 0.0, 0.0), v(2.0, 0.0, 0.0), 0.5)
                .is_err()
        );
    }

    #[test]
    fn capsule_side_parallel_motion_and_degenerate_segment() {
        let id = ColliderId::Sphere(sphere_world().entities()[0].id());
        let solid = Solid {
            id,
            a: v(0.0, 0.0, 0.0),
            b: v(0.0, 1.0, 0.0),
            radius: 0.1,
            bounds: Bounds::solid(v(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.1),
        };
        let SweepOutcome::Hit(hit) =
            sweep_solid(v(-2.0, 0.5, 0.0), v(4.0, 0.0, 0.0), 0.2, solid).unwrap()
        else {
            panic!("capsule side")
        };
        assert!((hit.fraction - 0.425).abs() < 1e-12);
        assert_eq!(hit.normal, Some(v(-1.0, 0.0, 0.0)));
        assert!(matches!(
            sweep_solid(v(0.31, -0.5, 0.0), v(0.0, 2.0, 0.0), 0.2, solid).unwrap(),
            SweepOutcome::Miss
        ));
        let degenerate = Solid {
            b: solid.a,
            bounds: Bounds::solid(solid.a, solid.a, 0.1),
            ..solid
        };
        assert!(matches!(
            sweep_solid(v(-2.0, 0.0, 0.0), v(4.0, 0.0, 0.0), 0.2, degenerate).unwrap(),
            SweepOutcome::Hit(_)
        ));
    }

    #[test]
    fn surface_near_tangent_and_nearest_identity_are_classified_conservatively() {
        let mut world = sphere_world();
        let first = world.entities()[0].id();
        let second = world
            .spawn_sphere(
                Sphere::new(1.0).unwrap(),
                Transform::new(v(1e-10, 0.0, 0.0), 1.0).unwrap(),
                0.0,
            )
            .unwrap();
        let scene = ContactScene::from_world(&world).unwrap();
        let SweepOutcome::Hit(hit) = scene
            .sweep(v(-3.0, 0.0, 0.0), v(6.0, 0.0, 0.0), 0.5)
            .unwrap()
        else {
            panic!("expected nearest sphere")
        };
        assert_eq!(hit.collider, ColliderId::Sphere(first));
        assert_eq!(hit.fraction, 0.25);
        assert_ne!(first, second);
        assert!(matches!(
            scene
                .sweep(v(-1.5, 0.0, 0.0), v(-1.0, 0.0, 0.0), 0.5)
                .unwrap(),
            SweepOutcome::Miss
        ));
        assert!(matches!(
            scene
                .sweep(v(-1.5, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.5)
                .unwrap(),
            SweepOutcome::Hit(BodyContact {
                fraction: 0.0,
                initial_overlap: false,
                ..
            })
        ));
        let SweepOutcome::Hit(short) = scene
            .sweep(v(-1.500000001, 0.0, 0.0), v(2e-9, 0.0, 0.0), 0.5)
            .unwrap()
        else {
            panic!("expected short sweep entry")
        };
        assert!((short.fraction - 0.5).abs() < 1e-6);
        for offset in [-1e-14, 1e-14] {
            let result = scene.sweep(v(-3.0, 1.5 + offset, 0.0), v(6.0, 0.0, 0.0), 0.5);
            assert_eq!(
                result,
                scene.sweep_direct(v(-3.0, 1.5 + offset, 0.0), v(6.0, 0.0, 0.0), 0.5)
            );
            if offset < 0.0 {
                assert!(matches!(
                    result,
                    Ok(SweepOutcome::Hit(_) | SweepOutcome::Indeterminate)
                        | Err(ContactError::Indeterminate)
                ));
            } else {
                assert!(matches!(
                    result,
                    Ok(SweepOutcome::Miss | SweepOutcome::Indeterminate)
                        | Err(ContactError::Indeterminate)
                ));
            }
        }
    }

    #[test]
    fn cached_scene_rebuilds_only_for_authoritative_collider_geometry() {
        let mut world = WorldState::new(DeterministicSeed(8));
        let organism = world
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.1, 0.3, 0.1, 1.0).unwrap(),
            )
            .unwrap();
        let source = world
            .spawn_finite_source(v(0.0, 1.0, 0.0), 3.0, 1.0, 0.5, 1.0, 0.1)
            .unwrap();
        let body = world.spawn_body(v(0.0, 0.0, -1.0), 0.1).unwrap();
        let mut scene = ContactScene::from_world(&world).unwrap();
        world.source_mut(source).unwrap().begin_tick().unwrap();
        world.source_mut(source).unwrap().allocate(0.2).unwrap();
        world
            .source_mut(source)
            .unwrap()
            .move_to(v(0.5, 1.0, 0.0))
            .unwrap();
        world
            .body_mut(body)
            .unwrap()
            .apply_motion(v(0.0, 0.0, -0.5), None)
            .unwrap();
        assert!(!scene.refresh(&world).unwrap());
        let sphere = world
            .spawn_sphere(
                Sphere::new(0.2).unwrap(),
                Transform::new(v(2.0, 0.0, 0.0), 1.0).unwrap(),
                0.1,
            )
            .unwrap();
        assert!(scene.refresh(&world).unwrap());
        assert!(!scene.refresh(&world).unwrap());
        assert_eq!(world.entity(sphere).unwrap().sphere().radius(), 0.2);
        world.apply_radii(&[Sphere::new(0.3).unwrap()]).unwrap();
        assert!(scene.refresh(&world).unwrap());
        world.organisms_mut()[0]
            .grow(&[(0.1, v(0.0, 1.0, 0.0))])
            .unwrap();
        assert!(scene.refresh(&world).unwrap());
        assert_eq!(scene.solid_count(), 4);
        assert_eq!(
            scene.sweep(v(0.0, 0.3, -1.0), v(0.0, 0.0, 2.0), 0.1),
            scene.sweep_direct(v(0.0, 0.3, -1.0), v(0.0, 0.0, 2.0), 0.1)
        );
        world.prune_branch(organism, 1).unwrap();
        assert!(scene.refresh(&world).unwrap());
        assert_eq!(scene.solid_count(), 2);
        assert!(!scene.refresh(&world).unwrap());
    }

    #[test]
    fn direct_and_bvh_agree_on_dense_deterministic_sweeps() {
        let mut world = WorldState::new(DeterministicSeed(3));
        for i in 0..128 {
            let x = (i % 16) as f64 * 0.5 - 4.0;
            let y = (i / 16) as f64 * 0.5 - 2.0;
            world
                .spawn_sphere(
                    Sphere::new(0.13).unwrap(),
                    Transform::new(v(x, y, 0.0), 1.0).unwrap(),
                    0.0,
                )
                .unwrap();
        }
        let scene = ContactScene::from_world(&world).unwrap();
        assert!(scene.node_count() > 1);
        let mut state = 7_u64;
        for _ in 0..1024 {
            let mut next = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((state >> 32) as f64 / u32::MAX as f64) * 2.0 - 1.0
            };
            let start = v(next() * 5.0, next() * 3.0, -2.0);
            let movement = v(next() * 2.0, next() * 2.0, 4.0);
            assert_eq!(
                scene.sweep(start, movement, 0.1),
                scene.sweep_direct(start, movement, 0.1)
            );
        }
    }

    fn full_world() -> WorldState {
        let mut world = WorldState::new(DeterministicSeed(7));
        for i in 0..4 {
            world
                .spawn_organism(
                    v(i as f64 - 1.5, 0.0, 0.0),
                    GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
                )
                .unwrap();
        }
        world.spawn_source(v(0.0, 2.0, 0.0), 100.0, 10.0).unwrap();
        let mut time = crate::SimulationTime::default();
        let step = crate::SimulationStep::new(0.01).unwrap();
        for _ in 0..2000 {
            if world
                .organisms()
                .iter()
                .all(|tree| tree.nodes().len() == 48)
            {
                break;
            }
            crate::advance_life(&mut world, &mut time, step, &[]).unwrap();
        }
        assert!(
            world
                .organisms()
                .iter()
                .all(|tree| tree.nodes().len() == 48)
        );
        world
    }

    #[test]
    fn real_381_primitive_growth_scene_has_380_current_colliders() {
        let mut world = full_world();
        let scene = ContactScene::from_world(&world).unwrap();
        assert_eq!(scene.solid_count(), 380); // The 381st rendered primitive is a non-solid resource marker.
        let mut state = 97_u64;
        for _ in 0..2048 {
            let mut next = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((state >> 32) as f64 / u32::MAX as f64) * 2.0 - 1.0
            };
            let start = v(next() * 3.0, 1.0 + next(), -3.0);
            let movement = v(next(), next(), 6.0);
            assert_eq!(
                scene.sweep(start, movement, 0.1),
                scene.sweep_direct(start, movement, 0.1)
            );
        }
        let organism = world.organisms()[0].id();
        world.prune_branch(organism, 1).unwrap();
        let changed = ContactScene::from_world(&world).unwrap();
        assert!(changed.solids.iter().all(|solid| !matches!(solid.id,
            ColliderId::Node { organism: id, node: 1 } | ColliderId::Connection { organism: id, child: 1 } if id == organism)));
    }

    #[test]
    #[ignore = "local CPU timing experiment; run serially in release"]
    fn contact_cpu_benchmark() {
        use std::time::Instant;
        fn stats(mut values: Vec<f64>) -> (f64, f64) {
            values.sort_by(f64::total_cmp);
            (
                values[values.len() / 2],
                values[(values.len() * 95).div_ceil(100) - 1],
            )
        }
        fn measure(label: &str, world: &WorldState) {
            let start = v(0.12, 0.32, -2.0);
            let movement = v(0.0, 0.0, 4.0);
            let bounds = Bounds::sweep(start, start.checked_add(movement).unwrap(), 0.1);
            let scene = ContactScene::from_world(world).unwrap();
            let mut sim_world = world.clone();
            let body = sim_world.spawn_body(v(0.12, 0.32, -0.5), 0.1).unwrap();
            sim_world
                .body_mut(body)
                .unwrap()
                .set_desired_velocity(v(0.0, 0.0, 100.0))
                .unwrap();
            let mut direct_candidates = Vec::new();
            let mut bvh_candidates = Vec::new();
            let mut narrow = Vec::new();
            let mut direct_query = Vec::new();
            let mut bvh_query = Vec::new();
            let mut rebuild = Vec::new();
            let mut refresh = Vec::new();
            let mut full_tick = Vec::new();
            let mut cached_tick = Vec::new();
            let mut reusable_scene = ContactScene::from_world(world).unwrap();
            let mut cache = None;
            for _ in 0..50 {
                let now = Instant::now();
                std::hint::black_box(scene.candidates(bounds, false));
                direct_candidates.push(now.elapsed().as_secs_f64() * 1e3);
                let now = Instant::now();
                let candidates = scene.candidates(bounds, true);
                bvh_candidates.push(now.elapsed().as_secs_f64() * 1e3);
                let now = Instant::now();
                for index in candidates {
                    let solid = scene.solids[index];
                    if bounds.overlaps(solid.bounds) {
                        std::hint::black_box(sweep_solid(start, movement, 0.1, solid).unwrap());
                    }
                }
                narrow.push(now.elapsed().as_secs_f64() * 1e3);
                let now = Instant::now();
                std::hint::black_box(scene.sweep_direct(start, movement, 0.1).unwrap());
                direct_query.push(now.elapsed().as_secs_f64() * 1e3);
                let now = Instant::now();
                std::hint::black_box(scene.sweep(start, movement, 0.1).unwrap());
                bvh_query.push(now.elapsed().as_secs_f64() * 1e3);
                let now = Instant::now();
                std::hint::black_box(ContactScene::from_world(world).unwrap());
                rebuild.push(now.elapsed().as_secs_f64() * 1e3);
                let now = Instant::now();
                assert!(!reusable_scene.refresh(world).unwrap());
                refresh.push(now.elapsed().as_secs_f64() * 1e3);
                let mut changed = sim_world.clone();
                let mut time = crate::SimulationTime::default();
                let now = Instant::now();
                crate::advance_life(
                    &mut changed,
                    &mut time,
                    crate::SimulationStep::new(0.01).unwrap(),
                    &[],
                )
                .unwrap();
                full_tick.push(now.elapsed().as_secs_f64() * 1e3);
                let mut changed = sim_world.clone();
                let mut time = crate::SimulationTime::default();
                let now = Instant::now();
                crate::advance_life_cached(
                    &mut changed,
                    &mut time,
                    crate::SimulationStep::new(0.01).unwrap(),
                    &[],
                    &mut cache,
                )
                .unwrap();
                cached_tick.push(now.elapsed().as_secs_f64() * 1e3);
            }
            eprintln!(
                "contact_cpu scene={label} solids={} nodes={} direct_candidates_ms={:?} bvh_candidates_ms={:?} narrow_ms={:?} direct_query_ms={:?} bvh_query_ms={:?} rebuild_ms={:?} refresh_ms={:?} full_tick_ms={:?} cached_tick_ms={:?}",
                scene.solid_count(),
                scene.node_count(),
                stats(direct_candidates),
                stats(bvh_candidates),
                stats(narrow),
                stats(direct_query),
                stats(bvh_query),
                stats(rebuild),
                stats(refresh),
                stats(full_tick),
                stats(cached_tick)
            );
        }
        let mut small = WorldState::new(DeterministicSeed(3));
        small
            .spawn_organism(
                Vec3::ZERO,
                GrowthParameters::new(0.14, 0.32, 0.12, 1.0).unwrap(),
            )
            .unwrap();
        small.spawn_source(v(0.0, 2.0, 0.0), 4.0, 1.0).unwrap();
        let mut time = crate::SimulationTime::default();
        for _ in 0..60 {
            crate::advance_life(
                &mut small,
                &mut time,
                crate::SimulationStep::new(1.0 / 60.0).unwrap(),
                &[],
            )
            .unwrap();
        }
        measure("small", &small);
        let mut large = full_world();
        measure("real-381", &large);
        let id = large.organisms()[0].id();
        large.prune_branch(id, 1).unwrap();
        measure("post-prune", &large);
    }
}
