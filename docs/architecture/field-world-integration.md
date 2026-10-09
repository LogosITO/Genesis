# Implicit-field world instances

**Status: partially implemented.** `WorldState` now owns exact-byte visual field
definitions and stable occurrences; runtime saves use version 7. GPU ray queries
and a two-instance fixture provide visual readback. Field graphs still have no
contact capability, and the main interactive viewport does not yet display them.
Existing authored capsule instances remain intact.

## Implemented boundary

- A definition stores validated exact JSON bytes and their SHA-256
  `DefinitionRevision`. The graph is compiled once per retained revision and
  shared by occurrences. The digest identifies bytes, not mathematical
  equivalence or an authenticated author.
- An occurrence has a stable `EntityId`, revision, enabled flag, generation,
  translation and positive uniform scale. Replacements validate first. Unused
  field revisions are removed when the final referencing occurrence changes or
  disappears. World-space samples and outward-derived bounds are read-only.
- Version-7 saves store exact source and occurrence records. Loading recompiles
  every graph, checks source revisions and semantics version, then validates all
  IDs, references, transforms and budgets before accepting the runtime. Older
  versions load with empty field collections. There is no field event in the
  deterministic tick scheduler yet.
- The separate graph GPU query maps rays into each visible occurrence's local
  frame and reports the nearest approximate hit with world ID. An uncertain or
  exhausted occurrence blocks a nearest-hit claim. The two-instance fixture
  assembles GPU ray readback into a PPM image. This is a research display path;
  it uploads graph buffers per occurrence and query chunk.

## Open gates

The main interactive viewport still needs an integrated field snapshot and
depth/identity output. The current GPU `f32` tracer cannot certify hit/miss or
surface normals, especially at tangencies and internal union zero faces.
Contact remains with the existing analytic `ContactScene`; general field
collision requires a separately justified numerical contract and independent
regressions. Cross-adapter parity, batched uploads, and performance at larger
instance counts have not been established.
