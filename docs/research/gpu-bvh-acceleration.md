# GPU BVH acceleration: local experiment

## Status and method

Foundation 0.6.1 is an experimental renderer extension, not a certified numerical method. Measurements below were made locally on Windows, NVIDIA GeForce RTX 4070 SUPER, Vulkan, driver 591.86, Rust 1.94.0, `wgpu` 30.0.1. Direct and BVH used the same analytic scene, camera, resolution, RGBA8 target, and shader intersection routines. GPU compute time came from adapter timestamp queries. Each row used three warmups and ten measured draws; median is the upper middle sample and p95 is the largest of ten. Full-frame time includes CPU dispatch and GPU completion/timestamp readback, but not swapchain presentation. Benchmarks ran serially in release profile. These are one-run observations, not cross-adapter throughput claims.

The fixed-grid cases contain nested ordinary spheres. The real 381 case contains four grown organisms, their capsule connections, and one source. CPU BVH build timings use 25 fresh builds; node counts and depth are deterministic for each snapshot. A 64-byte primitive record and a 64-byte BVH node are allocated in distinct storage buffers. The first-upload numbers below are CPU time in `queue.write_buffer`, **not measured GPU transfer time**.

## Correctness evidence

The four-way differential test read back 16,861 deterministic rays against the real 381-primitive scene. It included 1,722 GPU hits. CPU direct and CPU BVH agreed exactly on hit, identity, distance, and normal. GPU direct and GPU BVH also agreed exactly on those fields. One CPU `f64` versus GPU `f32` identity difference was a near-distance tie; the alternate CPU hit was within the stated comparison tolerance. Additional readback tests cover an empty scene, equal and overlapping spheres, boxes, long and degenerate capsules, tiny and large primitives, tangent and near-tangent rays, interior starts, near/far clipping, growth, source movement, and camera movement. The broad test is deterministic but does not sample the full accepted coordinate/radius range.

Four 800×450 offscreen scene pairs were saved from actual GPU readback: branching organism, four-organism stress scene, overlapping capsules, and moved camera. Direct/BVH RGB byte mismatches: **zero** in all four pairs (360,000 pixels per pair). The images were visually inspected. Major shapes are present; visible seams at overlapping solids and occlusion by the large yellow source are common to both paths. The capture files are generated test artifacts, not simulated screenshots. Set `GENESIS_CAPTURE_DIR` to an absolute path to retain them.

## Fixed-grid GPU timings at 1280×720

All times are GPU compute **median / p95 in ms**. CPU build is release-profile median in ms. Active BVH bytes exclude spare allocation capacity.

| Primitives | Direct GPU | BVH GPU | CPU build | Nodes / depth | Active BVH bytes |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.0123 / 0.0130 | 0.0194 / 0.0195 | 0.0001 | 1 / 1 | 64 |
| 13 | 0.0580 / 0.0583 | 0.0216 / 0.0219 | 0.0010 | 7 / 3 | 448 |
| 48 | 0.1909 / 0.1912 | 0.0256 / 0.0261 | 0.0096 | 31 / 5 | 1,984 |
| 128 | 0.4932 / 0.4936 | 0.0290 / 0.0297 | 0.0666 | 63 / 6 | 4,032 |
| 256 | 0.9768 / 1.2132 | 0.0409 / 0.0417 | 0.2125 | 127 / 7 | 8,128 |
| 381 | 1.4489 / 1.4495 | 0.0572 / 0.0582 | 0.2495 | 255 / 8 | 16,320 |

BVH is slower at one primitive. The automatic policy therefore keeps direct traversal for up to four primitives, one leaf's capacity. At 13 primitives the measured BVH compute and full-frame times were lower, but no adapter-independent crossover is claimed.

## Real four-organism scene

All times are **median / p95 in ms**. This scene has 381 primitives, 255 nodes, depth 8, 24,384 active primitive bytes, 16,320 active BVH bytes, and a 16,384-byte BVH buffer allocation.

| Resolution | Direct GPU compute | BVH GPU compute | Direct full frame | BVH full frame |
| --- | ---: | ---: | ---: | ---: |
| 1280×720 | 2.8610 / 3.1532 | 0.1315 / 0.1390 | 3.1086 / 3.4466 | 0.3699 / 0.3986 |
| 1920×1080 | 6.0352 / 6.8625 | 0.1417 / 0.1477 | 6.2626 / 7.1149 | 0.4093 / 0.4614 |
| 2560×1440 | 10.8304 / 10.8740 | 0.1945 / 0.2031 | 11.0518 / 11.1289 | 0.4077 / 0.5497 |

Fresh CPU BVH build median/p95 was 0.3138/0.4932 ms for the real scene. The first real-scene BVH upload enqueued 16,320 bytes in 0.0230 ms of CPU time; its BVH build-and-flatten measurement was 0.5173 ms. First primitive upload enqueued 24,384 bytes in 0.0122 ms. These individual enqueue observations are not paired GPU-copy timings and should not be added to the steady-state GPU compute table.

## Limits and reproduction

The conservative bounds and slab margins are practical floating-point guards, not a proof against all false culling. GPU direct itself can differ from CPU `f64` near tangencies and almost equal hits. Traversal is preorder rather than ray-near-first; a different scene may reduce or reverse the speedup. There is no incremental refit, hardware ray tracing, presentation timing, cross-adapter parity, per-ray visit count, or isolated GPU transfer timer. Buffer allocation figures exclude render targets and driver overhead.

```sh
cargo test -p analytic-renderer --test gpu gpu_bvh_broad_real_scene_differential --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test gpu gpu_bvh_analytic_edge_cases --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test gpu gpu_bvh_rebuilds_after_growth_and_source_movement --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test gpu gpu_bvh_offscreen_image_parity --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test gpu gpu_bvh_comparison_benchmark --locked -- --ignored --nocapture --test-threads=1
```

The historical [First Scale measurements](first-scale-benchmark.md) remain the Foundation 0.6 record for direct GPU rendering and the CPU-only BVH. The current [spatial acceleration design](../architecture/spatial-acceleration.md) describes the new GPU path.
