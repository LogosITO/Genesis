# First playable local experiment — Foundation 0.9

## Hypothesis

An authoritative analytic world with finite shared resources, stable semantic identities, tick-indexed events, and swept-sphere contact can support a short understandable objective without making the renderer or GPU authoritative.

## Scenario and method

**The Passage** starts after 45 deterministic growth ticks. Two organisms draw on one finite source. Ten ordinary analytic spheres form two lane walls. The central green organism blocks an orange kinematic sphere travelling along the lane. A renderer-only magenta sphere marks the goal. The player moves the source, selects and prunes the central first branch, then moves the body into the goal region. The objective checks the authoritative body centre, source location, and central branch absence; merely reaching a rendered marker is insufficient. The lane can be walked around, so this explicit objective guard matters.

The automated test starts from the same level constructor used by the native viewport. It checks a ray pick against a stable branch ID, rejects root pruning, moves the body until contact blocks it, checks the contact scene before and after the cut, schedules source movement before pruning on the same tick, saves pending events in v5, reloads and replays exactly on this host, verifies growth of the second organism after source movement, reaches the goal, and reconstructs a fresh initial runtime for restart. It prints a JSON success record. The native app uses the existing analytic GPU renderer, CPU semantic picking, and the same `advance_life_cached` event/contact path. The gameplay objective and HUD live only in the example.

## Environment

- Local Windows x86_64, Rust 1.94, release profile, `wgpu` 30.0.1.
- NVIDIA GeForce RTX 4070 SUPER, Vulkan backend, NVIDIA driver as reported by `wgpu`.
- Native window, measured with `GENESIS_MEASURE=1`; one active desktop with other applications, no controlled GPU clock or presentation mode.
- Package copied to a temporary directory outside the Git repository and launched there. It opened an interactive GPU window and loaded its compiled-in shader/font data.

## Results

- Headless playthrough: **PASS**, first success at tick 190, body `z = 1.270518854291677` inside the radius-0.25 goal region, determined from authoritative world state. It started blocked at `z < 0`, then crossed after source move and prune. Save/replay equality and restart equality passed on this host.
- Native viewport: window opened, a real rendered frame showed the orange body, blue analytic walls, two colored organisms, yellow source, magenta goal, and readable text HUD. Clicking a visible green node selected its semantic `GrowthNode` identity; sending **P** removed a subtree from the world (reported node count fell from 14 to 9). **R** restored the initial tick/state; **Space** paused it. The portable executable also opened from outside the repository. A complete human-driven start-to-goal sequence was **not verified**.
- CPU `--passage-measure`, release profile, one local run: fixed-step simulation median/p95 **0.0061/0.0113 ms** (120 samples); fresh contact BVH build **0.0079/0.0107 ms** (100); analytic contact query **0.0008/0.0009 ms** (1000); renderer snapshot **0.0035/0.0039 ms** (100). The query is an isolated deterministic sweep, not a separately instrumented part of each runtime tick.

Native steady-scene samples below come from approximately two-second windows after growth reached 29 total nodes and the GPU BVH had rebuilt twice. The window was resized externally; scene and camera stayed unchanged. `frame_ms` measures time inside the redraw callback through queue presentation and sampled timer readback. It excludes idle time between callbacks and is **not** a display-frame pacing or FPS guarantee. GPU values are timestamped compute-pass durations.

| Inner resolution | Frame samples | Redraw median/p95 ms | GPU compute median/p95 ms | Simulation median/p95 ms | Snapshot median/p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1280×720 | 330 | 5.597 / 5.902 | 0.183 / 0.232 | 0.021 / 0.028 | 0.009 / 0.014 |
| 1920×1080 | 330 | 4.106 / 4.672 | 1.090 / 1.939 | 0.019 / 0.028 | 0.009 / 0.013 |
| 2560×1440 | 330 | 4.401 / 5.204 | 0.834 / 1.730 | 0.022 / 0.030 | 0.010 / 0.015 |

The non-monotonic GPU timings make these unsuitable for scaling conclusions. The active desktop, window occlusion, graphics clocks, driver scheduling, and asynchronous presentation were not controlled. The last reported upload enqueue and BVH build times were zero in the steady scene because no geometry changed; those values do not measure the initial upload cost. The renderer maintained the BVH rather than rebuilding it on every frame.

## Limitations and known problems

- A full manual start-to-goal playthrough with mouse, movement, pause, focus loss, resource move, restart, and close as one sequence remains **NOT VERIFIED**. Individual native interactions above were exercised, and the deterministic headless playthrough covers the authoritative systems.
- The camera can hide the low central stem behind overlapping wall spheres. Arrows can change the view; a usability playtest should determine whether the stem is obvious enough.
- GPU render primitives can overlap and are not a watertight union. CPU contact has no sliding or depenetration; near-tangent and overlapping-solid behavior retains the documented numerical limits.
- The goal marker is renderer-only and non-colliding. The objective uses world state. Horizontal-only movement is a demo rule.
- No external user has tested the level. Hosted Windows and Linux CI are not confirmed by this local experiment. Cross-platform bitwise replay is not claimed.

## Reproduction

```sh
cargo test -p first-light passage_playthrough --release --locked -- --nocapture
cargo run --release -p first-light --locked -- --passage-measure
cargo run --release -p first-light --locked -- --passage
```

On Windows, `powershell -ExecutionPolicy Bypass -File tools/package-passage.ps1` creates the portable folder. Run `PLAY.cmd` there. `GENESIS_MEASURE=1` enables per-window timing logs on standard error; default play does not block every frame for GPU timestamp readback.

## Planned user testing

Give two testers a clean portable package and only the in-window HUD. Record whether they identify the orange body, magenta goal, green stem and yellow source; whether they discover the needed order of **M**, click, **P**, movement; time to completion; invalid clicks; camera changes; restart use; and whether success feels tied to the world. Capture their screens with consent. A failed discovery or blocked route should lead to a small level/HUD correction before a public playable claim.

## Conclusion

The local engine supports a deterministic, independently launched analytic interaction loop and a verifiable win condition. Native rendering and some controls worked on the reference Windows/Vulkan machine. The evidence is insufficient to call this a publicly validated playable release.
