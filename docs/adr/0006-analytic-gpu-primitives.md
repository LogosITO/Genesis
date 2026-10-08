# ADR 0006 — Analytic GPU primitives before general field tracing

## Status

Accepted for the Foundation 0.3 experiment.

## Context

The CPU model distinguishes exact primitive SDFs from sign-only CSG expressions and ideal distance-to-zero-set bounds. A direct GPU sphere tracer would inherit uncertified floating-point steps and could present CSG zero sets without solid interiors as physical surfaces. The project needs one visible, testable GPU slice now.

## Decision

Render only spheres and axis-aligned boxes with analytic ray intersections in WGSL. Keep primitive parameters and growth in the CPU world, build a bounded renderer-owned snapshot, and use a compute texture plus image-presentation triangle. Supply a CPU analytic reference and opt-in offscreen GPU comparison. Do not expose general CSG as renderable solid geometry.

## Alternatives

- General sphere tracing from `IdealDistanceBound`: rejected for this stage because `f64` and `f32` values are not certified conservative numeric steps, and CSG zero-set semantics need more work.
- Mesh or voxel conversion: rejected as an authoritative representation contrary to the mathematical world model.
- CPU image rendering only: useful for reference, but does not test the intended GPU path.

## Consequences

The slice is small and directly comparable, and core tests remain GPU-free. It supports only two primitives, a fixed object cap, positive uniform scale, and `f32` GPU precision. Boxes are renderer-only until the world model gains a reasoned persistent box entity. More geometry requires a new contract and evidence, not an implicit extension of the current one.
