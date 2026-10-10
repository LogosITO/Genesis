# Native mixed viewport (research stage)

The `first-light --mixed` mode draws the current `WorldState`'s analytic spheres,
capsules, boxes, living structures, and enabled Field Graph instances in one GPU
compute pass. The analytic intersection and BVH routines are taken from the
existing shader. The Field Graph evaluator is inserted from the existing WGSL
source at shader creation; its node indices are offset when definitions are
packed. Instances with the same exact definition revision share one resident
node range. This source assembly depends on the two shader entry markers; the
WGSL validation test and real GPU test guard it. A proper WGSL module facility
would replace this assembly if more consumers need it.

The pixel ray direction is normalized in world space. Analytic and implicit
candidate distances are measured from that same world origin. An instance's
positive uniform scale divides its local near/far distances, and its returned
distance is multiplied back to world units. The nearest candidate wins. A
relative `1e-4 × max(1, |a|, |b|)` display band marks near ties. This is a
diagnostic policy, not a proven floating-point error bound. No CPU `f64` bounds
filter runs ahead of the GPU field query; the `y = 1 + 1e-8` regression remains
in the existing GPU tests.

In diagnostic mode, uncertain samples are red, exhausted rays yellow, and
near-tied depths magenta. In normal mode, a candidate is retained but tinted
orange when any field outcome prevents a reliable ordering. With no candidate,
uncertainty remains colored. A confirmed `Miss` alone may use the background.
All field intersection and normal calculations remain approximate `f32` display
work; this does not authorize implicit physical contact.

The image and definition/instance GPU buffers are reused when size/content is
unchanged. Resize recreates the image. The CPU still constructs a bounded
field snapshot and a small per-frame options uniform. The mixed frame has no
mandatory GPU readback; optional timestamp sampling and explicit capture tests
do synchronize. Neither `WorldState` nor simulation is modified by rendering.

Run locally:

```text
cargo run -p first-light --locked -- --mixed
```

Orbit with arrows or A/D/W/S. `1` toggles analytic geometry, `2` fields, `3`
diagnostics, `T` moves the first world-owned field occurrence, `N` shows
normals, and Esc exits. The life world continues to grow. The Passage uses its
unchanged analytic rendering path.

The hardware regression and actual GPU BMP capture are produced by:

```text
cargo test -p analytic-renderer --test mixed_gpu native_mixed_depth_matches_reference_at_center --locked -- --ignored --nocapture
```

It writes `target/mixed-captures/native-mixed.bmp` (ignored). The test covers
empty output, a field behind/in front of an analytic sphere, disabled fields,
visibility controls, an inside-start uncertainty, a smooth union, and a resize
to 512×288. It compares center-pixel source ordering with the blocking
offscreen reference. The captured image is actual GPU output. Full-image
identity/depth parity, camera motion automation, native screenshot automation,
and sustained interactive stability remain open verification work.
