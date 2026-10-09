# Foundation 0.14.0: Field Graph GPU scaling

**Status: partial.** A release-profile, ignored native GPU test measures the
existing interpreter at four image sizes with GPU compute timestamps and CPU
wall times. It runs a one-node sphere, generated 8- and 16-node unions, and
the bounded 32-node, 47-visit `balanced-16.json` union. Run it with:

```sh
cargo test -p analytic-renderer --test field_graph_gpu --release field_graph_resolution_matrix -- --ignored --nocapture
```

The test warms each scene/resolution once, then reports median and p95 of five
blocking frames. Measurements below are from NVIDIA GeForce RTX 4070 SUPER,
Vulkan, Windows, release profile. The two runs varied substantially; these are
observations, not a speed guarantee. `readback_wait_ms` overlaps GPU execution,
so it must not be added to the compute timestamp. `cpu_prepare_ms` includes
buffer creation/upload and command encoding; it is not an isolated upload time.

| Graph | Resolution | GPU compute median, run 1 | GPU compute median, run 2 |
| --- | ---: | ---: | ---: |
| Sphere, 1 node | 1080p | 11.35 ms | 10.65 ms |
| Sphere, 1 node | 1440p | 17.02 ms | 20.22 ms |
| Balanced union, 32 nodes | 1080p | 57.37 ms | 84.71 ms |
| Balanced union, 32 nodes | 1440p | 96.96 ms | 106.57 ms |

An extended third run measured the generated 8-node/11-visit graph at 7.28,
17.39 and 30.93 ms GPU compute median at 720p, 1080p and 1440p. The
16-node/23-visit graph took 12.09, 27.97 and 51.56 ms respectively. The
32-node graph took 32.07, 67.07 and 117.07 ms in this run. Different graph
placement changes ray lengths, so node count alone does not explain all cost.

In run 2, the 32-node graph took 34.96 ms at 720p and 4.78 ms at 320×240.
At 1440p its full blocking frame median was 110.78 ms, CPU preparation
0.095 ms and readback wait 108.88 ms. The image is 14,745,600 bytes, with an
equal-sized aligned readback buffer and 1,024 node bytes, excluding allocation
overhead, uniforms, dummy buffers and driver memory. The shader repeatedly
evaluates the graph while sphere tracing; this compute cost dominates the small
CPU upload in this workload. GPU sample counts per pixel and standalone upload
time are not yet instrumented.

Changing the workgroup from 8×8 to 16×8 or 4×8 slowed the 32-node graph in
local trials. An inlined one-node path modestly helped the sphere but slowed
the complex graph. A 12-frame stack strongly helped the sphere but slowed the
32-node graph; WGSL `override` as a local array length was rejected by Naga.
All experiments were reverted. The working interpreter retains an 8×8 group
and a fixed 16-frame stack. No performance improvement is claimed.

The image budget was extended to 3,686,400 pixels with a 4096-pixel per-axis
ceiling, permitting 1440p measurements while bounding readback allocation.
Adapter texture and buffer limits are checked before rendering. Seven native
GPU graph tests, including CPU/GPU parity, near-boundary cases, shared-face
sampling and world-instance readback, passed after this change. Other adapters,
longer runs, deep versus wide graph matrices, multi-instance rendering, allocation
reuse and interactive presentation remain unmeasured. A future optimization
should begin with per-pixel work instrumentation and a paired before/after
benchmark, then retain only a demonstrated gain with equal output.
