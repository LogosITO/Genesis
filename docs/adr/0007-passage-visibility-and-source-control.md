# ADR 0007 — Passage visibility and source control

## Status

Accepted for the experimental Passage slice.

## Context

The central stem can be partially hidden behind analytic wall spheres. A green branch that is small on screen is hard to recognize, while selecting through a wall would make pruning ambiguous. Growth already responds to finite source concentration, but the player cannot directly pause that input.

## Decision

Keep the CPU nearest-surface pick authoritative for selection and preserve existing contact rules. Mark the central stem's existing node and connection as emissive-looking in the renderer snapshot; selected branches use yellow. Classify an occluded stem along a click ray only to explain why the front target was selected. An unmapped visible goal marker blocks semantic picking instead of permitting a click through it. Add a tick-indexed source activity switch using the existing `SetSourceActive` event. All changes to growth remain in `WorldState` through the fixed-step simulation.

## Alternatives

- A silhouette or through-wall outline would expose hidden geometry, but needs an additional pass or depth policy and may imply an interaction that is forbidden by nearest-surface picking. Deferred until user testing shows the existing visible accent is insufficient.
- Enlarging the central stem would change the collision lane and existing playthrough contract. Rejected for this stage.
- Renderer-side source animation would not change authoritative growth. Rejected.

## Consequences

Visible stem pixels are brighter without changing their geometry, identity, or selection rules. Occluded parts remain hidden. The HUD can state when another surface covers the stem and whether resource input is active. The source switch is replayable and obeys pause and restart behavior. There is one additional CPU occlusion query on a successful Passage click. CPU `f64` and GPU `f32` agreement at nearly coincident surfaces is still not guaranteed; the CPU ambiguity policy fails closed.
