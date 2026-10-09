# Proposed implicit-field world instances (future milestone)

**Status: design proposal, not implemented.** `WorldState` remains the sole authority;
the current field graph is an immutable preview outside the world and has no contact
capability. Existing authored capsule instances and version-6 saves remain intact.

1. A definition record would retain logical ID, exact-byte `DefinitionRevision`,
   validated source bytes, and a content revision. The revision is provenance; it
   does not prove mathematical equivalence of differently formatted source.
2. A world occurrence would receive the normal stable `EntityId` and a validated
   translation/positive uniform scale. Its identity would survive definition edits.
   Editing creates a new revision and explicitly updates selected occurrences;
   cached bounds, CPU queries and GPU buffers would then be invalidated together.
3. The occurrence would declare occupied-set semantics `F≤0` and capability
   `SignedUnitLipschitz` or `ExactSdf`, never infer a physical boundary from every
   zero point. Bounds would transform the compiler's finite AABB and expand for
   any supported body radius. An acceleration structure could reject candidates
   only with a numerically sound bound or with a verified analytic shape bound.
4. Selection and picking would return a world identity plus an explicit
   approximate/indeterminate status. A renderer diagnostic cannot become an
   authoritative contact result. Contact capability should initially be limited
   to a proven subset with independently checked normal and tie rules; other
   graphs remain display-only. CPU decides gameplay; GPU readback checks parity.
5. Save/load would persist source bytes, revision, occurrence identity, transform,
   and a versioned capability policy. A mismatched hash or unsupported graph would
   fail closed before mutating the world. A format migration needs its own tests;
   this research API does not change persistence version.

Open gates: certified numerical exclusion or a conservative failure policy,
interior and tangent behaviour, identities at overlapping boundaries, and
cross-adapter sampling differences. Until these gates are met, `WorldState` owns
only its existing analytic shapes and authored capsule instances.
