---
title: Documentation
---

Run the vertical slice from the repository root:

```sh
cargo run -p hello-world --locked
```

The output is computed after four steps and a signed-distance query. Start with the [field contracts](../reference/specifications/mathematical-fields/), [distance bounds](../reference/specifications/distance-bounds/), [gradients](../reference/specifications/gradients/), [spatial queries](../reference/specifications/spatial-queries/), [world state](../reference/specifications/world-state/), and [simulation time](../reference/specifications/simulation-time/). API docs are generated from Rust source with `cargo doc --workspace --no-deps --locked`.

For the experimental GPU viewport, use `cargo run -p first-light --locked`; see the [First Light guide](../reference/architecture/first-light/) and [CPU/GPU contract](../reference/specifications/cpu-gpu-contract/). GPU tests are opt-in and require a compatible native adapter.

For the experimental growth system, run `cargo run -p first-life --locked -- baseline`, `changed`, or `limited`. Each prints a machine-readable summary. Use `cargo run -p first-light --locked -- --life` for the native viewport and press **M** to move its resource source. See the [growth model](../reference/specifications/growth-model/), [environment field](../reference/specifications/environment-fields/), [persistence](../reference/specifications/world-persistence/), and [replay](../reference/specifications/replay-determinism/) specifications.

Foundation 0.5 adds bounded branching and analytic capsule connections. Run `cargo run -p first-life --locked -- scale --measure` for a local CPU scaling sample. The [capsule contract](../reference/specifications/capsule/) and [First Structure measurements](../reference/research/first-structure-measurements/) record limits and evidence. The world can exceed the GPU snapshot budget; overflow is reported explicitly.
