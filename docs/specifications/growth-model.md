# First Structure growth model (experimental)

## Established knowledge

Each organism is a rooted, ordered tree in `WorldState`. Node 0 is the root; every other node has one earlier parent ID, so cycles cannot form. IDs are allocation indices and are never reused. Nodes store world position and local maturity energy. The organism stores a shared budget. Limits are 4 organisms, 48 nodes each, 4 sources, and 512 ordinary spheres. These are world limits, independent of the renderer's 256-primitive snapshot limit.

## Decision and equations

At each fixed tick, ordered events first change resource sources. Each node with fewer than `max_children` samples the continuous concentration `C_i = Σ R_j(p_i)`. Local uptake is `g_i = C_i × uptake × Δt`. Node maturity becomes `min(threshold, energy_i + g_i)`. The shared budget gains `max_i(g_i)`, capped at `48 × threshold`; using the maximum keeps additional active nodes from multiplying one environmental supply. This is a toy allocation rule, not a physical conservation equation.

Mature requests are visited in ascending existing node ID. Each accepted request requires at least one threshold in the shared budget, consumes exactly one threshold, resets that parent's maturity to zero, and appends one child. At most one child per existing node per tick; newborn nodes wait until the next tick. Nodes with two children are saturated and receive zero uptake. At 48 nodes, no further child is allocated and accrued energy saturates. `max_children` is configurable as 1 or 2; the default is 2. Child IDs and parent links are stable across saves and replay. The seed is recorded but not consumed by this rule.

The child direction is normalized from `(0.4 parent_x + 0.35 resource_dx + divergence, 1, 0.4 parent_z + 0.35 resource_dz)`, where `resource_d` points from node to the strongest positive source and ties select earlier source insertion order. A second child uses divergence `-0.65` along X; a first child uses zero. With no positive source, resource direction is zero; zero uptake prevents new growth. The fixed positive Y term avoids zero vectors. The child position is `parent + segment_length × direction`. These constants define a deterministic stylized morphology, not botanical realism.

## Budgets and failure

World and simulation time are cloned for an atomic tick. Events apply in insertion order; organisms and nodes are processed in allocation order; sources are sampled in insertion order. A step evaluates at most 4 × 48 × 4 source samples and 512 legacy radius updates. Candidate positions must remain within ±1000 units. Invalid values, topology, or out-of-range positions abort the tick without changing authoritative state. `Organism::grow` itself also commits atomically. Determinism means the same serialized initial state, events, build, and floating-point execution model produce the same state; arbitrary cross-platform bitwise identity is unproven.

Rule parameters must be finite and positive: node radius and segment length ≤10 world units, threshold and uptake ≤1000. Each node renders as an analytic sphere. Every parent-child edge renders as an analytic capsule using authoritative endpoints and radius `0.55 × node_radius`. Overlapping primitives do not form a regularized watertight CSG union. A fully populated four-organism world needs 192 node spheres, 188 capsules, up to 4 source markers, plus ordinary spheres; it cannot fit the 256-primitive GPU snapshot. Overflow is an explicit error, never silent truncation.

## Open questions

Real resource transport, collision avoidance, branch orientation independent of world X, topology pruning, and scalable visibility require separate experiments. The current rule can place branches close together or overlapping; no biological or structural validity is claimed.
