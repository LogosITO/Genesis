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
