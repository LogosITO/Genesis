# Distance bounds and numerical scope

## Established mathematical result

For a continuous globally `L`-Lipschitz function `f` on Euclidean 3-space, with `L > 0`, every point `z` in its zero set satisfies `|f(p)| = |f(p)-f(z)| <= L ||p-z||`. Therefore `|f(p)|/L` is a lower bound on distance from `p` to the **zero set** in exact real arithmetic. If the zero set is empty, this is still a finite lower bound on an infinite distance. For exterior ray marching, require `f(p) > 0` and move no farther than this ideal bound. A negative sample means the ray started or entered the chosen interior; the current ray query does not trace exits.

Sphere and axis-aligned box SDFs are one-Lipschitz. Negation preserves `L`. For two functions bounded by `L1` and `L2`, their pointwise `min` and `max` are bounded by `max(L1,L2)`. Positive uniform scale with `g(p)=s f((p-t)/s)` preserves the child's Lipschitz bound. Consequently every currently constructible `Field` has ideal bound `L=1`, including CSG and transformed children. This result concerns its zero set, which may include a ghost zero set without an occupied solid.

## Implemented query

`LipschitzField::ideal_zero_set_bound(p)` evaluates `abs(sample(p))/L` and returns `IdealDistanceBound`. It rejects nonfinite or nonpositive `L` and nonfinite results. This is a typed *ideal-arithmetic* contract, separate from `ExactSdf`. `Field` implements it despite not being an exact SDF. It is valid as a zero-set bound in the ideal model; it does **not** identify a regularized solid boundary.

## Floating point limitation

Rust `f64` evaluation rounds coordinates, `hypot`, subtraction, scaling, and division. The implementation has no proven outward rounding or error envelope for these operations. A computed positive value can exceed its exact-real counterpart. Thus `IdealDistanceBound::value()` is **not** a certified conservative numerical step and must not be used unguarded for a GPU sphere tracer or collision response. A future certified API needs explicit evaluation error bounds or interval arithmetic, robust transform handling, and a rule for numerical margins. Arbitrary `SignedField` implementations have no Lipschitz contract and are excluded even from the ideal bound.
