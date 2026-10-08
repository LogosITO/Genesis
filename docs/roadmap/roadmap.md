# Roadmap

## Foundation 0.1 — current
CPU analytic sphere and box fields, signed CSG, minimal world and fixed-step growth, executable example, CI, documentation, and static site. No performance or production claim.

## Candidate 0.2 — measured field queries
Add gradients and a conservative distance-bound contract, then compare CPU traversal strategies using reproducible scenes. Define a conformance corpus with ambiguous CSG boundaries. This stage depends on numerical evidence.

## Candidate 0.3 — rendering experiment
Lower a constrained field graph to `wgpu` / WGSL and compare it against CPU reference samples. Keep analytic state authoritative. GPU support remains optional for core tests.

## Later research
Growth grammars and L-systems, environment interactions, state persistence, destruction and recovery rules, physics queries, embedding/FFI, standalone UI, and authored language. Sequence and scope will follow experiments, not promises.
