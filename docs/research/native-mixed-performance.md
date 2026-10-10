# Native mixed compute measurements

## Method

This is one local GPU experiment, not a frame-rate promise. Windows 11 Pro
build 26200, NVIDIA GeForce RTX 4070 SUPER, driver 591.86, Vulkan, Rust 1.94.0,
`wgpu` 30.0.1, release profile. The opt-in test uses the actual native mixed
compute pipeline with one camera at `(0,0,-6)` and a 0.9 radian vertical field
of view. Each row has 3 warmups and 7 serial samples. GPU compute uses a
validated timestamp query around the compute pass. CPU draw/submit time ends
when the queue is submitted. A separate blocking timestamp read waits for GPU
completion; it is **not** full-frame presentation or image readback. No window
or swapchain is involved. The test prints machine-readable `MIXED_BENCH` JSON:

```text
cargo test --release -p analytic-renderer --test mixed_benchmark native_mixed_compute_matrix --locked -- --ignored --nocapture --test-threads=1
```

The graphs are exact, generated unions of translated spheres with 1, 8, 16,
or 32 nodes. The analytical case has one sphere. Mixed uses that sphere plus
the graph. The work is not normalized for image occupancy; an `empty` graph
is translated to x=40, `occupied` uses a radius-3 sphere, and `overlap` and
`shared` each draw two occurrences of the same 8-node definition. The latter
share one resident GPU node range but perform two ray traces per pixel.

## Observations

GPU compute median / p95 in milliseconds from the first 8×8 run:

| Scene | Nodes | 720p | 1080p | 1440p |
| --- | ---: | ---: | ---: | ---: |
| Field | 1 | 3.99 / 4.55 | 9.41 / 9.58 | 15.91 / 16.72 |
| Field | 8 | 6.07 / 6.69 | 14.13 / 15.02 | 25.03 / 25.49 |
| Field | 16 | 11.00 / 11.58 | 24.29 / 25.75 | 41.22 / 44.13 |
| Field | 32 | 22.92 / 24.71 | 48.87 / 53.15 | 85.46 / 87.10 |
| Analytic only | 0 | 0.014 / 0.015 | 0.028 / 0.028 | 0.046 / 0.046 |
| Mixed | 8 | 6.53 / 7.22 | 14.03 / 14.50 | 25.07 / 26.66 |
| Mixed | 32 | 21.16 / 24.15 | 50.34 / 51.12 | 85.23 / 87.89 |

At 1080p, the additional scene types gave `empty` 32-node 17.73 / 18.43,
`occupied` 1-node 14.02 / 15.19, `overlap` 8-node 28.86 / 29.19, and
`shared` 8-node 28.02 / 29.68 ms. CPU draw/submit medians were 0.07–0.21 ms
across these rows. The timestamp wait followed GPU compute and was typically
roughly 0.1–0.3 ms longer; it is deliberately excluded from CPU draw/submit.
No image readback, CPU graph compilation, GPU upload completion, window frame
interval, or input-to-photon latency was separately measured. The underlying
GPU graph buffer is reused after unchanged frames, while the CPU rebuilds a
bounded node vector to check changes. These numbers identify field evaluation
as the dominant observed compute cost for this matrix. Even mostly empty rays
carry substantial work because no per-instance ray-domain reduction exists.

## Safe tuning experiment

We tried 16×8 workgroups instead of 8×8 without changing field mathematics.
The same matrix was run in an A/B/A sequence; the last A run restored 8×8.
At 1080p, the 32-node field median was 48.87 / 54.03 / 50.16 ms for
A / B / A2. Mixed 32-node was 50.34 / 52.99 / 50.77 ms. The 8-node rows varied
too little and inconsistently to support a gain. The 16×8 change was reverted.
There is no accepted speedup from this experiment. A bounds-based skip was not
added because an unproved `f64`→`f32` prefilter previously caused a real false
miss; the numerical contract must come before that optimization.

Next work should instrument sphere-tracing steps per ray and test a defensible
GPU-space conservative domain. Full native frame intervals and workload-specific
rebuild/upload costs also need measurement. Seven samples and one adapter do not
establish cross-machine performance or stable p95 tails.

The later [Performance Research 1](performance-research-1.md) tested an expanded
GPU-local ray domain, a static WGSL specialization experiment, and fresh
release-profile measurements. This earlier record remains the original baseline.
