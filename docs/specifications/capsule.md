# Analytic capsule (experimental)

## Mathematical contract

A capsule is the closed set of points within radius `r > 0` of segment `AB`. For point `P`, let `d = B - A`, `t = clamp(dot(P - A, d) / dot(d, d), 0, 1)`, and `q = A + t d`. Its ideal signed distance is `length(P - q) - r`: negative inside, zero on the boundary, positive outside. If `A = B`, set `q = A`; this is a sphere. This primitive implements `ExactSdf` in the ideal real-arithmetic sense, unlike arbitrary CSG combinations. Evaluated `f64` samples are not rounding-certified.

The analytic gradient, where unique, is `normalize(P - q)`. Points on the segment axis return `GradientIssue::CapsuleAxis`. At the side-to-cap seam the outward gradient agrees in exact arithmetic. Axis-aligned bounds are `min(A,B) - r` and `max(A,B) + r` componentwise.

The CPU renderer intersects the finite cylinder side and both hemispherical caps with quadratic roots and returns the earliest valid crossing in a closed ray interval. An interior ray returns the positive exit; a tangent counts as contact. The WGSL renderer evaluates the same pieces using `f32`. Discriminants, very short segments, tangencies, nearly parallel rays, and seam ties can differ numerically between CPU and GPU. Tested rays are evidence for those cases, not a universal robustness certificate.

## Supported domain

`Capsule::new` accepts finite endpoints of magnitude at most 10,000 world units, radius in `[1e-12, 10,000]`, and equal or at least `1e-12` separated endpoints. CPU field queries are restricted to points of magnitude at most 10,000. Renderer snapshots additionally require radius at least `0.0001` for useful `f32` representation; its endpoint coordinates must be within ±10,000. The large accepted coordinate range is a validation bound, not a promise that tiny features remain distinguishable near its edge.

Growth connections use these capsules as separate primitives, with radius `0.55 × node_radius`. Overlap with node spheres is intentional for visualization; the renderer selects the nearest primitive hit. It does not trace a regularized or watertight union surface, and the per-primitive ID identifies either a node or a connection.
