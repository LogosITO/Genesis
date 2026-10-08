# Analytic gradients

`AnalyticGradient::gradient` returns `Defined(Vec3)` or `Undefined(reason)`. Numeric overflow still returns `MathError`. A defined gradient is the derivative of the scalar expression at the sampled point, not automatically a physical surface normal.

| Field | Defined derivative | Explicitly undefined |
| --- | --- | --- |
| Sphere `||p||-r` | `p/||p||` | Centre `p=0` (`SphereCenter`). |
| Axis-aligned box | Outside: normalized vector of positive `abs(p)-h` components with coordinate signs. Inside or smooth face: sign of the unique active axis. | Edge/corner ties, medial-axis ties, or an active axis at coordinate zero (`BoxNonsmooth`). |
| `min` / `max` CSG | Gradient of the uniquely active branch. Difference negates the second branch gradient when that branch wins. | Equal sampled branch values (`BranchTie`), even if identical inputs happen to share a derivative. |
| Positive uniform transform | Child gradient at local coordinate `(p-t)/s`; the factors `s` and `1/s` cancel. Transformed spheres use the world-space direction `p-t` to avoid an unnecessary division. | Child undefined status or numeric transform failure. |

The implementation compares sampled branch values exactly as `f64`; it applies no hidden seam tolerance. Roundoff may change the selected branch near a tie. Vector normalization scales components before taking a length, avoiding overflow for very large finite directions, but its result still has ordinary floating point error. Finite differences are used only in reference tests away from singularities; they are not silently substituted for an undefined analytic derivative.

At a verified smooth boundary, a unit gradient can be interpreted as an outward normal. The present CSG API cannot certify that a zero sample belongs to the boundary of a regularized occupied solid, so it does not expose a general CSG surface-normal query.
