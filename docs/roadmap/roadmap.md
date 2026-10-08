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

## Candidate 0.5
Measure many-organism scaling and visual legibility, then choose one grounded extension: bounded branching with explicit tip allocation, or analytic connection primitives with CPU/GPU parity. The current single-chain rule and point-like sphere rendering should be evaluated before a field compiler or spatial index is introduced.

## Later research
Growth grammars and L-systems, finite resource budgets, destruction and recovery rules, physics queries, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
