# Architecture overview

**Status: experimental / research stage.** Foundation 0.1 established a CPU-only vertical slice: analytic geometry, mutable state, fixed-step growth, and an embeddable coordinator. Foundation 0.2 adds analytic gradients, ideal Lipschitz zero-set bounds, and bounded primitive ray queries. It is not a renderer or a complete engine.

The source of truth is analytic parameters and rules. Sampling at a point is a query, not a stored mesh or voxel model. A future renderer may produce pixels, textures, or acceleration structures without changing this source of truth.

Current flow: `Vec3 / Transform` → `Sphere / AxisAlignedBox / Field` → `WorldState` → `advance` → `Runtime`. The original example executes this flow and prints the resulting radius and field sample. The CPU query path in `field` is a reference for future GPU comparisons, with no certified floating-point tracing guarantee.

Future modules are conditional on experiments: a compiler for field graphs, a `wgpu` / WGSL renderer, physics with compatible field contracts, a language for authored rules, an editor, and an FFI boundary. None exists yet. A standalone application can wrap the same runtime later; the runtime has no UI dependency.

## Research status

- Established knowledge: analytic sphere and box SDF formulas and sign-based CSG combinations.
- Decision: retain analytic state; use CPU `f64` and explicit ideal-versus-numeric contracts for this slice.
- Hypothesis: a field-first model can support shared rendering and simulation queries at useful scale.
- Open questions: spatial indexing, robust conservative bounds, surface gradients, rule scheduling, persistence, cross-platform reproducibility, GPU lowering.
- Experimental findings: only local compilation and tests reported in this repository; no performance claim.
