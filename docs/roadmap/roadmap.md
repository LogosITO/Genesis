# Roadmap

## Foundation 0.1 — complete locally
CPU analytic sphere and box fields, signed CSG, minimal world and fixed-step growth, executable example, CI configuration, documentation, and static site. No performance or production claim.

## Foundation 0.2 — current
Analytic gradients, ideal Lipschitz zero-set bounds, CSG counterexamples, bounded CPU ray queries for exact primitives, and property tests. A floating-point-certified conservative step and general CSG solid-boundary query remain open.

## Candidate 0.3 — First Light
Lower a constrained primitive field graph to `wgpu` / WGSL and compare samples and hit decisions against the CPU reference on reproducible scenes. Add explicit numerical margins before claiming safe GPU tracing. Keep analytic state authoritative and GPU optional for core tests.

## Later research
Growth grammars and L-systems, environment interactions, state persistence, destruction and recovery rules, physics queries, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
