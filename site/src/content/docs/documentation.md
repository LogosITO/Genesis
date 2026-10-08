---
title: Documentation
---

Run the vertical slice from the repository root:

```sh
cargo run -p hello-world --locked
```

The output is computed after four steps and a signed-distance query. Start with the [field contracts](../reference/specifications/mathematical-fields/), [world state](../reference/specifications/world-state/), and [simulation time](../reference/specifications/simulation-time/). API docs are generated from Rust source with `cargo doc --workspace --no-deps --locked`.
