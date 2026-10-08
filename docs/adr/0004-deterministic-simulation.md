# ADR 0004: Deterministic simulation

## Status
Accepted for Foundation 0.1.

## Context
Wall-clock updates and partial mutation make results difficult to reproduce.

## Decision
Use a validated fixed duration, integer tick count, insertion-ordered entities, and all-or-nothing radius updates. Record a seed, but do not invent randomness for a deterministic growth rule.

## Alternatives
Variable frame time and an ECS scheduler are unnecessary for the current example. Fixed-point arithmetic is deferred until cross-platform bitwise reproduction is required.

## Consequences
Repeated local runs are predictable. Floating point results can differ between environments; step duration and operation order are part of the model.
