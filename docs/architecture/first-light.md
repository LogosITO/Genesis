# First Light example

Run from the repository root after installing the pinned Rust toolchain:

```sh
cargo run -p first-light --locked
```

The native window starts at 1280×720 with one growing blue sphere from `WorldState` and one static orange analytic box. The sphere grows by `0.12` local radius units per simulated second. The simulation uses fixed `16,666,667 ns` steps; rendering observes the latest completed state. Pausing stops simulation steps, not camera redraws. A four-step catch-up cap discards excess wall-time debt after prolonged slow frames, so the example is deterministic for a given number of completed ticks but does not promise to catch up to real time under sustained overload.

Controls: **A/D** or **Left/Right** orbit, **W/S** or **Up/Down** tilt, **Space** pause/resume, **N** normal debug, **Esc** exit. Resize the window to test other resolutions. Zero-sized windows skip drawing. The console prints adapter, backend, driver, frame rate, CPU submission time, optional GPU compute time, resolution, object count, upper bound on primitive tests, ticks, and current radius every two seconds.

To run GPU parity and offscreen tests explicitly:

```sh
cargo test -p analytic-renderer --test gpu --locked -- --ignored --nocapture
```

The ordinary workspace suite validates WGSL and math without a GPU. The opt-in test needs a native adapter; a missing adapter is a test failure, not a reported pass. For repeatable measurements and caveats see [First Light benchmark](../research/first-light-benchmark.md). The source is `examples/first-light/src/main.rs`.
