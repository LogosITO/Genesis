# Mixed analytic and implicit depth reference

**Status: partial integration.** The new offscreen reference composes actual
analytic and Field Graph GPU query readbacks from one camera ray stream. It
derives the analytic snapshot from `WorldState`, queries enabled field
occurrences without changing that state, and compares approximate hit distance
along normalized world-space rays. A smaller approximate distance wins.
Both query shaders normalize the uploaded `f32` direction before evaluating
distance. This fixes an earlier mismatch in the analytic query path; the 17
existing analytic GPU/BVH opt-in tests passed after the change.
Distances within `1e-4 × max(1, |a|, |b|)` are shown magenta as ambiguous;
this is a display diagnostic, not a certified floating-point error bound.
Uncertain field outcomes are red and budget exhaustion yellow, including when
the analytic query has a hit. An analytic hit is never used to erase unresolved
field evidence. Empty, analytic-only and field-only rays are supported.

The ignored native GPU test covers an empty world, sphere in front of field,
field in front of sphere, visibility toggles, definition sharing across two
occurrences, stable identity and invalid trace policy. The fixture command

```sh
cargo run -p field-graph-preview --locked -- --mixed-world-fixture
```

produced a real 256×256 PPM on NVIDIA GeForce RTX 4070 SUPER/Vulkan. The image
was visually inspected: the analytic sphere, authored capsule and both field
occurrences are present. One run reported 5,816 analytic pixels, 6,784 front
field pixels, 552 visible rear-field pixels, 8 exhausted pixels and zero
uncertain/ambiguous pixels. The image is derived from GPU intersections, with
CPU lighting and composition. No mesh or voxel geometry was introduced.

Seven release-profile command runs measured **213.61 ms median** and
**274.10 ms p95** full blocking time for the 256×256 fixture on this machine.
Each call uploads graph data and performs repeated small GPU submissions and
readbacks; these timings combine CPU, GPU, synchronization and PPM output.
They are not GPU compute timestamps or native frame intervals. A single
debug-profile call took 267.28 ms. This design is a correctness reference and
is unsuitable for the interactive viewport as implemented.

Unresolved work: real GPU-side depth/status composition, buffer reuse, shared
definition upload, per-pass timings, resize, camera movement in a native mixed
viewport, cross-adapter validation and rigorous floating-point ordering bounds.
General implicit fields remain visual only; authoritative contacts are unchanged.

## Rejected ray-bounds prefilter

A CPU prefilter using outward-rounded `f64` world bounds was tested before GPU
field queries. It is not valid as a transparent replacement for the current
`f32` shader query. On the RTX 4070 SUPER/Vulkan, a sphere ray at
`y = 1 + 1e-8` was strictly outside the ideal sphere bounds in `f64`, but
rounded to `y = 1` on upload; the direct shader returned an approximate hit
at distance `3.9557457`, while the prefilter returned `Miss`. The candidate
was removed. An opt-in GPU regression compares direct and world-owned query
statuses at this boundary. Any future culling needs a stated GPU numerical
domain and a conservative margin for conversion, tracing tolerance and
arithmetic, plus differential tests; an ideal occupied-set exclusion alone
cannot preserve GPU diagnostic outcomes.
