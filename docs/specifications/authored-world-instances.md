# Authored world instances (Foundation 0.11)

**Status:** experimental, bounded CPU-authoritative slice. Authored structures are static analytic capsules, not ecological organisms.

## Definition, occurrence and derived geometry

`WorldState` stores up to four exact source-byte definitions and eight occurrences. A definition records SHA-256, compiler semantics version 1 and at most 65,536 original JSON bytes. Loading recompiles every distinct definition with the bounded `world-authoring` compiler and verifies its recorded digest and semantics before accepting the world. Equal byte revisions share one definition record and one in-memory compiled `Structure`; each occurrence has its own monotonic `EntityId`. At most 4,096 capsule segments may be allocated across occurrences. Definitions remain in the table after removing or replacing their last occurrence, so historical revisions consume one of the four slots until a new world is constructed.

An occurrence records its pinned definition revision, compiler semantics version, transform, enabled flag, solid flag and generation. A segment target consists of occurrence ID, occurrence generation, exact revision, compiler semantics version and derivation path. Renderer primitive indices and `Segment.id` are disposable. Removing, replacing, moving, scaling or disabling an occurrence invalidates old segment selections. Re-enabling does not revive them. An external file edit has no effect on existing occurrences. `replace_authored` validates the entire new source and transformed geometry before changing the occurrence; it preserves enabled and solid state, while cross-revision segment-state migration is unsupported.

## Transform and numerical domain

Only translation and positive uniform scale are supported. Rotation and nonuniform scale are unavailable; the API cannot silently approximate them. Source endpoints and radii are compiled in local authored coordinates. Each endpoint becomes `local × scale + translation`, and radius becomes `local radius × scale`, using checked finite `f64` arithmetic. Every resulting endpoint component must lie within ±9,000 world units and every radius in `[0.0001, 9000]`. These limits keep the geometry within the renderer's supported numeric domain; GPU conversion to `f32` is approximate. A failed transform, source compilation or budget check leaves `WorldState` unchanged. No cross-platform bitwise identity or watertight union of overlapping capsules is promised.

## Interaction and contact

An enabled occurrence contributes its capsules to `Scene::from_world` and CPU semantic picking. A solid enabled occurrence contributes the same transformed capsules to the static contact scene and its disposable BVH; non-solid or disabled occurrences do not block movement. Disabling, moving, replacing or removing a contacted occurrence clears the body's old contact record. Unchanged contact geometry reuses the existing BVH; changed geometry rebuilds it. Contact is moving-sphere versus individual capsules, not a regularized CSG union. Picking uses the existing CPU direct/BVH authority; GPU draw IDs are not semantic IDs. `PruneBranch` still applies only to ecological organisms.

`SetAuthoredEnabled` and `SetAuthoredTransform` are typed tick-indexed events. Runtime scheduling validates the current target and proposed transform. Simulation applies ordered events to a candidate world before committing a completed tick. A removed target fails; a changed generation makes a previously selected segment stale. Pending events persist and replay in insertion order. Future events against instances that are removed after scheduling are rejected when applied, leaving that tick unchanged.

## Persistence and GPU readiness

World-save envelope version 6 embeds the shared exact-byte definition table, occurrence records, current tick and pending events. It accepts validated versions 1–5; authored data or authored events mislabeled as an older version are rejected. Input is bounded to 2 MiB before JSON parsing. A failed `load_into` leaves the current runtime unchanged. The digest checks accidental drift or corruption relative to the saved digest, not authenticity against an attacker who can change both source and digest.

The authoritative CPU transaction and a GPU upload are separate operations. A renderer snapshot or GPU allocation can fail after a valid world change; the world remains consistent and the renderer reports the failure under its existing device-loss policy. The native viewport is a local demonstration, not a guarantee of successful presentation on every adapter. The renderer never silently drops authored capsules when constructing a `Scene`.
