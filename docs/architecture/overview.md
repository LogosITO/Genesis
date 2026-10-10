# Architecture overview

Foundation 0.12 introduced bounded Field Graph authoring: exact-byte JSON source → validated typed nodes → CPU `f64` sampling and bounds / WGSL `f32` interpretation. Later integration added world-owned visual field instances and a native mixed viewport. Field Graphs still do not participate in physical contact. See the [field-graph specification](../specifications/field-graph.md), [native mixed viewport](../research/native-mixed-viewport.md), and [World Lab scene format](../specifications/world-lab.md).

The native mixed renderer can derive a revision-scoped WGSL evaluator for a supported single-definition scene. Its device-local bounded cache and interpreter fallback do not change `WorldState`; see [Performance Research 2](../research/performance-research-2.md) for the conditional qualification and numerical limits.

An opt-in structural evaluator can reuse one GPU program across parameter revisions and definitions with identical operation links. Heterogeneous topologies still use the interpreter; see [Performance Research 3](../research/performance-research-3.md).

Foundation 0.11 adds bounded authored definitions and world occurrences. Exact source bytes and stable occurrence IDs live in `WorldState`; compiled segments, rendering snapshots and contact BVHs are derived. See the [authored instance contract](../specifications/authored-world-instances.md) and [ADR 0009](../adr/0009-authored-world-instances.md).

Foundation 0.8 adds finite source reservoirs and proportional allocation among existing organisms. Source ledgers, growth topology, kinematic contact, and pending events persist in version-5 saves. The renderer only reads the resulting analytic world. See the [allocation contract](../specifications/resource-allocation.md), [ecosystem state](../specifications/ecosystem-state.md), and [contact specification](../specifications/kinematic-contact.md).

**Status: experimental / research stage.** Foundation 0.1 established analytic geometry, mutable state, fixed-step growth, and an embeddable coordinator. Foundation 0.2 added gradients, ideal Lipschitz zero-set bounds, and bounded CPU primitive ray queries. Foundation 0.3 added an optional native GPU prototype for two analytic primitives. Foundation 0.4 adds a bounded mathematical growth graph and continuous resource environment. It is not a complete engine.

The source of truth is analytic parameters and rules. Sampling at a point is a query, not a stored mesh or voxel model. The current renderer produces pixels and textures without changing this source of truth; future acceleration structures may do the same.

Current flow: `Vec3 / Transform` → `Sphere / AxisAlignedBox / Field` → `WorldState` → `advance` / `advance_life` → `Runtime`. `WorldState` owns ordinary spheres, growth nodes, and resource sources. `world-simulation` applies events and growth on a candidate state before atomic commit. `world-runtime` coordinates ticks, pending events, and bounded versioned saves. The renderer takes a bounded snapshot of world spheres, growth nodes, and sources, sends analytic parameters to WGSL, and computes each pixel by ray intersection. A full-screen triangle only presents the computed image; it is not world geometry. The original `hello-world` and First Light modes remain. See [renderer architecture](renderer.md).

Future modules are conditional on experiments: physics with compatible field contracts, a richer language for authored rules, a full editor, and an FFI boundary. The experimental World Lab is a small external-scene viewer, not a full editor. The runtime remains free of UI and GPU dependencies.

## Research status

- Established knowledge: analytic sphere and box SDF formulas and sign-based CSG combinations.
- Decision: retain analytic state; use CPU `f64`, bounded GPU `f32` snapshots, and analytic sphere/AABB intersections.
- Hypothesis: a field-first model can support shared rendering and simulation queries at useful scale.
- Open questions: reusable spatial indexing, robust conservative bounds, resource transport, cross-platform reproducibility, GPU field lowering, and useful branching rules.
- Experimental finding: one finite source produced 16 nodes for one organism and 8 each for two competitors after 40 local fixed ticks; see the [ecology experiment](../research/first-ecology-experiment.md). This does not validate biology.
- Experimental finding: the three First Life headless scenarios produce distinct node counts and tip positions; see the [growth model](../specifications/growth-model.md) and [measurement record](../research/first-life-measurements.md).
- Experimental findings: one native Vulkan adapter passed the local parity and resolution smoke tests; see the [benchmark record](../research/first-light-benchmark.md). No general performance claim.
