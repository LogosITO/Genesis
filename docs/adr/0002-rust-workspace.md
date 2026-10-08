# ADR 0002: Rust workspace

## Status
Accepted for Foundation 0.1.

## Context
Math, field semantics, world state, simulation, and embedding have distinct dependency boundaries.

## Decision
Use five small Rust crates in one Cargo workspace with edition 2024 and pinned toolchain. Use the standard library; no external Rust dependency is needed for this slice.

## Alternatives
A monolithic crate would reduce files but obscure the required dependency direction. GPU and plugin crates are deferred until real implementations exist.

## Consequences
The core builds without graphics hardware. Crate boundaries add modest workspace overhead. Cargo.lock is committed for repeatable examples and CI.
