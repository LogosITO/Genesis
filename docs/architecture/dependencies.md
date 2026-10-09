# Dependency direction

```text
math
  ↑
field
  ↑
world ← simulation
             ↑
           runtime
```

Core direct dependencies: `field → math`; `world → math, field`; `simulation → math, field, world`; `runtime → simulation, world`. The original example uses public APIs from all five. Cargo enforces an acyclic workspace graph. `simulation` uses `field::Sphere` to validate new radii.

Foundation 0.4 uses Serde derives in math, field, world, and simulation for the bounded versioned save. `runtime` uses Serde JSON to serialize and strictly validate the full state. This adds data-format dependencies to the CPU crates but no GPU edge. World topology and source state stay in `world`; scheduling and step ordering stay in `simulation`/`runtime`.

Foundation 0.3 adds `renderer → math, field, world, wgpu` and `first-light → renderer, simulation, world, math, field, winit`. The renderer is a separate leaf. The `first-life` headless example uses the renderer's CPU snapshot API, which does not create a GPU device. `math`, `field`, `world`, `simulation`, and `runtime` have no GPU or window dependency. The original `hello-world` example does not depend on the renderer. The renderer's `Scene` is a frame snapshot, not a second authoritative world model.

Foundation 0.10 adds `authoring → math, serde_json` as an independent CPU leaf. `authoring-inspect → authoring` reads and compiles external definitions without GPU access. `first-light → authoring` is only an example-level edge for the static preview; the core world, simulation and runtime crates do not depend on authoring. The preview converts immutable typed segments into a disposable renderer `Scene`. No authoring code is part of the mutable `WorldState` or its save schema.

Foundation 0.10.1 adds `authoring → sha2` for SHA-256 of exact source bytes. The RustCrypto dependency is MIT OR Apache-2.0, supports the pinned Rust toolchain, and is used only in the CPU authoring leaf. No new world, simulation, renderer or release-workflow dependency edge is introduced.

Foundation 0.11 adds `world → authoring → math`, so validated authored definitions can be authoritative without a second world model. `simulation → world`, `runtime → simulation, world`, and `renderer → world` retain their direction. Compiled authoring structures and GPU/BVH buffers are derived; only exact definitions and occurrence state persist. The `authoring-inspect` example now exercises the same world, runtime, renderer and contact APIs headlessly.

Foundation 0.12 adds `authoring → field` so the bounded graph CPU evaluator reuses existing primitive formulas, and an explicit `renderer → authoring` edge for direct WGSL graph interpretation. This introduces no cycle: `field → math`, `world → authoring`, and neither authoring nor field depends on renderer. The `field-graph-preview` example depends on authoring and renderer but does not create a world instance. No GPU dependency is added to math, field, authoring, world, simulation, or runtime.
