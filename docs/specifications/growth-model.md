# First Life growth model (experimental)

## Established knowledge

The world stores a rooted, ordered graph per organism. Each node has a stable local integer ID, a parent ID, a finite world position, and an energy accumulator. The root is node 0. In this version the graph is a single chain: node `i > 0` has parent `i - 1`. Each node is rendered as an analytic sphere; the graph, not the spheres or GPU buffer, is authoritative.

## Decision and equations

For the current tip at position `p`, source concentrations are summed as `C(p) = Σ R_j(p)`. During a fixed step of duration `Δt`, the tip gains `G = C(p) × uptake × Δt` model energy units. The accumulator becomes `min(threshold, old_energy + G)`. At or above the threshold, if capacity permits, the tip spends its entire threshold budget and allocates exactly one child. Surplus inflow within that tick is discarded. This is a bounded toy rule, not a conservation law.

The child direction is the normalized vector `(source_x - tip_x, 1, source_z - tip_z)` for the source with the largest *positive* concentration at the tip. Equal concentrations choose the source with the lower insertion index. With no contributing source, direction is `(0,1,0)`, but zero inflow prevents growth. The child centre is `tip + segment_length × direction`. The positive Y component keeps the chain advancing upward, including when the source coincides with the tip. Directional source response is horizontal; the source's Y coordinate still affects concentration.

Only the current tip accumulates energy. Earlier nodes retain zero. Each organism proposes at most one new node per tick. Allocation order fixes node IDs; IDs are never reused. No randomness is used. The recorded seed is reserved for future seeded rules.

## Budgets and failure

Maximum: 4 organisms, 48 nodes each, 4 sources, and 32 ordinary spheres per world. The renderer's hard limit is 256 primitives; these world maxima produce at most 228. One step evaluates at most 4 × 4 source samples, 4 growth proposals, and 32 legacy radius updates. At node capacity, the current tip's energy saturates at the threshold and no node is silently dropped. A proposed position outside ±1000 world units, nonfinite arithmetic, or a legacy sphere radius overflow fails the entire step atomically.

Rule parameters must be finite and positive: node radius and segment length ≤10 world units, threshold and uptake ≤1000. The rule permits multiple organisms but no removal, branches, collision, resource competition, or regeneration yet. Its output does not imply biological validity or emergence.

## Open questions

How should multiple tips compete for a finite budget? What constraints should prevent analytic spheres from visually overlapping? A later branching rule needs an explicit allocation and event order before it can claim reproducibility.
