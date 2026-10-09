# First Structure growth model (experimental)

## Established knowledge

Each organism is a rooted, ordered tree in `WorldState`. Node 0 is the root; every other node has one earlier parent ID, so cycles cannot form. Active nodes remain in allocation order, but pruning leaves gaps: IDs are stable allocation numbers, not vector indices, and are never reused. Nodes store world position and local maturity energy. The organism stores a shared budget and a `next_node_id` allocator. Limits are 4 organisms, 48 active nodes each, 1024 lifetime node allocations per organism, 4 sources, and 512 ordinary spheres. These are world limits, independent of the renderer's [dynamic GPU storage budget](render-snapshot.md).

## Decision and equations

At each fixed tick, ordered events first change resource sources. Each node with fewer than `max_children` samples the continuous concentration `C_i = Σ R_j(p_i)`. Local uptake is `g_i = C_i × uptake × Δt`. Node maturity becomes `min(threshold, energy_i + g_i)`. The shared budget gains `max_i(g_i)`, capped at `48 × threshold`; using the maximum keeps additional active nodes from multiplying one environmental supply. This is a toy allocation rule, not a physical conservation equation.

Mature requests are visited in ascending existing node ID. Each accepted request requires at least one threshold in the shared budget, consumes exactly one threshold, resets that parent's maturity to zero, and appends one child with a fresh ID. At most one child per existing node per tick; newborn nodes wait until the next tick. Nodes with two children are saturated and receive zero uptake. At 48 active nodes, no further child is allocated and accrued energy saturates. Exhausting 1024 lifetime IDs is an explicit error. `max_children` is configurable as 1 or 2; the default is 2. Child IDs and parent links are stable across saves and replay. The seed is recorded but not consumed by this rule.

The child direction is normalized from `(0.4 parent_x + 0.35 resource_dx + divergence, 1, 0.4 parent_z + 0.35 resource_dz)`, where `resource_d` points from node to the strongest positive source and ties select earlier source insertion order. A second child uses divergence `-0.65` along X; a first child uses zero. With no positive source, resource direction is zero; zero uptake prevents new growth. The fixed positive Y term avoids zero vectors. The child position is `parent + segment_length × direction`. These constants define a deterministic stylized morphology, not botanical realism.

## Budgets and failure

World and simulation time are cloned for an atomic tick. Events apply in insertion order; organisms and nodes are processed in allocation order; sources are sampled in insertion order. A step evaluates at most 4 × 48 × 4 source samples and 512 legacy radius updates. Candidate positions must remain within ±1000 units. Invalid values, topology, or out-of-range positions abort the tick without changing authoritative state. `Organism::grow` itself also commits atomically. Determinism means the same serialized initial state, events, build, and floating-point execution model produce the same state; arbitrary cross-platform bitwise identity is unproven.

Rule parameters must be finite and positive: node radius and segment length ≤10 world units, threshold and uptake ≤1000. Each node renders as an analytic sphere. Every parent-child edge renders as an analytic capsule using authoritative endpoints and radius `0.55 × node_radius`. Overlapping primitives do not form a regularized watertight CSG union. A fully populated four-organism world needs 192 node spheres, 188 capsules, up to 4 source markers, plus ordinary spheres. The tested 381-primitive scene uses one source and renders completely on the reference adapter. Scenes above the configured GPU budget fail explicitly, without truncation.

## Open questions

Foundation 0.8 retains this structural growth rule and adds [finite proportional source allocation](resource-allocation.md). In finite mode, shared spendable budget gains exactly the units allocated to that organism; local node energy is a maturity signal and is not summed as conserved energy. The unlimited-source mode above remains available for older saves and examples. Real resource transport, collision avoidance, branch orientation independent of world X, and scalable visibility require separate experiments. Foundation 0.7 adds [branch pruning](first-interaction.md); the current rule can still place branches close together or overlapping, and no biological or structural validity is claimed.
