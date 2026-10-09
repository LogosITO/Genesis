# ADR 0009: Authored definitions and world occurrences

## Status

Accepted for Foundation 0.11 experimental scope.

## Context

The v1 structural grammar previously produced static previews but had no authoritative world occurrence. A file path or renderer draw index cannot provide stable world identity, and duplicating source bytes for every occurrence would waste save space.

## Decision

`world-state` depends on CPU-only `world-authoring`. `WorldState` holds a bounded table of exact-byte, digest-checked definitions and a separate bounded list of mutable occurrences. Compiled `Structure` values are derived, shared by revision in memory, omitted from saves and rebuilt on load. Each occurrence has a monotonic world ID and generation. Renderer and contact snapshots derive their capsules from the same validated transformed segments. Runtime events mutate a candidate world at a fixed tick. Save version 6 embeds the source table.

## Alternatives

- Store definitions only by external file path: rejected because file changes would silently alter replay.
- Embed source bytes in each occurrence: rejected because repeated occurrences duplicate data.
- Convert generated segments into organisms: rejected because authored structures have no growth or resource state.
- Add a new generic scene graph or ECS: deferred; one bounded occurrence list covers this milestone.
- Add rotation now: deferred until a tested transform contract exists across contact, renderer and persistence.

## Consequences

The dependency graph remains acyclic and CPU simulation has no GPU requirement. Definitions consume bounded memory even when no occurrence references them. Transforming an occurrence re-derives world-space capsules, while grammar compilation is shared per revision and is not repeated per frame. Segment selections become stale after geometry or participation changes. GPU allocation and presentation remain separate from authoritative CPU commit. Cross-revision mutable segment migration, regularized capsule union and unlimited geometry remain unsupported.
