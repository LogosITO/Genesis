# ADR 0005: Lipschitz and CSG query boundaries

## Status
Accepted for Foundation 0.2. Corrects the regularized-solid wording in ADR 0003 and the 0.1 field specification.

## Context
`min`/`max` CSG preserves sign and Lipschitz bounds, but `A difference A` leaves a zero shell without occupied volume. Floating point evaluation also does not certify a conservative numerical step.

## Decision
Keep `Field` as a signed expression and explicitly separate its zero set from regularized occupied solids. Add `LipschitzField` and an `IdealDistanceBound` type for exact-real-arithmetic reasoning. Return explicit undefined gradients at singularities and ties. Restrict the bounded CPU ray query to exact SDF primitives and give it an `Indeterminate` status.

## Alternatives
A boolean exact-SDF flag would allow invalid combinations. Treating every zero sample as a surface would produce ghost hits. General CSG regularization, interval arithmetic, and GPU tracing are deferred until their guarantees can be tested.

## Consequences
Primitive ray queries and ideal zero-set bounds are reproducible CPU references. General CSG can provide field samples and ideal zero-set bounds but cannot yet certify occupied-solid boundaries or safe floating-point GPU steps.
