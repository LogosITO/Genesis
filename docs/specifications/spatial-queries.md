# CPU reference spatial queries

The CPU path supports point evaluation (`ScalarField`), analytic gradients (`AnalyticGradient`), ideal zero-set bounds (`LipschitzField`), and a bounded exterior ray query for `ExactSdf` primitives. It is a reference for future comparisons, not a renderer or a certified numerical geometry kernel.

`Ray::new(origin, direction, max_distance)` requires finite coordinates, a nonzero direction, and finite positive length. Direction is normalized. `RayOptions::new(hit_tolerance, max_iterations)` requires finite positive tolerance and a nonzero iteration budget. `trace` accepts only an `ExactSdf` such as `Sphere` or `AxisAlignedBox`. General CSG `Field` cannot be passed, because its zero set may not be an occupied-solid surface.

Statuses:

- `Hit`: an exterior sample reached nonnegative field value no greater than the tolerance. It is an approximate hit at a sampled point, not an exact intersection proof.
- `Miss`: the finite segment was exhausted by the ideal Lipschitz step argument. Floating point error means this is not a formally certified numerical miss.
- `Indeterminate`: the ray began inside, the iteration budget expired, arithmetic failed, or floating point could not advance. Exhaustion is never reported as `Miss`.

The loop has a finite iteration cap and checks arithmetic progress. It handles only exterior starts and first hits. Tangencies may converge slowly and return `Indeterminate`; this is preferable to claiming a miss. `Hit` does not return a normal because gradients may be undefined at edges or seams. Callers must choose a tolerance appropriate for their world units. No hidden default tolerance is provided.

Deterministic counterexamples and bounded property tests cover primitive hits, misses, budget exhaustion, CSG ghost zero sets, transform behavior, Lipschitz inequalities, and gradients. Property runs use 128 cases per property and fixed seed `0xF002_2026` with pinned proptest 1.11.0. The old hello-world example remains unchanged.
