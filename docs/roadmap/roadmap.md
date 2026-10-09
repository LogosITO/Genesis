# Roadmap

## Foundation 0.1 — complete locally
CPU analytic sphere and box fields, signed CSG, minimal world and fixed-step growth, executable example, CI configuration, documentation, and static site. No performance or production claim.

## Foundation 0.2 — complete locally
Analytic gradients, ideal Lipschitz zero-set bounds, CSG counterexamples, bounded CPU ray queries for exact primitives, and property tests. A floating-point-certified conservative step and general CSG solid-boundary query remain open.

## Foundation 0.3 — First Light, experimental local prototype
Analytic sphere and AABB intersections in `wgpu` / WGSL, native viewport, fixed-step growth from `WorldState`, camera controls, normal debug, CPU reference tests, opt-in GPU readback, and explicit precision limits. A general field graph compiler and CSG renderer are outside this stage. Local validation is recorded in the [First Light benchmark](../research/first-light-benchmark.md); cross-platform behavior is not yet established.

## Foundation 0.3.1 — Release governance, configured locally

Conventional Commit PR titles, independent Rust crate version rules, and a reviewable Release Please workflow. GitHub App credentials, branch and environment protection, hosted checks, tags, and releases remain to be configured or exercised on GitHub; see the [release policy](../architecture/releases.md).

## Foundation 0.4 — First Life, experimental local slice
A bounded rooted growth graph, continuous resource concentration, tick-indexed source events, version-1 JSON saves, exact local replay tests, three headless scenarios, and a state-driven native viewport. Cross-platform replay and visual quality remain unverified. This is an inspectable procedural model, not validated biology. See the [growth](../specifications/growth-model.md), [environment](../specifications/environment-fields.md), [persistence](../specifications/world-persistence.md), and [replay](../specifications/replay-determinism.md) specifications.

## Foundation 0.5 — First Structure, experimental local slice
Bounded two-child branching with shared resource allocation, analytic capsule connections, version-2 saves with version-1 read compatibility, CPU/GPU selected-ray parity, and controlled 1–48-node measurements. At this stage four fully populated organisms exceeded the then-current GPU snapshot cap. See the historical [measurement record](../research/first-structure-measurements.md).

## Foundation 0.6 — First Scale, experimental local slice

Dynamic bounded GPU primitive storage removes the former 256-object cap. The real 381-primitive four-organism world renders on the reference adapter; sampled CPU/GPU direct hits agree within defined tolerances. A deterministic CPU BVH and primitive bounds are validated against CPU direct rays. This stage used direct GPU traversal; see the historical [First Scale measurements](../research/first-scale-benchmark.md).

## Foundation 0.6.1 — GPU BVH, experimental local slice

The CPU-built hierarchy is flattened for bounded stackless WGSL traversal. A 16,861-ray four-way comparison and four pairs of real offscreen captures matched GPU direct and BVH on the reference adapter. Matched-scene timings support BVH for the tested larger scenes, while a single primitive is slower; automatic drawing retains direct traversal for up to four primitives. Floating-point bounds are not formally certified and other adapters remain untested. See the [design](../architecture/spatial-acceleration.md) and [local experiment](../research/gpu-bvh-acceleration.md).

## Foundation 0.7 — First Interaction, experimental local slice

CPU analytic picking maps snapshot primitives to stable world targets. A tick-indexed `PruneBranch` event removes an authoritative subtree and changes later growth. Version-3 saves retain sparse IDs and pending cuts; version-1/2 saves migrate on load. Headless pruning and save/replay scenarios, GPU before/after readback, and local latency measurements are recorded in the [interaction contract](../specifications/first-interaction.md) and [measurement record](../research/first-interaction-measurements.md). Native click and key controls are implemented; human interaction across operating systems still needs wider validation.

## Foundation 0.7.1 — First Contact, experimental local slice

One persisted kinematic sphere, tick-indexed velocity, analytic swept contact against spheres and capsules, CPU-only collision BVH, pruning-aware movement, version-4 saves, GPU body readback, and repeated prune/regrowth tests. See the [contact contract](../specifications/kinematic-contact.md) and [local measurements](../research/first-contact-measurements.md). AABB sweep, exact union contact, sliding, and cross-platform bitwise replay remain open.

## Foundation 0.8 candidate

Investigate a shared CPU spatial-index contract or cached contact BVH to avoid rebuilding unchanged geometry, then test contact normals and penetration bounds near seams on Windows and Linux. Compare conservative collision against independent geometric references before adding more collider classes.

## Later research
Growth grammars and L-systems, finite resource budgets, destruction and recovery rules, broader physical interaction, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
