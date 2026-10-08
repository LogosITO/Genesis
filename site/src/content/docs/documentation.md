---
title: Documentation
---

Run the vertical slice from the repository root:

```sh
cargo run -p hello-world --locked
```

The output is computed after four steps and a signed-distance query. Start with the [field contracts](../reference/specifications/mathematical-fields/), [distance bounds](../reference/specifications/distance-bounds/), [gradients](../reference/specifications/gradients/), [spatial queries](../reference/specifications/spatial-queries/), [world state](../reference/specifications/world-state/), and [simulation time](../reference/specifications/simulation-time/). API docs are generated from Rust source with `cargo doc --workspace --no-deps --locked`.

For the experimental GPU viewport, use `cargo run -p first-light --locked`; see the [First Light guide](../reference/architecture/first-light/) and [CPU/GPU contract](../reference/specifications/cpu-gpu-contract/). GPU tests are opt-in and require a compatible native adapter.
