# Architecture overview

Foundation 0.7.1 adds a single kinematic body and CPU-only analytic sphere/capsule sweep in `world-simulation`. Contact uses a conservative, rebuild-on-step CPU BVH; the renderer maintains its own GPU acceleration. The body and velocity events persist in version-4 saves. See the [contact specification](../specifications/kinematic-contact.md).

**Status: experimental / research stage.** Foundation 0.1 established analytic geometry, mutable state, fixed-step growth, and an embeddable coordinator. Foundation 0.2 added gradients, ideal Lipschitz zero-set bounds, and bounded CPU primitive ray queries. Foundation 0.3 added an optional native GPU prototype for two analytic primitives. Foundation 0.4 adds a bounded mathematical growth graph and continuous resource environment. It is not a complete engine.

The source of truth is analytic parameters and rules. Sampling at a point is a query, not a stored mesh or voxel model. The current renderer produces pixels and textures without changing this source of truth; future acceleration structures may do the same.

Current flow: `Vec3 / Transform` → `Sphere / AxisAlignedBox / Field` → `WorldState` → `advance` / `advance_life` → `Runtime`. `WorldState` owns ordinary spheres, growth nodes, and resource sources. `world-simulation` applies events and growth on a candidate state before atomic commit. `world-runtime` coordinates ticks, pending events, and bounded versioned saves. The renderer takes a bounded snapshot of world spheres, growth nodes, and sources, sends analytic parameters to WGSL, and computes each pixel by ray intersection. A full-screen triangle only presents the computed image; it is not world geometry. The original `hello-world` and First Light modes remain. See [renderer architecture](renderer.md).

Future modules are conditional on experiments: a compiler for field graphs, physics with compatible field contracts, a language for authored rules, an editor, and an FFI boundary. The current GPU prototype does not lower general field graphs. The runtime remains free of UI and GPU dependencies.

## Research status

- Established knowledge: analytic sphere and box SDF formulas and sign-based CSG combinations.
- Decision: retain analytic state; use CPU `f64`, bounded GPU `f32` snapshots, and analytic sphere/AABB intersections.
- Hypothesis: a field-first model can support shared rendering and simulation queries at useful scale.
- Open questions: spatial indexing, robust conservative bounds, finite resource competition, cross-platform reproducibility, GPU field lowering, and useful branching rules.
- Experimental finding: the three First Life headless scenarios produce distinct node counts and tip positions; see the [growth model](../specifications/growth-model.md) and [measurement record](../research/first-life-measurements.md).
- Experimental findings: one native Vulkan adapter passed the local parity and resolution smoke tests; see the [benchmark record](../research/first-light-benchmark.md). No general performance claim.
