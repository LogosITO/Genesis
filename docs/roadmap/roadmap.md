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

Dynamic bounded GPU primitive storage removes the former 256-object cap. The real 381-primitive four-organism world renders on the reference adapter; sampled CPU/GPU direct hits agree within defined tolerances. A deterministic CPU BVH and conservative primitive bounds are validated against CPU direct rays. GPU BVH traversal and camera culling remain unfinished research, so large-scene rendering still scales roughly with pixels × primitives. See [snapshot capacity](../specifications/render-snapshot.md), [acceleration design](../architecture/spatial-acceleration.md), and [local measurements](../research/first-scale-benchmark.md).

## Candidate 0.7

Test a bounded GPU BVH representation and traversal against CPU direct, CPU BVH, and GPU direct on identical rays and changing worlds. Enable it only if correctness and matched-scene measurements support it; keep direct traversal as a fallback. Investigate branch collision and shape continuity separately.

## Later research
Growth grammars and L-systems, finite resource budgets, destruction and recovery rules, physics queries, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
