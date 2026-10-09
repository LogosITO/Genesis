# Foundation 0.9.6 local runtime verification

## Established knowledge and decision

The Passage retains authoritative CPU `WorldState`, fixed-tick simulation, analytic GPU direct/BVH paths, and versioned persistence. This stage changes only native lifecycle handling, local measurement, and verification. The native lifecycle policy is specified in [native runtime](../specifications/native-runtime.md). Device-loss recovery remains partial: the current safe response is diagnostic exit.

## Environment and reproduction

Local Windows run on NVIDIA GeForce RTX 4070 SUPER, driver 591.86, Vulkan backend, `wgpu 30.0.1`, Rust 1.94.0, release profile, Fifo present mode. Start at `cd56ae1ee107`. Run `cargo build -p first-light --release --locked`, then set `GENESIS_MEASURE_SCENARIO` to `idle`, `growth`, or `interaction`, set `GENESIS_WINDOW_SIZE` to `1280x720`, `1920x1080`, or `2560x1440`, and run `target/release/first-light --passage`. PowerShell example:

```powershell
$env:GENESIS_MEASURE_SCENARIO='interaction'
$env:GENESIS_WINDOW_SIZE='1920x1080'
.\target\release\first-light.exe --passage 2> target\native-interaction-1080.log
```

Exit with Escape. JSON lines with `kind=native-frame-window` are machine readable. Raw local logs from this run are under ignored `target/native-096-*.log`; they are not repository fixtures.

## Actual native measurements

The table uses the **last** approximately two-second window from each run, after earlier warmup windows. Different tick ranges and GPU timestamp sample counts limit between-row comparisons. Each row has 121–122 frame samples and a Fifo presentation limit near 60 Hz. Values are milliseconds. `interval` shows p50/p95/p99; `step` and `GPU` show p50/p95. GPU timestamps are sparse and should not be read as stable p95 estimates.

| Scenario | Actual client size | Tick, steps | Interval p50/p95/p99 | Loop p50/p95 | Acquire p50 | Step p50/p95 | GPU p50/p95 (n) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Idle | 1280×720 | 45, 0 | 16.58 / 19.05 / 22.28 | 16.44 / 18.87 | 15.85 | — | 1.38 / 1.48 (6) |
| Growth | 1280×720 | 492, 121 | 16.64 / 17.51 / 18.02 | 16.49 / 17.17 | 15.97 | 0.031 / 0.038 | 0.33 / 0.42 (5) |
| Interaction | 1280×720 | 125, 119 | 16.49 / 18.52 / 19.35 | 16.37 / 17.81 | 15.72 | 0.027 / 0.045 | 0.51 / 0.59 (5) |
| Interaction | 1920×1080 | 132, 119 | 16.55 / 19.04 / 19.46 | 16.44 / 18.67 | 15.90 | 0.029 / 0.050 | 1.08 / 1.87 (5) |
| Interaction | 2560×1440 | 131, 119 | 16.66 / 19.12 / 20.22 | 16.49 / 18.44 | 15.94 | 0.028 / 0.046 | 1.55 / 7.94 (5) |

The 1440p GPU p95 includes a high outlier in only five samples. Idle GPU time also varied considerably across windows. No GPU speedup or regression is inferred. The large acquisition wait means these Fifo frame intervals mostly show presentation pacing. The interaction 720p run repeated restart cycles without a crash after fixing a discovered duration-underflow panic: restart cleared the accumulator inside a fixed-step loop before that loop subtracted its step. CPU contact and allocation phase timings are available from isolated opt-in release tests, but are **not** separated within the native frame measurement. Input-to-photon latency and display scan-out were not measured.

## Resize, rendering, and input evidence

Native window automation moved the running interaction window through outer sizes 1920×1080, 2560×1440, 720×1280, and 1280×720, then minimized and restored it. The logged **client** sizes were 1904×1041, 2544×1401, 704×1241, and 1264×681 after OS decorations. A zero-size skip was logged during minimize. Simulation ticks continued after restore, and there was no panic, device error, or surface-retry exhaustion. These observations verify the window lifecycle on this one machine, not pixel identity on its presentation surface. Repeated rapid dragging, simulated driver reset, and a real surface `Lost` event were not produced.

The opt-in `passage_gpu_state_images_match_direct_and_bvh` test performed actual offscreen GPU readback for ten state snapshots including source off/on, pause, prune, movement, and restart. Direct and BVH RGB bytes agreed for every state on this adapter. Sequential 1280×720, 1920×1080, 2560×1440, 720×1280, and return-to-1280×720 readbacks also agreed and had expected dimensions. It checked a changed camera ray aspect ratio. Captured BMPs are in ignored `target/passage-captures-096/`. These are real GPU outputs, but they do not prove the presented window image after an OS resize.

The ordinary Passage input tests verify event-state effects for movement keys, camera controls, picking, pruning, source movement/toggle, pause, restart, and focus-loss key clearing. The new mixed stress test verifies two identical application runs through resource toggles, pruning, kinematic movement, pause/resume, restart, scene identity, world validation, and save/load. It does not inject genuine OS mouse/keyboard events. Native OS-level input and visual human playtest remain **NOT VERIFIED**.

## Opt-in test audit: 22 tests

Run all supported local opt-in tests sequentially with `cargo test --release --workspace --locked -- --ignored --test-threads=1`. This Windows/Vulkan run passed **22/22** (18 GPU, four CPU timing). No test requires a native window. The GPU group requires a compatible native adapter; it stays ignored in ordinary CI. Local CPU timing tests should run in release and serially. Test names below are the actual `--ignored` inventory:

| Group | Tests | Result |
| --- | --- | --- |
| Renderer GPU correctness/readback (11) | `gpu_bvh_broad_real_scene_differential`, `gpu_bvh_analytic_edge_cases`, `gpu_bvh_rebuilds_after_growth_and_source_movement`, `gpu_analytic_parity_and_image_readback`, `gpu_resolution_and_timestamp_smoke`, `gpu_first_life_snapshot_matches_cpu`, `gpu_real_381_scene_and_dynamic_source`, `gpu_body_snapshot_follows_authoritative_fixed_step`, `gpu_ecology_snapshot_matches_authoritative_growth`, `gpu_bvh_offscreen_image_parity`, `gpu_first_scale_reference_captures` | PASS 11/11 |
| Renderer GPU timing (5) | `gpu_pruning_update_benchmark`, `gpu_growth_scaling_measurements`, `gpu_fixed_scene_scale_benchmark`, `gpu_real_381_resolution_benchmark`, `gpu_bvh_comparison_benchmark` | PASS 5/5 |
| Passage GPU readback/timing (2) | `passage_gpu_state_images_match_direct_and_bvh`, `passage_gpu_resolution_measurement` | PASS 2/2 |
| CPU timing (4) | `cpu_real_381_bvh_benchmark`, `interaction_cpu_benchmark`, `contact_cpu_benchmark`, `finite_allocation_and_growth_commit_benchmark` | PASS 4/4 |

The CPU tests measure isolated phases. In this run, four-organism allocation p50/p95 was 0.0010/0.0011 ms and growth commit 0.0014/0.0017 ms. For 380 solids, contact BVH query was 0.0020/0.0028 ms, rebuild 0.0588/0.0910 ms, and cached tick 0.0329/0.0645 ms. They are not additive native-frame components.

## Limits and open questions

Fault-injection unit tests cover surface result classification and retry count, not a real driver fault. Device recreation, real device-loss recovery, cross-adapter rendering, input-to-photon latency, full native phase breakdown, rapid user-driven resize, and independent playtesting remain unverified or unimplemented. The local logs and BMPs are ignored diagnostic artifacts; future repeatable publication should package selected evidence separately after review. Standard CI cannot execute native GPU or window verification on generic runners.
