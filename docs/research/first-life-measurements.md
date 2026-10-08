# First Life local measurement record

**Historical Foundation 0.4 record at commit `83054d4`.** Current Foundation 0.5 branching and capsule code changes topology, primitive counts, and timings; run the commands below at that commit to reproduce this table. See the [First Structure record](first-structure-measurements.md) for current measurements.

## Hypothesis

A bounded analytic growth graph can be advanced, saved, converted to a render snapshot, and queried on one native GPU without storing a mesh or voxel world.

## Methodology

`first-life [scenario] --measure` ran 120 fixed steps at `1/60` simulated seconds in the Cargo dev profile. It timed the entire 120-step CPU loop, one CPU `Scene::from_world` construction, one pretty-JSON serialization, and one parse/validation. These are single runs after compilation, with no warm-up or statistical confidence. A separate opt-in GPU test constructed the 120-tick baseline snapshot, compared GPU ray hits to CPU hits, then made three warm-up and five measured offscreen draws at 1280×720. CPU submit and compute timestamps are medians of five. The native viewport logged approximate frame rate and GPU compute time over two-second windows; those logs use a changing scene.

## Environment

- Local run: 2026-10-09; Windows 11 Pro 10.0.26200 x64.
- AMD Ryzen 5 7500F, 12 logical processors; NVIDIA GeForce RTX 4070 SUPER, driver 591.86; Vulkan backend.
- Rust/Cargo 1.94.0, `wgpu 30.0.1`, Cargo dev/test profile, 1280×720 GPU image.
- The headless run needs no GPU device. No Linux or second-GPU run has been performed.

## Metrics and results

| Scenario | Nodes at tick 120 | CPU step mean | Snapshot | Save | Load | Save size |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Baseline | 12 | 1.462 µs | 15.000 µs | 149.200 µs | 125.800 µs | 3,694 B |
| Changed at tick 40 | 10 | 1.579 µs | 18.300 µs | 162.400 µs | 149.300 µs | 3,362 B |
| Resource limited | 1 | 1.255 µs | 18.400 µs | 113.000 µs | 121.600 µs | 1,121 B |

The changed scenario's tip was `(1.472783680560033, 2.1493189660112506, 0)`; baseline tip was `(0, 3.5199999999999996, 0)`. The limited scenario kept only the root. In the GPU test, the baseline snapshot had 13 primitives. Its CPU submit median was **0.521 ms** and GPU compute median **0.065 ms** at 1280×720; selected per-node rays agreed with CPU intersection IDs and distances within `2e-4` world units. A native viewport run logged 164.9 frames/s over one two-second interval with 18 primitives and a `0.187 ms` sampled compute pass. That is an observed frame interval of about 6.06 ms, including scheduling and presentation; it is not an isolated renderer cost.

## Limitations

The CPU figures are single-run diagnostic timings and varied with compilation, process scheduling, and scene size. The GPU test serializes draws with timestamp readback and is not a throughput benchmark. The native viewport scene changes during measurement; its frame rate cannot be directly compared to the fixed snapshot GPU sample. A window-only capture showed visible analytic nodes and source. A targeted Windows `M` key message moved the source; a later capture showed the new source position and the chain bending toward it. Pause/resume ergonomics and long-running stability were not exercised. No performance target or scalability claim follows.

## Reproduction

```sh
cargo run -p first-life --locked -- baseline --measure
cargo run -p first-life --locked -- changed --measure
cargo run -p first-life --locked -- limited --measure
cargo test -p analytic-renderer --test gpu --locked -- --ignored --nocapture
cargo run -p first-light --locked -- --life
```

## Conclusion

This local slice exercised deterministic graph growth, serialization, CPU snapshot construction, GPU queries, and one native source-move interaction on a Windows/Vulkan system. Cross-platform reproducibility, pause/resume interaction, and larger-scene performance remain open.
