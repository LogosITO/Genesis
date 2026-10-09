# First Scale local measurements

## Status and method

These are local observations, not cross-adapter claims. Reference machine: Windows, NVIDIA GeForce RTX 4070 SUPER, Vulkan, NVIDIA driver 591.86, Rust 1.94.0, `wgpu` 30.0.1. GPU compute time uses `TIMESTAMP_QUERY`, which this adapter supports. Each GPU row uses three warmups and ten measured frames; median is the upper middle sample and p95 is the largest of ten. Snapshot times use 25 builds. Benchmarks below were run serially. Render target is RGBA8. GPU upload is part of CPU submit time and was **not separately isolated**. CPU frame time includes timestamp readback, so it is not a presentation frame-time guarantee. No swapchain/present timing or per-ray intersection counter was measured.

Before Foundation 0.6, both the CPU snapshot and WGSL loop had a 256-primitive cap. The old direct GPU compute medians at 1280×720 for the nested fixed grid of 1/13/48/128/256 ordinary spheres were 0.0129/0.0661/0.2220/0.5773/1.1507 ms in dev and 0.0130/0.0659/0.2215/0.5774/1.1507 ms in release. The 381 case returned `TooManyObjects`; it had no valid frame. The new fixed-grid contents differ from the real growth scene, so their timings are not compared as a pure count effect.

## Direct GPU path after the change

The same nested fixed grid, fixed camera, 1280×720 pixels. Values are GPU compute **median / p95 in ms**. One GPU record is 64 bytes; the last column is active primitive bytes, not all GPU memory.

| Primitives | Dev | Release | Active bytes |
| ---: | ---: | ---: | ---: |
| 1 | 0.0124 / 0.0126 | 0.0127 / 0.0130 | 64 |
| 13 | 0.0597 / 0.0600 | 0.0598 / 0.0603 | 832 |
| 48 | 0.1974 / 0.1978 | 0.1983 / 0.1984 | 3,072 |
| 128 | 0.5110 / 0.5113 | 0.5125 / 0.5130 | 8,192 |
| 256 | 1.0124 / 1.2105 | 1.0154 / 1.0156 | 16,384 |
| 381 | 1.5022 / 1.5028 | 1.5063 / 1.5068 | 24,384 |

For the **real** four-organism world, 192 growth nodes produced 381 render primitives. The direct renderer drew all 381. Fixed camera at `(0,3,-12)` looking toward `(0,2,0)`; 64-byte records total 24,384 active bytes. Values are median / p95 in ms:

| Resolution | Dev GPU compute | Release GPU compute | Dev full frame | Release full frame |
| --- | ---: | ---: | ---: | ---: |
| 1280×720 | 2.8204 / 3.3348 | 2.8205 / 3.0669 | 3.4089 / 3.9454 | 3.0669 / 3.3196 |
| 1920×1080 | 6.2954 / 6.7736 | 6.2951 / 6.7952 | 6.9277 / 7.5008 | 6.5631 / 7.1527 |
| 2560×1440 | 11.1478 / 11.6561 | 11.1468 / 11.6619 | 11.8653 / 12.3949 | 11.5894 / 12.1078 |

Real-scene snapshot construction was 0.3305 / 0.3765 ms dev and 0.0426 / 0.0477 ms release. The real scene costs more than the fixed sphere grid at 381 because it includes capsule intersection work and different depth/overlap patterns. Source movement rebuilt a full 381-primitive snapshot; two viewport sizes and changed source ID were checked. Of 525 selected rays, 98 hit; GPU direct and CPU direct had zero ID differences, and selected distances/normals met test tolerances. This is a sample, not a proof over all rays.

## CPU BVH experiment

The CPU median-split tree for that real snapshot had 255 nodes, depth 8, and 64,008 bytes of allocated node/index capacity (excluding the scene). In a release build, 25 builds gave 0.3227 / 0.5200 ms median / p95. For the same 525 rays, 20 batches gave direct CPU 8.0479 / 8.7781 ms and BVH CPU 0.1051 / 0.1364 ms per batch. The 525-ray correctness test compared IDs, distances, and normals exactly against CPU direct and passed. This does **not** measure GPU acceleration: the GPU still uses direct traversal. The tree is rebuilt for each immutable snapshot; no refit or GPU upload was implemented.

## Visual checks and limits

Actual offscreen GPU captures were produced for a single capsule, branching organism, four-organism 381 scene, overlapping connections, and 381 analytic spheres. The images were visually inspected locally. The major shapes appeared, but the large yellow source marker occludes branches and seams between overlapping analytic solids remain visible. Pixel captures are not a topology or watertightness proof. `gpu_first_scale_reference_captures` can write PPM files by setting `GENESIS_CAPTURE_DIR`; otherwise it checks visible geometry pixels without saving files.

The direct GPU algorithm remains approximately `O(pixels × primitives)`. There is no conservative GPU culling, GPU BVH traversal, separate upload timer, full render-target memory accounting, cross-adapter parity study, or certified floating-point containment. For further scaling, validate a bounded GPU hierarchy against direct GPU and CPU results before making performance claims.

## Reproduction

```sh
cargo test -p analytic-renderer --test gpu cpu_bvh_matches_real_381_primitive_world --locked
cargo test --release -p analytic-renderer --test gpu cpu_real_381_bvh_benchmark --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test gpu gpu_fixed_scene_scale_benchmark --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test gpu gpu_fixed_scene_scale_benchmark --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test gpu gpu_real_381_resolution_benchmark --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test gpu gpu_real_381_resolution_benchmark --locked -- --ignored --nocapture --test-threads=1
```

The historical [First Structure measurements](first-structure-measurements.md) remain the original 0.5 record of the failed 381 snapshot; its error timing is not a completed snapshot timing.
