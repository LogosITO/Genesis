# First Light example

Run from the repository root after installing the pinned Rust toolchain:

```sh
cargo run -p first-light --locked
```

The native window starts at 1280×720 with one growing blue sphere from `WorldState` and one static orange analytic box. The sphere grows by `0.12` local radius units per simulated second. The simulation uses fixed `16,666,667 ns` steps; rendering observes the latest completed state. Pausing stops simulation steps, not camera redraws. A four-step catch-up cap discards excess wall-time debt after prolonged slow frames, so the example is deterministic for a given number of completed ticks but does not promise to catch up to real time under sustained overload.

Controls: **A/D** or **Left/Right** orbit, **W/S** or **Up/Down** tilt, **Space** pause/resume, **N** normal debug, **Esc** exit. Resize the window to test other resolutions. Zero-sized windows skip drawing. The console prints adapter, backend, driver, frame rate, CPU submission time, optional GPU compute time, resolution, object count, upper bound on primitive tests, ticks, and current radius every two seconds.

Run `cargo run -p first-light --locked -- --life` for the living organism. In this mode, **left-click** selects a node or parent-child connection through a bounded CPU analytic ray query. The window title and stderr identify the selected target and tick. **P** queues an authoritative `PruneBranch` event for the next tick; **M** moves the source. Pause with **Space** before a careful selection, then resume to apply the cut. Root, source, empty, ambiguous, and stale selections cannot prune. See the [First Interaction contract](../specifications/first-interaction.md).

The same mode now includes an orange kinematic sphere. Hold **I/K** to move it along positive/negative Z and **J/L** along negative/positive X; key presses and releases change desired velocity on the next fixed tick. The title displays the current collider, if any. Move toward a branch, pause and select it, press **P**, resume, then move through the cleared space. Collision stops motion; it does not slide. The body and branch geometry both come from `WorldState`. See [kinematic contact](../specifications/kinematic-contact.md).

To run GPU parity and offscreen tests explicitly:

```sh
cargo test -p analytic-renderer --test gpu --locked -- --ignored --nocapture
```

The ordinary workspace suite validates WGSL and math without a GPU. The opt-in test needs a native adapter; a missing adapter is a test failure, not a reported pass. For repeatable measurements and caveats see [First Light benchmark](../research/first-light-benchmark.md). The source is `examples/first-light/src/main.rs`.
