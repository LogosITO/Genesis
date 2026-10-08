# Roadmap

## Foundation 0.1 — complete locally
CPU analytic sphere and box fields, signed CSG, minimal world and fixed-step growth, executable example, CI configuration, documentation, and static site. No performance or production claim.

## Foundation 0.2 — complete locally
Analytic gradients, ideal Lipschitz zero-set bounds, CSG counterexamples, bounded CPU ray queries for exact primitives, and property tests. A floating-point-certified conservative step and general CSG solid-boundary query remain open.

## Foundation 0.3 — First Light, experimental local prototype
Analytic sphere and AABB intersections in `wgpu` / WGSL, native viewport, fixed-step growth from `WorldState`, camera controls, normal debug, CPU reference tests, opt-in GPU readback, and explicit precision limits. A general field graph compiler and CSG renderer are outside this stage. Local validation is recorded in the [First Light benchmark](../research/first-light-benchmark.md); cross-platform behavior is not yet established.

## Candidate 0.4 — First Life
Design a bounded, persisted growth rule and one environment interaction. Before broadening GPU geometry, test numerical stability and scene scaling with reproducible fixtures, and decide whether a field compiler or a spatial index is justified by measurements.

## Later research
Growth grammars and L-systems, environment interactions, state persistence, destruction and recovery rules, physics queries, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
