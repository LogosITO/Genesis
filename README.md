# Genesis

**Experimental / Research Stage.** Genesis stores analytic parameters, transforms, growth rules, finite resource reservoirs, and one kinematic body as authoritative world state; no mesh or voxel geometry is stored. Foundation 0.8 lets bounded organisms compete for shared finite sources and persists the result in version-5 saves. CPU math, simulation, and contact remain testable without a GPU. This is neither validated biology nor a general physics engine.

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
cargo run -p first-life --locked -- pruning
cargo run -p first-life --locked -- pruning-replay
cargo run -p first-life --locked -- contact
cargo run -p first-life --locked -- contact-pruned
cargo run -p first-life --locked -- contact-replay
cargo run -p first-life --locked -- ecology isolated
cargo run -p first-life --locked -- ecology competition
cargo run -p first-life --locked -- ecology separated
cargo run -p first-life --locked -- ecology environment-change
cargo run -p first-life --locked -- ecology pruning
cargo run -p first-life --locked -- ecology replay
cargo run --release -p first-life --locked -- ecology measure
cargo run -p first-life --locked -- baseline --measure
cargo run -p first-life --locked -- scale --measure
cargo run -p first-light --locked -- --life
```

The original headless scenarios print JSON after 120 fixed ticks. `contact` stops at a branch, `contact-pruned` passes through its cleared path, and `contact-replay` verifies save/load. The six `ecology` scenarios print JSON after 40 ticks with organism IDs, topology, source balances, allocation, and a state fingerprint. `ecology measure` reports local release-profile CPU timings for one, two, and four organisms. In native First Life mode, two differently colored organisms share one finite source; the title shows their node counts, remaining stock, allocation, and tick. **Left-click** selects a node or connection, **P** queues its cut, **M** moves the source, **I/J/K/L** move the body, and **Space** pauses or resumes. Ambiguous picks cannot authorize a cut. See the [allocation contract](docs/specifications/resource-allocation.md), [ecosystem state](docs/specifications/ecosystem-state.md), [local experiment](docs/research/first-ecology-experiment.md), [contact contract](docs/specifications/kinematic-contact.md), and [save contract](docs/specifications/world-persistence.md).

Play the local **The Passage** prototype:

```sh
cargo run --release -p first-light --locked -- --passage
cargo test -p first-light passage_playthrough --locked -- --nocapture
```

The orange body must cross a growing analytic branch after moving a finite resource source and pruning the stem. An in-window HUD shows controls and feedback; the headless playthrough prints a JSON success record. See the [player guide](docs/guides/the-passage.md), [manual QA checklist](docs/guides/playtest-qa.md), and [research record](docs/research/first-playable.md). On Windows, `powershell -ExecutionPolicy Bypass -File tools/package-passage.ps1` creates and checks a local ZIP under `target/dist/`; it is not published.

Inspect an external mathematical definition on the CPU, then display it as analytic capsules:

```sh
cargo run --release -p authoring-inspect --locked -- examples/authoring/branch-a.json
cargo run --release -p first-light --locked -- --authoring examples/authoring/branch-a.json
```

Edit the JSON and press **R** in the preview to reload without recompiling. The static structure is separate from living `WorldState` organisms. A second definition, controls, and limits are in the [authoring example](examples/authoring/README.md) and [format contract](docs/specifications/mathematical-authoring.md).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
```

On a machine with a compatible GPU, run the additional offscreen parity and resolution checks:

```sh
cargo test -p analytic-renderer --test gpu --locked -- --ignored --nocapture --test-threads=1
```

The [architecture](docs/architecture/overview.md), [renderer design](docs/architecture/renderer.md), [resource allocation](docs/specifications/resource-allocation.md), [contact specification](docs/specifications/kinematic-contact.md), [spatial acceleration](docs/architecture/spatial-acceleration.md), [CPU/GPU contract](docs/specifications/cpu-gpu-contract.md), [field contract](docs/specifications/mathematical-fields.md), [release policy](docs/architecture/releases.md), [roadmap](docs/roadmap/roadmap.md), and [research ledger](docs/research/references.md) are canonical technical documents. The [site](site/README.md) imports these files instead of maintaining copies.

License: MIT OR Apache-2.0.
