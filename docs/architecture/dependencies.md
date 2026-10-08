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

Actual direct dependencies: `field → math`; `world → math, field`; `simulation → math, field, world`; `runtime → simulation, world`. The example uses public APIs from all five. Cargo enforces an acyclic workspace graph. No crate depends on GPU, UI, or a renderer. `simulation` uses `field::Sphere` to validate new radii. A future renderer should depend on field/world interfaces, never the reverse.
