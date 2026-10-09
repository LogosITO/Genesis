# Spatial acceleration research

**Status: CPU experiment.** `Bvh` borrows an immutable renderer `Scene`; it is neither world state nor saved data. The direct CPU and GPU analytic paths remain the rendering reference. No GPU BVH traversal or camera culling is enabled.

The CPU builder calculates world-space bounds for each sphere, axis-aligned box, and capsule. It makes a binary median split on the widest node axis, with insertion index as a stable tie breaker. Leaves hold at most four primitive indices. Nodes retain their bounds and child indices; the snapshot retains the analytic parameters and stable IDs. An empty scene has no nodes. A new snapshot requires a new BVH; topology changes cannot leave a stale tree attached to a new scene. The viewport currently rebuilds its snapshot each frame and does not build a BVH.

CPU traversal rejects a node when its bounds do not overlap the closed ray segment, then calls the existing analytic primitive intersection in each visited leaf. Equal-distance hits select the earlier snapshot index, matching `Scene::intersect`. There is no recursive shader traversal, stack limit, or silent leaf omission because this path is CPU-only. The builder uses fallible vector reservation and reports `TooManyObjects` on allocation failure.

The bounds expand arithmetic endpoints one representable `f64` value outward. This handles rounding of the endpoint operation but is **not a formal proof** of containment for all floating-point intermediate calculations or GPU `f32` conversion. The CPU reference itself is not certified near tangencies and extreme scales. The hierarchy is therefore experimental; it is not used to cull GPU geometry. See [primitive bounds](../specifications/primitive-bounds.md), [snapshot](../specifications/render-snapshot.md), and [measurements](../research/first-scale-benchmark.md).

Camera-frustum culling remains disabled. A center-only test would incorrectly remove long capsules and nearby large primitives. Future culling must cover the complete pixel-ray frustum, near/far planes, transformed bounds, and numerical margins, and must revalidate on camera movement and resize.
