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
Bounded two-child branching with shared resource allocation, analytic capsule connections, version-2 saves with version-1 read compatibility, CPU/GPU selected-ray parity, and controlled 1–48-node measurements. Four fully populated organisms overflow the current GPU snapshot explicitly. Visual legibility, cross-adapter numerical behavior, and large-world rendering remain unverified; see the [growth model](../specifications/growth-model.md), [capsule contract](../specifications/capsule.md), and [measurement record](../research/first-structure-measurements.md).

## Candidate 0.6
Address measured snapshot and intersection scaling without weakening world-state authority: test a bounded visibility or acceleration strategy on a defined scene class, then investigate branch collision and shape continuity separately. Do not infer a watertight CSG union from overlapping spheres and capsules.

## Later research
Growth grammars and L-systems, finite resource budgets, destruction and recovery rules, physics queries, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
