# ADR 0008: Content-addressed authored structures

## Status

Accepted for static authoring previews in Foundation 0.10.1. Mutable world integration remains a proposal.

## Context

Author-supplied `id` and numeric `revision` do not prove which JSON bytes produced a structure. A reload can renumber draw IDs, and a save that refers only to a path can silently change meaning. The existing world save must remain untouched.

## Decision

Hash exact loaded source bytes with SHA-256 and store the digest with the compiled structure. Record an explicit compiler semantics version. Give every emitted segment a bounded derivation path consisting of its axiom position and replacement-child positions. Scope that path by digest and semantics version. Keep one-based draw IDs only for renderer snapshots.

Use a small self-contained authoring snapshot containing format version, semantics version, digest and exact source bytes. Verify the digest, then validate and recompile. A preview reload compares exact bytes, prepares a complete new structure, scene and CPU BVH, then swaps state and invalidates selection. Unchanged bytes are a no-op.

## Alternatives

- Canonical JSON hashing: rejected because canonicalization rules and numeric spellings need their own specification.
- Author labels or file paths as revisions: rejected because they permit silent content drift.
- Generated coordinate or draw-index identity: rejected because repeated geometry and unrelated insertion can alias.
- Cross-revision path matching: deferred because path equality alone does not prove semantic continuity.
- A content-addressed external store: deferred until distribution and availability requirements are known.

## Consequences

Whitespace changes intentionally produce a new revision and clear selection. Snapshot byte arrays add size but avoid an external content store. SHA-256 is an integrity check, not a signature or a substitute for validation. Derivation paths are unique under current bounds but may be invalidated by a compiler semantics change. GPU allocation and device failure remain governed by the renderer after the CPU transaction; they cannot be promised as an atomic CPU/GPU commit. The current `WorldState` format and gameplay stay unchanged.
