# First Life environment field

## Historical unlimited-source decision

A source has a stable world entity ID, centre `s`, positive influence radius `r ≤ 100` world units, peak strength `q ∈ [0,100]`, and an active flag. At finite point `p`, its concentration is

`R(p) = q × max(0, 1 - ||p - s|| / r)` when active, and `0` when inactive.

The value is continuous, nonnegative, compactly supported, and dimensionless in this model. The sum of up to four source values is sampled at the tip before each proposed growth update. `uptake` converts concentration to model energy per simulated second. Sampling does not deplete a source. There is no finite supply, diffusion, exchange, or conservation claim.

Source positions must stay in ±1000 world units. Move and active-state changes are typed, tick-indexed events. Moves are checked against the same bound before scheduling and before application. Distances use checked `f64` vector operations. A zero or tiny radius is rejected only at zero; very small positive radii are valid but may produce no sampled concentration at ordinary positions.

## Limitations and hypothesis

This remains the supported unlimited-source mode for earlier saves and examples. Foundation 0.8 adds a finite reservoir and proportional allocation while preserving this analytic influence function; see the [resource allocation contract](resource-allocation.md). The field is not the signed geometry field API or a physical energy model. The baseline, changed, and limited scenarios test reproducible topology changes locally; they do not validate biology.
