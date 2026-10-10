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

Seven release-profile command runs with 4096-ray chunks measured **213.61 ms
median** and **274.10 ms p95** full blocking time for the 256×256 fixture on
this machine. The later 65,536-ray bounded batch reduced submissions and
readbacks; seven release runs measured **152.40 ms median** and **156.23 ms
p95** (`129.33, 146.84, 156.23, 150.41, 153.12, 152.40, 154.64` ms). These
are separate local runs, not controlled paired GPU timestamps. The resulting
256×256 RGB image was byte-identical to the previously inspected capture; its
5,816 analytic, 6,784 front-field, 552 rear-field and 8 exhausted pixels also
matched. An opt-in 4097-ray test compared one call with the previous chunking
and exercised rejection at 65,537 rays. Input and output storage sizes are
checked against adapter limits before allocation. Each call still uploads
graph data; timings combine CPU, GPU, synchronization and PPM output. They are
not GPU compute timestamps or native frame intervals. Even the improved path
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
