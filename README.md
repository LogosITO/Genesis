# Project Name TBD

**Experimental / Research Stage.** A small foundation for mathematically defined mutable worlds. Analytic parameters, transforms, and growth rules are the source of truth; no mesh or voxel representation is stored. Foundation 0.3 adds an optional native GPU viewport for spheres and axis-aligned boxes. The CPU math and simulation remain independently testable without a GPU. No floating-point intersection is formally certified.

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
