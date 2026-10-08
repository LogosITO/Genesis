# First Structure local scaling record

## Hypothesis

A bounded branching world can simulate and produce analytic node and connection snapshots at 1–48 nodes, while an over-capacity four-organism scene fails explicitly rather than losing geometry.

## Methodology and environment

On 2026-10-09, Windows 11 x64, Ryzen 5 7500F, RTX 4070 SUPER (driver 591.86), Vulkan, Rust 1.94, `wgpu 30.0.1`. The deterministic scene uses one source at `(0,2,0)` with radius 100 and strength 10; each organism starts at a fixed root, uses threshold 0.12 and a 0.01-second step. The CLI advances until each organism has the requested count. For each state it takes the median of 25 one-step timings on clones and 25 CPU snapshot builds. A four-organism run reaches 192 nodes. Timings below are single local runs, not throughput guarantees. `node storage` is only `node_count × size_of::<GrowthNode>()`, a lower bound rather than process RSS.

The opt-in GPU test builds the same one-organism states, draws at 1280×720, discards three warm-up runs and reports medians of five. `GPU compute` comes from timestamp queries; `full frame` includes draw submission and blocking readback, not window presentation. Dev and release profiles were run separately on the same machine.

## Results

| Nodes / organisms | Primitives | Node storage lower bound | CPU step dev / release µs | Snapshot dev / release µs | GPU compute dev / release ms | Full frame dev / release ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 / 1 | 2 | 48 B | 1.5 / 0.4 | 1.5 / 0.3 | 0.017 / 0.017 | 0.477 / 0.198 |
| 8 / 1 | 16 | 384 B | 4.5 / 1.2 | 8.0 / 1.1 | 0.124 / 0.123 | 0.569 / 0.310 |
| 16 / 1 | 32 | 768 B | 7.3 / 1.6 | 15.7 / 1.7 | 0.245 / 0.244 | 0.728 / 0.437 |
| 32 / 1 | 64 | 1,536 B | 12.9 / 2.5 | 32.8 / 3.3 | 0.487 / 0.486 | 1.021 / 0.696 |
| 48 / 1 | 96 | 2,304 B | 18.1 / 3.0 | 52.3 / 5.3 | 0.729 / 0.727 | 1.268 / 0.933 |
| 192 / 4 | 381 required | 9,216 B | 71.2 / 8.4 | 202.3 / 19.3 to error | unavailable | unavailable |

The four-organism world remained valid and saved as 52,304 JSON bytes, below the 65,536-byte save cap for this scene. Its snapshot returned `TooManyObjects` after reaching the 256-object cap. The measured snapshot duration is time to error, not a complete 381-object build. Ordinary spheres would increase the required primitive count further.

## Limitations and findings

The naïve GPU loop tests each of 96 objects per pixel at 48 nodes. The observed compute time rose with primitive count; this is a local observation, not an asymptotic proof. CPU snapshot construction performs duplicate-ID checks while inserting and is bounded but grows noticeably in the dev profile. No resident-memory profiler, multi-adapter comparison, sustained frame-rate measurement for these fixed scenes, or high-resolution GPU scaling study was performed. Floating-point hit accuracy outside selected differential rays remains unproven. A native visual capture is recorded separately only if a real viewport run can be inspected.

## Reproduction

```sh
cargo run -p first-life --locked -- scale --measure
cargo run -p first-life --release --locked -- scale --measure
cargo test -p analytic-renderer --test gpu --locked gpu_growth_scaling_measurements -- --ignored --nocapture
cargo test -p analytic-renderer --test gpu --release --locked gpu_growth_scaling_measurements -- --ignored --nocapture
```

## Conclusion

The one-organism analytic snapshot was measurable through 48 nodes locally. The existing 256-primitive GPU budget cannot display the full four-organism world, and the failure is explicit. Foundation 0.6 should first define a correct bounded visibility or acceleration policy before increasing world-scale claims.
