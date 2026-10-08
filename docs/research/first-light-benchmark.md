# First Light local benchmark record

## Hypothesis

Analytic sphere/AABB compute rendering and readback work on at least one native adapter at 1280×720, 1920×1080, and 2560×1440 without converting world geometry to a mesh or voxel model.

## Methodology

The ignored `gpu_resolution_and_timestamp_smoke` test renders two primitives into an offscreen RGBA8 texture in the debug profile. For each resolution it executes three warm-up draws and records five draws. CPU time measures the `draw` call through queue submission, not GPU completion or presentation. When supported, a GPU timestamp pair brackets only the compute pass. The reported value is the median of five samples. Each sample waits for readback, so this is a serialized smoke benchmark, not a throughput measurement. The viewport was separately launched in a native window at 1280×720 and logged fixed ticks and radius growth.

## Environment

- Local run: 2026-10-09; repository work in progress from baseline `76139a5`.
- Windows 11 Pro build `26200`, NVIDIA GeForce RTX 4070 SUPER, NVIDIA driver `591.86`, `wgpu 30.0.1`, Vulkan backend.
- Rust 1.94.0, Cargo dev/test profile, two analytic objects, no presentation in the resolution test.
- GPU timestamp feature available on this adapter. No multi-adapter or Linux measurement yet.

## Metrics and results

| Resolution | CPU submit median | GPU compute median | Maximum primitive tests per image |
| --- | ---: | ---: | ---: |
| 1280×720 | 0.314 ms | 0.028 ms | 1,843,200 |
| 1920×1080 | 0.388 ms | 0.058 ms | 4,147,200 |
| 2560×1440 | 0.330 ms | 0.100 ms | 7,372,800 |

The CPU figures include per-draw buffer and texture creation, encoder recording, and queue submission. They are not total frame times. The GPU figures exclude presentation and readback, and are only five-sample smoke measurements. The non-monotonic CPU medians reflect measurement noise and host scheduling; no scaling claim follows from them. Native viewport logs at 1280×720 showed changing world ticks and radii, with sampled GPU compute durations varying roughly from `0.028` to `0.300 ms` as the sphere grew. These are local observations, not performance targets.

## Limitations

Only one Windows/Vulkan adapter was exercised. A local screenshot was inspected and a native resize to 1920×1080 was observed in logs. Keyboard controls, device loss, and extended runtime were not automatically exercised. The offscreen readback asserts computed center and background differ; the local window capture is stored under ignored `target/` and is not a release artifact. The GPU parity suite uses selected rays and explicit tolerances, not a statistical error bound. Debug-profile Rust timings and a two-object scene do not predict larger-world performance.

## Reproduction

```sh
cargo test -p analytic-renderer --test gpu --locked -- --ignored --nocapture
cargo run -p first-light --locked
```

Record the adapter, backend, driver, profile, resolution, object count, and raw samples for any new measurement. Do not compare runs with different growth radius as though geometry workload were identical.

## Conclusion

The first analytic GPU path and three requested resolutions worked on the recorded adapter. Cross-platform correctness, robust precision near degenerate cases, visual quality, device recovery, and useful performance at scale remain open.
