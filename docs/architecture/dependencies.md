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

Foundation 0.3 adds `renderer → math, field, world, wgpu` and `first-light → renderer, simulation, world, math, field, winit`. The renderer is a separate leaf. `math`, `field`, `world`, `simulation`, and `runtime` have no GPU or window dependency. The original `hello-world` example does not depend on the renderer. The renderer's `Scene` is a frame snapshot, not a second authoritative world model.
