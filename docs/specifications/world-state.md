# World state

`WorldState` stores analytic growing spheres in insertion order. `EntityId` is a monotonically assigned `u64`, stable for the world's lifetime and never reused. Removal, persistence, component systems, and general object types are not implemented. Each entity owns a `Sphere`, `Transform`, and nonnegative finite growth rate. Invalid insertion leaves the world unchanged. `DeterministicSeed` is recorded in state; the current growth rule does not consume randomness, so changing the seed does not change outcomes.

The world exposes read-only entity access. A simulation step computes and validates every new radius before applying any, so failure leaves the world unchanged. `apply_radii` checks count to avoid partial updates. This is a minimal state model, not a general ECS.
