# Project Name TBD

**Experimental / Research Stage.** A small, CPU-only foundation for mathematically defined mutable worlds. The current source of truth is analytic sphere parameters, transforms, and growth rules; no mesh or voxel representation is stored. Foundation 0.2 adds CPU reference gradients, ideal distance-to-zero-set bounds, and bounded ray queries for exact primitives. These are not certified floating-point GPU tracing guarantees.

## Run

Install Rust 1.94 via [rustup](https://rustup.rs/). The repository pins the toolchain.

```sh
cargo run -p hello-world --locked
```

Expected output: `{"entity_id":0,"ticks":4,"radius":1.5,"signed_distance":0.5}`. It is computed after four fixed steps, then a field query at `(2,0,0)`.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
```

The [architecture](docs/architecture/overview.md), [field contract](docs/specifications/mathematical-fields.md), [distance-bound proof and limits](docs/specifications/distance-bounds.md), [CPU spatial queries](docs/specifications/spatial-queries.md), [roadmap](docs/roadmap/roadmap.md), and [research ledger](docs/research/references.md) are the canonical technical documents. The [site](site/README.md) imports these files instead of maintaining copies.

License: MIT OR Apache-2.0. No project brand, organization, domain, or release channel has been chosen.
