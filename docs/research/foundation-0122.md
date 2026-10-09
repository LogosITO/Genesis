# Foundation 0.12.2: narrow numerical exclusions

**Status: partial numerical contract, local experiment.** The goal was to remove a
false sense of certainty from the existing exterior field tracer. The result is a
small CPU broad phase, not a certified field evaluator or collision algorithm.

## Model and method

The ideal geometry uses the exact real values of validated, finite binary `f64`
parameters. CPU arithmetic is assumed to use IEEE-754 round-to-nearest, gradual
underflow and no fast-math reassociation. A sphere or box has exact primitive
axis bounds in this model. Each graph translation or positive uniform scaling
enlarges the resulting axis limits by one representable `f64` value in the required
direction; union takes minima/maxima. Query-ray endpoint multiplication and addition
are similarly enlarged. A closed ray segment or region strictly separated from
the graph's enlarged AABB has no occupied point in common with the graph.

This argument does **not** enclose errors from `Sphere::sample`, `Box::sample`,
`hypot`, graph branch selection, or WGSL `f32`. The existing Lipschitz region
estimate stays diagnostic only. A field step passing the ray's far limit now
returns `Indeterminate(UncertifiedAdvance)` unless the entire segment is disjoint
by bounds. `BoundsMiss` is the only confirmed CPU solid miss in this API.

## Reproductions

- A translated sphere with radius `0.1` and offset `0.2` has outward x-bounds
  strictly wider than the rounded arithmetic endpoints; a point on the rounded
  edge remains `Unknown`, while a separated region is excluded.
- A ray at `(0.9,0.9,-3)` along +z passes through the sphere's AABB but misses
  the sphere itself. The sample-based far step returns `UncertifiedAdvance`, not
  `Miss`. A ray at x=3 is strictly outside the AABB and returns `BoundsMiss`.
- A fixed-seed generator produced 16 valid four-sphere union graphs and checked
  64 points each: **1,024 CPU comparisons** with independent Euclidean sphere
  values. Sample signs and shape bounds agreed in the tested domain; no confirmed
  region exclusion contained a sampled occupied point.
- A separate GPU readback used existing balanced graphs with 1, 4, 8 and 16
  leaves, 128 points each: **512 actual GPU samples**, including four boundary
  points. On the local NVIDIA GeForce RTX 4070 SUPER/Vulkan adapter, values
  differed by less than 0.002 world units and signs matched outside a 0.002 band.
  Four near-zero samples were deliberately not assigned a physical-boundary label.

These deterministic tests are regressions, not a proof of floating-point behaviour
on other machines. The next work is a stronger interval or analytic primitive
classification for regions overlapping the graph AABB, plus ray outcomes for
inside and tangent starts. Until then, `Unknown` is intentional and field graph
collision remains disabled in `WorldState`.

## Local verification

`cargo fmt --all -- --check`, Clippy with `-D warnings`, workspace build, workspace
tests, rustdoc, and `cargo deny check advisories licenses` passed. The ordinary
suite ran **123 tests, 0 failures**; the separate opt-in suite ran **33 tests,
0 failures**. The site built for `/Genesis/` and checked **11,047** local links.
GitHub-hosted CI, another CPU implementation, another GPU, and native interactive
input were not verified by this local experiment.
