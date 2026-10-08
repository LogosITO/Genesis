# Mathematical field contracts

Foundation 0.2 is a CPU reference model. All distances use the world length unit. `sample` returns `Result<f64, MathError>` and rejects nonfinite results. The guarantees below concern the mathematical functions in exact real arithmetic; an `f64` result is an approximation, not an interval proof.

| Representation | Contract | Valid operations in 0.2 | Limits |
| --- | --- | --- | --- |
| `ScalarField` | Any finite scalar sample where evaluation succeeds. | Point evaluation. | No sign, distance, or continuity guarantee. |
| `SignedField` | Negative means chosen interior, positive exterior, zero is a zero set. | Sign classification and `contains` (`sample <= 0`). | Zero need not be an occupied solid or physical boundary. An arbitrary implementation need not be Lipschitz. |
| `ExactSdf` | In ideal arithmetic, signed Euclidean distance to a nondegenerate primitive boundary. | Primitive point sampling, analytic gradients where defined, bounded CPU ray query. | Implemented only by `Sphere` and `AxisAlignedBox`; floating point values are not exact reals. |
| `LipschitzField` | Global finite positive Euclidean Lipschitz upper bound `L`. | Ideal zero-set lower bound `abs(f(p))/L`. | Implemented by primitives and `Field`; it says nothing about regularized solid boundaries or rounding-certified steps. |
| `IdealDistanceBound` | Typed value of the ideal zero-set lower-bound formula. | Read its nonnegative value. | The stored floating point value is not a certified numerical lower bound. |

`Sphere` and `AxisAlignedBox` have ideal Lipschitz constant one. Their radii and half extents must be finite and strictly positive. `Transform` permits only finite translation and strictly positive finite uniform scale. Translation and positive uniform scale preserve the ideal SDF and Lipschitz properties. Degenerate primitives and zero/negative scale are rejected; CSG can still produce an empty or lower-dimensional result.

`Field` combines signed functions using `min(a,b)` for union, `max(a,b)` for intersection, and `max(a,-b)` for difference. It can also transform a child. These expressions are globally one-Lipschitz in ideal arithmetic, but **`Field` is not an `ExactSdf`** and its zero set is not necessarily a solid boundary. `Field::difference(A,A)` equals `abs(fA)`: it is zero on A's original surface but has no negative interior. Externally tangent solids can similarly give a zero-only intersection. `SignedField::contains` is retained as a non-positive sample test for compatibility; it is not an occupied-solid query. Negative samples identify the expression's strict interior. The regularized solid would require the closure of that interior, and its boundary requires neighborhood information not provided by this API.

`AnalyticGradient` returns `Defined(Vec3)` only for a unique derivative. It returns `Undefined` at sphere centers, AABB non-smooth points, and CSG branch ties. A `Defined` gradient is a surface normal only after a separate valid surface test. See `gradients.md` for formulas.

All input vectors are finite. Evaluation may still fail for finite values when intermediate arithmetic overflows. Subnormal scales, cancellation near surfaces, and branch comparisons can lose precision. The API applies no implicit epsilon and makes no cross-platform bitwise guarantee. Deep recursive `Field` trees may exhaust the call stack. No empty-geometry marker, topology solver, or floating-point certified distance bound exists yet.
