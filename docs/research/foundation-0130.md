# Foundation 0.13.0: world-owned visual field occurrences

**Status: partial / experimental.** `WorldState` owns at most four validated
exact-byte field definitions and eight visual occurrences. Occurrences have
monotonic world IDs, a revision, enabled state, generation and validated positive
uniform transform. One compiled graph is shared by occurrences of the same
revision. Mutations validate a proposed result before committing. The world
offers read-only world-unit sampling and outward-derived bounds. Save format 7
records source and occurrence state; loading recompiles definitions and rejects
invalid revisions, versions, transforms and dangling references.

The renderer's GPU diagnostic query maps each ray into each enabled occurrence's
local frame, reads actual WGSL results and chooses the nearest displayed hit.
Any uncertain or exhausted occurrence prevents a claimed nearest result. Exact
distance ties retain allocation order. GPU `Miss` and `Hit` remain preview-only;
they are not certified by the CPU numerical bounds contract. `ContactScene`
continues to see zero colliders in a world containing only field occurrences.

The `field-graph-preview --world-fixture` executable created two distinct
world-owned instances and wrote a 128×128 capture from GPU ray readback. On the
local NVIDIA GeForce RTX 4070 SUPER, it reported 3,877 hit pixels, 12,497 miss
pixels, 10 exhausted pixels, zero uncertain pixels, and 23.83 ms from first
query submission through readback of four bounded ray chunks. The actual image
was inspected: both analytic objects are visible. The result is a local debug
measurement, not a steady-state GPU benchmark. It includes repeated uploads,
readbacks, and CPU color composition; no GPU compute timestamp was taken.

This slice does not render instances in The Passage or the main viewport. It
does not add field picking, contact, growth or environment interaction. GPU
`f32` near-surface parity across adapters and the cost of repeated graph uploads
remain open. A batched instance buffer and integrated depth/identity output are
needed before treating this as a scalable gameplay renderer.
