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

The coordinate system and world unit follow [coordinate-system.md](coordinate-system.md). Negative samples are inside the selected sign region, positive samples outside, and zero samples form a mathematical zero set. Union of the supported solids gives a signed union, but its sample can differ from Euclidean distance, especially inside overlaps. A pointwise minimum of 1-Lipschitz functions is 1-Lipschitz in ideal real arithmetic. Positive uniform scale with multiplied output preserves that bound. The finite AABB returned by the compiler conservatively contains the nonpositive region in ideal arithmetic. These are mathematical claims, not floating-point interval proofs.

CPU samples use `f64` and the existing `Sphere` and `AxisAlignedBox` formulas. Gradients use their analytic implementations and propagate through transforms; a union chooses the lower sampled branch. A tie, sphere center, or box nonsmooth point reports an undefined gradient. Branch selection itself can change under rounding. Overflow returns `MathError`; no NaN is returned as a valid sample. One primitive under any supported transforms advertises ideal exact SDF; any union does not. Signedness, exact SDF, Lipschitz bound, finite bounds, and collision authority are separate properties.

The graph intentionally excludes difference and intersection. Existing `Field::difference(A,A)` has zero samples on A's surface but no negative interior, demonstrating why an arbitrary CSG zero set cannot automatically be treated as a physical boundary.

## Numerical ray queries

CPU and GPU queries use explicit near/far distances, positive hit tolerance, safety fraction, and iteration cap. CPU `TraceOptions` accepts tolerance `[0.00001,0.1]`, safety `(0,1]`, and 1–512 iterations. GPU `GraphTracePolicy` has the same numeric ranges in `f32`. A ray direction is normalized. From an exterior point, a positive sample within tolerance is an **approximate hit**. The ideal step `sample/L × safety` passes far for an ideal-model miss. A negative sample, nonprogress, invalid arithmetic, or undefined GPU normal yields **uncertain**, with a typed diagnostic reason. Reaching the sample budget yields **exhausted**, never miss. Interior-start exits and exact tangencies are not solved in this tracer; the analytic primitive renderer remains available separately.

CPU uses `f64`; GPU interprets the validated node buffer in `f32` with a bounded 16-frame iterative stack and 256-operation guard per sample. No shader recursion, generated WGSL, mesh, voxel, or primitive-list lowering is used. The GPU normal is a finite difference at the approximate hit, not an analytic gradient. Red pixels signal uncertainty; yellow pixels signal budget exhaustion. Both remain visible diagnostics. Conversion to `f32`, cancellation at small relative scales, branch ties, and grazing rays can disagree with CPU. No formally certified floating-point hit/miss guarantee or cross-adapter bitwise identity exists.

The GPU preview accepts 1–2048 pixels per image dimension, 4096 query rays per call, and validates node storage against adapter limits. An image has `width × height × 4` output bytes plus row-aligned readback storage. Each frame currently uploads the small node buffer and allocates output/readback buffers; there is no reuse cache. At the 32-node ceiling the node payload is 1024 bytes, excluding GPU allocation overhead. Camera motion re-evaluates the graph; no camera-dependent field cache is retained.

## Integration boundary

This preview has no `EntityId`, contact, growth, persistence, or gameplay semantics. A later milestone must define occupancy/boundary behavior, conservative floating-point query policy, provenance, instance transforms, save migration, and collision authority before field graphs can enter `WorldState`. Reusing the exact-byte `DefinitionRevision` is provenance only. The present graph never replaces a world-owned authored capsule definition.
