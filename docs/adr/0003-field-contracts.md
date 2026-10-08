# ADR 0003: Field contracts

## Status
Accepted for Foundation 0.1; CSG regularization wording corrected by ADR 0005.

## Context
Treating every signed field as an exact SDF can make geometric queries unsafe.

## Decision
Separate arbitrary scalar, signed, and exact-SDF traits. Sphere and axis-aligned box are exact SDF primitives. CSG has signed-set semantics only. No conservative distance estimate is claimed.

## Alternatives
A single `distance()` trait is simpler syntactically but mathematically misleading. A full interval or Lipschitz algebra is deferred pending research.

## Consequences
Consumers cannot assume CSG sample magnitude is safe for distance-based stepping. A future conservative estimator needs its own verified contract.
