# Bounded implicit field graph v1

**Status: experimental / research stage.** This is an isolated, non-colliding CPU/GPU preview. It does not change `WorldState`, version-6 saves, authored capsule instances, The Passage, or the analytic direct/BVH renderer.

## Source and validation

The external representation is JSON with `format_version: 1`, a logical `id`, a `root` node ID, and `nodes`. IDs are 1–64 ASCII letters, digits, `_` or `-`; IDs must be unique. Each node has an `op`, an `id`, and exactly the parameters below. Unknown fields and operations are rejected. Connections use node IDs and may refer to nodes in any source order. Every node must be reachable from the root. Shared children are allowed; they are evaluated on each visit.

| `op` | Parameters | Ideal sample and contract |
| --- | --- | --- |
| `sphere` | `radius` | `length(p) - radius`; exact primitive SDF. |
| `box` | `half_extent: [x,y,z]` | Standard centered AABB signed distance; exact primitive SDF. |
| `translate` | `child`, `offset: [x,y,z]` | `child(p - offset)`; preserves properties. |
| `scale` | `child`, `factor` | `factor × child(p/factor)` for positive uniform scale; preserves properties. |
| `union` | `left`, `right` | `min(left(p), right(p))`; signed union and ideal 1-Lipschitz field, **not advertised as exact SDF**. |

The source is at most 16 KiB. At most 32 distinct nodes, depth 12, and 64 expanded node visits per sample are accepted. Invalid references, cycles, unreachable nodes, excessive work, invalid numeric ranges, nonfinite values, and unsupported versions fail before the graph is returned. Primitive sizes and scales are in `[0.01,100]`; each offset and box half-extent component has absolute value at most 1000, with positive box extents at least 0.01. Derived bounds must fit `[-10000,10000]` on each axis. `Source` and node variants deny unknown JSON fields. SHA-256 identifies exact source bytes, including whitespace. The logical ID is not a hash or path. No source code or shader is accepted.

Example files: `examples/field-graph/definitions/twin.json` and `examples/field-graph/definitions/plinth.json`. The same executable reads both.

## Mathematical semantics

### Occupied solid and its boundary (Foundation 0.12.1)

For a validated graph in ideal real arithmetic, the **occupied set** is the closed set
`S = {p ∈ R³ | F(p) ≤ 0}`. Its interior contains points with an occupied neighbourhood;
its exterior is the interior of the complement. The physical/topological boundary is
`∂S = closure(S) ∩ closure(R³ \ S)`. The **zero set** is `{p | F(p)=0}`. These notions are
different. A finite union of the supported nondegenerate spheres and boxes is regular
closed, so every boundary point has zero field value, but a zero point need not be a
boundary point. `classify_sample` reports only the observed `f64` sign and a
`BoundaryCandidate` near zero; it cannot certify interior, exterior, or `∂S`.

Counterexample: unit-half-extent boxes centred at `(-1,0,0)` and `(1,0,0)` share the
complete `x=0` face. Their union is `[-2,2] × [-1,1]²`. At the origin both branch
fields and their minimum equal zero. Yet the open ball of radius `0.5` at the origin
lies wholly in `S`; the origin is an **interior** zero point. A ray starting at this
point must not report an external surface contact. Coincident branches can also have
ties without introducing a new boundary. The format excludes degenerate primitives
(positive radii and half extents are required); it does not support empty graphs.

| Operation | Ideal field contract | Gradient and query limit |
| --- | --- | --- |
| Sphere, AABB | Exact SDF; 1-Lipschitz; closed bounded occupied solid | Sphere centre and box nonsmooth points have undefined analytic gradient. |
| Translation | Preserves child contract and Lipschitz constant | Arithmetic overflow yields an error. |
| Positive uniform scale `s F(p/s)` | Preserves exact SDF when the child is exact; preserves unit Lipschitz because `s × (distance/s) = distance` | Very small nested scales can lose `f32` precision or overflow intermediate values. |
| Union `min(A,B)` | Signed occupancy union and 1-Lipschitz if both children are; exact SDF is **not** propagated | Branch ties have undefined gradient; internal zero faces are possible. |

`Graph::distance_semantics()` exposes `ExactSdf` or `SignedUnitLipschitz` in ideal
arithmetic. The latter is a sign field with a distance **lower bound outside** (`F(p)
≤ distance(p,S)` when `F(p)>0`), not an exact signed distance. The compiler's bounds
enclose the ideal occupied set. Neither capability grants collision authority.

### Region estimates and restricted entry

`RegionAabb` accepts ordered finite corners within ±10,000 world units. For a region
with centre `c` and enclosing radius `r`, a unit-Lipschitz field obeys
`F(c)-r ≤ F(p) ≤ F(c)+r` for every point `p` in that region **in exact arithmetic**.
`estimate_region` evaluates this candidate bound and returns `Outside`, `Inside`, or
`Unknown`. It uses ordinary `f64` operations, without outward rounding or an error
budget. Its named exclusions are **ideal-model estimates only** and must not prune
authoritative geometry. Cancellation, underflow and overflow can invalidate the
computed sign; evaluation failure returns `Unknown`.

Foundation 0.12.2 adds **outward-rounded graph AABBs**. Primitive sphere/box bounds
start from their stored finite binary parameters. Each translation addition and
positive-scale multiplication rounds the lower result one `f64` representable value
down and the upper result one up; union takes componentwise extrema. Under IEEE-754
round-to-nearest with gradual underflow and no fast-math reassociation, this encloses
the ideal occupied geometry described by those stored parameters. Arithmetic
overflow rejects the graph. The compiler allows a 24-ULP admission margin at the
±10,000 world-bound ceiling so an exactly maximal scaled primitive remains loadable
after outward rounding; this is an input-domain policy, not a field-value error
bound. `exclude_region` returns `ConfirmedOutside` only if a
closed query AABB is strictly separated from those enlarged bounds along an axis;
otherwise it returns `Unknown`. This is a broad-phase exclusion, **not** an
evaluation of the field over the region and not evidence of complete occupancy.
No outward-rounded interval propagation through sphere/box field formulas exists.

`trace_exterior_entry` accepts a finite ray with `near ≥ 0`, `far > near`, normalized
direction, tolerance `[1e-5,0.1]`, safety `(0,1]`, 1–512 samples, and an explicit
expanded-node work budget up to one million. Its first sample must exceed tolerance.
A negative start returns `StartsInside`; a near-zero start returns `StartNearZero`.
It advances by `safety × F(p)` and returns `ApproximateEntry` when a later positive
sample is at most tolerance. Since an external ray meets an occupied union before it
can reach any interior ghost zero, this is a restricted **first entry** query only.
The ray's near/far endpoint coordinates are outward-rounded after multiplication and
addition. Only strict separation of this entire segment AABB from graph bounds
returns `BoundsMiss`. If an approximate field step passes `far` while those boxes
overlap, the result is `Indeterminate(UncertifiedAdvance)` rather than a Miss.
Work and sample caps return `Indeterminate(WorkBudget)` and `Exhausted`. A grazing
tangent may exhaust.
The result is not suitable for authoritative picking/contact without a certified
floating-point policy. Existing `trace` and WGSL tracing are older zero-set preview
queries and can classify an interior zero start differently.

CPU sampling is `f64`; the GPU interpreter is `f32`. The GPU diagnostic
`sample_points` reads raw finite field values and marks invalid arithmetic as `None`.
For samples farther than 0.001 from zero, the local shared-face test compares signs;
at near-zero points it compares values within 0.0001 world units without asserting
topological classification. These thresholds describe a tested workload, not a global
error bound. No general field-graph collision, interior-exit query, regularized CSG,
or certified field-value exclusion is provided. The narrow bounds exclusions above
refer to the CPU `f64` model; they do not certify GPU `f32` outcomes.

The coordinate system and world unit follow [coordinate-system.md](coordinate-system.md). Negative samples are inside the selected sign region, positive samples outside, and zero samples form a mathematical zero set. Union of the supported solids gives a signed union, but its sample can differ from Euclidean distance, especially inside overlaps. A pointwise minimum of 1-Lipschitz functions is 1-Lipschitz in ideal real arithmetic. Positive uniform scale with multiplied output preserves that bound. The finite AABB returned by the compiler is outward-rounded under the CPU assumptions above. This does not bound numerical errors in field samples.

CPU samples use `f64` and the existing `Sphere` and `AxisAlignedBox` formulas. Gradients use their analytic implementations and propagate through transforms; a union chooses the lower sampled branch. A tie, sphere center, or box nonsmooth point reports an undefined gradient. Branch selection itself can change under rounding. Overflow returns `MathError`; no NaN is returned as a valid sample. One primitive under any supported transforms advertises ideal exact SDF; any union does not. Signedness, exact SDF, Lipschitz bound, finite bounds, and collision authority are separate properties.

The graph intentionally excludes difference and intersection. Existing `Field::difference(A,A)` has zero samples on A's surface but no negative interior, demonstrating why an arbitrary CSG zero set cannot automatically be treated as a physical boundary.

## Numerical ray queries

CPU and GPU queries use explicit near/far distances, positive hit tolerance, safety fraction, and iteration cap. CPU `TraceOptions` accepts tolerance `[0.00001,0.1]`, safety `(0,1]`, and 1–512 iterations. GPU `GraphTracePolicy` has the same numeric ranges in `f32`. A ray direction is normalized. From an exterior point, a positive sample within tolerance is an **approximate hit**. The ideal step `sample/L × safety` passes far for an ideal-model miss. A negative sample, nonprogress, invalid arithmetic, or undefined GPU normal yields **uncertain**, with a typed diagnostic reason. Reaching the sample budget yields **exhausted**, never miss. Interior-start exits and exact tangencies are not solved in this tracer; the analytic primitive renderer remains available separately.

CPU uses `f64`; GPU interprets the validated node buffer in `f32` with a bounded 16-frame iterative stack and 256-operation guard per sample. No shader recursion, generated WGSL, mesh, voxel, or primitive-list lowering is used. The GPU normal is a finite difference at the approximate hit, not an analytic gradient. Red pixels signal uncertainty; yellow pixels signal budget exhaustion. Both remain visible diagnostics. Conversion to `f32`, cancellation at small relative scales, branch ties, and grazing rays can disagree with CPU. No formally certified floating-point hit/miss guarantee or cross-adapter bitwise identity exists.

The GPU preview accepts 1–2048 pixels per image dimension, 4096 query rays per call, and validates node storage against adapter limits. An image has `width × height × 4` output bytes plus row-aligned readback storage. Each frame currently uploads the small node buffer and allocates output/readback buffers; there is no reuse cache. At the 32-node ceiling the node payload is 1024 bytes, excluding GPU allocation overhead. Camera motion re-evaluates the graph; no camera-dependent field cache is retained.

## Integration boundary

This preview has no `EntityId`, contact, growth, persistence, or gameplay semantics. A later milestone must define occupancy/boundary behavior, conservative floating-point query policy, provenance, instance transforms, save migration, and collision authority before field graphs can enter `WorldState`. Reusing the exact-byte `DefinitionRevision` is provenance only. The present graph never replaces a world-owned authored capsule definition.
