# Architecture overview

**Status: experimental / research stage.** Version 0.1 proves a small CPU-only vertical slice: analytic geometry, mutable state, a fixed-step growth rule, and an embeddable coordinator. It is not a renderer or a complete engine.

The source of truth is analytic parameters and rules. Sampling at a point is a query, not a stored mesh or voxel model. A future renderer may produce pixels, textures, or acceleration structures without changing this source of truth.

Current flow: `Vec3 / Transform` → `Sphere / AxisAlignedBox / Field` → `WorldState` → `advance` → `Runtime`. The example executes this flow and prints the resulting radius and field sample.

Future modules are conditional on experiments: a compiler for field graphs, a `wgpu` / WGSL renderer, physics with compatible field contracts, a language for authored rules, an editor, and an FFI boundary. None exists yet. A standalone application can wrap the same runtime later; the runtime has no UI dependency.

## Research status

- Established knowledge: analytic sphere and box SDF formulas and sign-based CSG combinations.
- Decision: retain analytic state; use CPU `f64` for this slice.
- Hypothesis: a field-first model can support shared rendering and simulation queries at useful scale.
- Open questions: spatial indexing, robust conservative bounds, surface gradients, rule scheduling, persistence, cross-platform reproducibility, GPU lowering.
- Experimental findings: only local compilation and tests reported in this repository; no performance claim.
