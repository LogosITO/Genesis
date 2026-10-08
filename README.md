# Project Name TBD

Foundation 0.5 adds a bounded branching organism and analytic capsule connections. **Experimental / Research Stage.** The default First Life rule has at most two children per node, a shared resource budget, deterministic node-ID allocation, and version-2 saves that load valid version-1 state. The renderer shows overlapping primitives as separate solids; it does not provide watertight CSG geometry.

**Experimental / Research Stage.** A small foundation for mathematically defined mutable worlds. Analytic parameters, transforms, and growth rules are the source of truth; no mesh or voxel representation is stored. Foundation 0.4 adds a bounded growth graph driven by a continuous resource field, tick-indexed events, and versioned saves. The CPU math and simulation remain independently testable without a GPU. No floating-point intersection is formally certified.

## Run

Install Rust 1.94 via [rustup](https://rustup.rs/). The repository pins the toolchain.

```sh
cargo run -p hello-world --locked
```

Expected output: `{"entity_id":0,"ticks":4,"radius":1.5,"signed_distance":0.5}`. It is computed after four fixed steps, then a field query at `(2,0,0)`.

```sh
cargo run -p first-light --locked
```

`first-light` opens a native window with a growing world sphere and a static box. It requires a compatible Vulkan or DirectX 12 adapter. Controls: **A/D** or **Left/Right** orbit; **W/S** or **Up/Down** tilt; **Space** pause; **N** normals; **Esc** exit. See the [First Light guide](docs/architecture/first-light.md).

Run the First Life headless scenarios and native viewport:

```sh
cargo run -p first-life --locked -- baseline
cargo run -p first-life --locked -- changed
cargo run -p first-life --locked -- limited
cargo run -p first-life --locked -- baseline --measure
cargo run -p first-life --locked -- scale --measure
cargo run -p first-light --locked -- --life
```

The first three headless scenarios print JSON summaries after 120 fixed ticks. `changed` moves the resource source at tick 40; `limited` supplies too little resource to create a child. `--measure` adds local timing diagnostics on stderr. `scale --measure` prints controlled CPU step and snapshot medians for 1–48 nodes and a four-organism overflow case; it is not a full-frame benchmark. In native First Life mode, **M** schedules a source move at the next tick; **Space** pauses or resumes. The renderer copies node positions, connections, and sources from `WorldState`. See the [growth model](docs/specifications/growth-model.md), [capsule contract](docs/specifications/capsule.md), [save contract](docs/specifications/world-persistence.md), and [replay contract](docs/specifications/replay-determinism.md).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
```

On a machine with a compatible GPU, run the additional offscreen parity and resolution checks:

```sh
cargo test -p analytic-renderer --test gpu --locked -- --ignored --nocapture
```

The [architecture](docs/architecture/overview.md), [renderer design](docs/architecture/renderer.md), [CPU/GPU contract](docs/specifications/cpu-gpu-contract.md), [field contract](docs/specifications/mathematical-fields.md), [distance-bound proof and limits](docs/specifications/distance-bounds.md), [CPU spatial queries](docs/specifications/spatial-queries.md), [release policy](docs/architecture/releases.md), [roadmap](docs/roadmap/roadmap.md), and [research ledger](docs/research/references.md) are the canonical technical documents. The [site](site/README.md) imports these files instead of maintaining copies.

License: MIT OR Apache-2.0. No project brand, organization, domain, or release channel has been chosen.
