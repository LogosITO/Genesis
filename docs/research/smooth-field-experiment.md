# Smooth field experiment

**Status:** local experimental finding, 2026-10-10.

## Hypothesis

A restricted polynomial smooth union can produce a visible continuous bridge
while preserving the graph's ideal unit-Lipschitz capability and finite
occupied bounds.

## Methodology and environment

The external `smooth-bridge.json` uses two radius-one spheres translated by
±0.9 world units with width 0.8; `hard-bridge.json` keeps the same inputs
with a hard union. CPU samples, gradient, bounds and rejected
solid query were tested. A native RTX 4070 SUPER/Vulkan GPU evaluated six
explicit points and a 160×120 image against a hard union with the same
primitive positions. The standard 640×480 preview wrote a real GPU PPM under
`target/field-graph-captures/`. Build profile was debug. Reproduce with:

```sh
cargo test -p world-authoring --locked
cargo test -p analytic-renderer --test field_graph_gpu smooth_union_gpu_samples_and_pixels_match_the_cpu_model --locked -- --ignored --nocapture
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/smooth-bridge.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/hard-bridge.json
```

## Results

At the origin the CPU field is −0.3, versus −0.1 for the hard union. At
`(0,0.5,0)` the smooth field is negative while the hard union is positive:
the bridge is a genuine changed occupied region. Six GPU samples matched the
CPU formula within 0.0001 world units on the selected points. The 160×120
GPU readbacks differed in 3,403 color bytes. Both 640×480 captures were visually
inspected: the hard union has a sharp neck, while the smooth field has a broad
continuous connection. The smooth centre ray
reported a GPU hit at 5.205465 and the CPU preview at 5.2054655 world units.
The smooth capture's measured full blocking time was 7.9911 ms in one debug
run and the hard capture's was 24.1726 ms in another. These uncontrolled
single runs are not a performance comparison or GPU compute timestamps.

## Limitations and conclusion

The bounds argument relies on exact-SDF children and ideal arithmetic.
Outward `f64` rounding of derived bounds does not certify shader `f32` hits.
The GPU normal is a finite difference, not the CPU analytic gradient. The
sample comparison and pixels were checked on one adapter only. Restricted
physical queries remain unsupported. This one operation passes an initial
visual and numerical slice; it does not complete the broader field-expression
or material program.

Periodic displacement was also assessed as a candidate: for
`F(p) + A sin(dot(ω,p)+phase)`, the ideal Lipschitz bound becomes at most
`L_F + |A|‖ω‖`. The current unit-step tracer, graph capability and bounds
compiler cannot accept that formula unchanged. It was deferred pending
explicit bound propagation and ray-step tests; no performance or visual claim
is made for it.
