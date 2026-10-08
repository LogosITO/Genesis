# Simulation time and determinism

`SimulationStep` is a finite positive duration in seconds. `SimulationTime` counts completed steps as `u64`; no wall clock participates. On each tick every radius becomes `radius + growth_per_second × step_seconds`. New radii are checked before committing, then the tick counter advances. Tick overflow or invalid radius leaves world and clock unchanged.

For identical initial state, step, executable, and floating point environment, the same ordered operations yield repeatable results. Floating point rounding accumulates over steps; arbitrary target platforms and compiler settings are not promised bitwise identical. The seed is reserved for future explicitly seeded rules; there is no stochastic behavior in this slice.
