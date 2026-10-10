# ADR 0010: bounded smooth union for visual field graphs

## Context

Version-one Field Graph definitions only allowed hard union. A smooth blend
changes the occupied set continuously, but the existing graph publishes
finite bounds and a unit-Lipschitz distance capability. Arbitrary displacement
would invalidate those contracts without additional propagation.

## Decision

Add `smooth_union` with a finite width in `[0.01,100]`, restricted to two
exact-SDF child branches. Use the polynomial smooth minimum in the CPU and
WGSL interpreters. The ideal field is signed and 1-Lipschitz, but is not an
exact SDF. Expand the union of child bounds by an outward-rounded `width/4`.
Keep the existing source format version because old definitions retain their
meaning. Restricted solid entry and sweep queries report unsupported for
graphs containing this operation, except a strict sweep bounds exclusion.

## Alternatives

- Allow smooth unions of arbitrary signed fields: rejected because their
  occupied bounds cannot be derived from the current child bounds alone.
- Add sinusoidal displacement first: deferred because its Lipschitz constant
  exceeds one and a finite occupied bound requires separate derivation.
- Keep only hard unions: retains the simpler contract but prevents a useful
  continuously controlled surface form.

## Consequences

Definitions can produce a smooth bridge without mesh conversion. The GPU
still evaluates the fixed bounded interpreter and uploads per call. Source
validation rejects nested smooth unions and hard-union children. Floating-point
samples and ray outcomes remain approximate; no contact authority is added.

## Status

Accepted for the experimental visual Field Graph path. CPU and one native
Vulkan GPU were exercised locally; cross-adapter behavior is unverified.
