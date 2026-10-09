# First Interaction local measurements

## Method

Windows, NVIDIA GeForce RTX 4070 SUPER, Vulkan, driver 591.86, Rust 1.94.0, `wgpu` 30.0.1. CPU timings use release profile, 25 serial repetitions, and median / p95 in milliseconds. Each operation is timed separately: CPU picking includes BVH build plus direct parity and ambiguity scans; schedule includes validation; pruning operates on a cloned world; snapshot and BVH are built after that cut. Values are local observations, not latency guarantees. The small case had 12 primitives; the real four-organism case had 381.

| Scene | CPU pick | Schedule cut | Prune update | Snapshot rebuild | CPU BVH rebuild |
| --- | ---: | ---: | ---: | ---: | ---: |
| Small, 12 primitives | 0.0033 / 0.0049 | 0.0006 / 0.0012 | 0.0004 / 0.0005 | 0.0003 / 0.0004 | 0.0002 / 0.0003 |
| Real, 381 primitives | 0.4784 / 0.5934 | 0.0077 / 0.0119 | 0.0032 / 0.0062 | 0.0410 / 0.0616 | 0.3376 / 0.5671 |

At 1280×720 in the same release build, the 381-primitive scene's steady GPU BVH compute was 0.1340 / 0.1360 ms and full-frame measurement was 0.3014 / 0.3340 ms. Pruning child 1 of the first organism removed 28 nodes and changed the renderer snapshot from 381 to 325 primitives. The **first changed frame** took 0.7411 ms wall time with 0.1475 ms GPU compute. Its primitive/BVH queue writes were 20,800/16,320 bytes, CPU enqueue time 0.0107 ms, and CPU BVH build/flatten time 0.3958 ms. Subsequent changed-scene frames measured 0.1516 / 0.1580 ms GPU compute and 0.3283 / 0.3563 ms full-frame. The post-cut scene was a different workload; these before/after GPU times are not a pure acceleration comparison. Queue time is not GPU transfer time, and full-frame time excludes presentation.

An independent 96-to-34-primitive pruning test compared actual 800×450 offscreen readbacks: GPU direct and BVH after the cut matched byte for byte; 2,052 pixels differed from the pre-cut image. CPU direct, CPU BVH, GPU direct, and GPU BVH agreed on the sampled post-cut rays. The captures were visually inspected: the cut branch disappeared, and the large source marker and existing capsule seams remained. The test used a real NVIDIA Vulkan adapter; other GPUs and drivers were not tested.

## Deterministic headless outcomes

All cases use seed 7 and 120 fixed ticks. `pruning` applies child-1 pruning at tick 60; `pruning-replay` saves and reloads at tick 80, then continues. These are actual JSON outputs' key values from a local development build. The FNV-1a fingerprint is diagnostic, not cryptographic.

| Scenario | Nodes | Bifurcations | Snapshot primitives | Child 1 active | Fingerprint |
| --- | ---: | ---: | ---: | --- | --- |
| Baseline | 12 | 5 | 24 | yes | `41032c1efd72b17e` |
| Pruning | 8 | 3 | 16 | no | `1a421ba1aaa89eca` |
| Pruning + replay | 8 | 3 | 16 | no | `1a421ba1aaa89eca` |

The pruning and replay runtimes compare equal in a test, including sparse IDs, resources, budgets, clock, and pending event state. The baseline differs in authoritative topology, not only image color.

## Reproduction and limits

```sh
cargo run -p first-life --locked -- baseline
cargo run -p first-life --locked -- pruning
cargo run -p first-life --locked -- pruning-replay
cargo test --release -p first-life interaction_cpu_benchmark --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test gpu gpu_pruning_update_benchmark --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test gpu gpu_bvh_rebuilds_after_growth_and_source_movement --locked -- --ignored --nocapture
```

The chosen picking ambiguity tolerance and CPU BVH bounds are not certified for all extreme rays. Timings use one adapter and one local run. The native input workflow was not measured as a human task, and no presentation latency or independent GPU buffer-copy time was captured.
