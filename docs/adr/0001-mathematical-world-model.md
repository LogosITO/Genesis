# ADR 0001: Mathematical world model

## Status
Accepted for Foundation 0.1.

## Context
The project requires geometry whose parameters and rules remain authoritative as the world changes.

## Decision
Store analytic sphere parameters and transforms as world state. Query fields directly. Meshes and voxels are not the authoritative geometry representation.

## Alternatives
Mesh-first and voxel-first models were considered unsuitable for this stated source-of-truth requirement. A general function graph is deferred until more use cases exist.

## Consequences
The slice is small and CPU-testable. Rendering, broad geometry coverage, acceleration, and persistence remain open work.
