# Mathematical structure authoring format v1

## Scope

This is a **static structural grammar**. It is neither a scalar field nor an ecological growth rule. The source JSON is authoritative for generation; the generated `Structure` contains analytic capsule endpoints, radii and parent relationships. A renderer `Scene` is a disposable snapshot. Foundation 0.11 can pin exact source bytes in `WorldState` as separate authored occurrences; these are not organisms. See the [instance contract](authored-world-instances.md).

Two runnable definitions and commands are in `examples/authoring/README.md`. The format is independent of the GPU API. `world-authoring` has no renderer dependency.

## Schema

Version 1 requires `format_version: 1`, a lowercase ASCII `id` (1–64 characters, starting with a letter), positive `revision`, `axiom`, `rules`, `iterations`, `step_length`, `yaw_degrees`, `pitch_degrees`, `branch_radius`, and `budgets`. Optional `instance.origin` is a three-component world-space translation and defaults to `[0,0,0]`. Unknown fields, duplicate JSON fields, malformed UTF-8/JSON, unsupported versions, nonfinite values, and invalid ranges are rejected. A `revision` is an author-supplied label, **not** a content hash or proof that two files have identical meaning.

Each rule is `{ "symbol": "X", "replacement": "..." }`; its left side is one ASCII uppercase letter. No two rules may use the same left side. `F` may itself have a rule. Other uppercase symbols are inert at geometric interpretation and must have a rule. Replacement strings may be empty. Every axiom and individual replacement must have balanced square brackets; the final expansion is also interpreted with checked pushes/pops. This restriction avoids rule fragments whose brackets rely on other rules to close them.

## Parallel rewriting and typed geometry

At each iteration, scan the previous string left to right. Replace each symbol with its rule's full right side when one exists; copy it otherwise. Replacements generated in an iteration are **not** processed until the next iteration. After exactly `iterations` passes, scan the final stream once with the turtle below. Expansion and drawing order are deterministic. There is no random seed because v1 has no stochastic choice.

The turtle begins at `instance.origin` with heading `+Y`, local right `+X`, and local up `+Z` in the project's right-handed coordinates. `F` draws one closed-segment analytic capsule from the current point to `point + heading × step_length`, then advances. `+` and `-` rotate heading toward/away from local right around local up by `yaw_degrees`; `&` and `^` rotate heading toward/away from local up around local right by `pitch_degrees`. Angles are degrees in the file and converted to radians for evaluation. `[` saves position, orientation and the current parent segment; `]` restores them. Variables draw nothing. The branch radius is constant for v1. A nonempty drawing is required.

Each `F` allocates a one-based `Segment.id` in depth-first interpretation order. This number is a **renderer draw ID**, not a persistent semantic identity. `Segment.parent` is the previous draw ID in that branch; `Segment.parent_identity` is its structural identity. Branch return restores both. The typed `Segment` record stores world-space endpoints and radius. Preview conversion uses `analytic_field::Capsule` and `analytic_renderer::Primitive::capsule` with the draw ID. Capsule overlap is **not** claimed to form a watertight CSG solid.

## Identity and provenance

`Definition.id` is the author's logical name. The JSON `revision` is an author-declared positive label; neither field identifies exact bytes. `Structure::content_revision()` is SHA-256 over the **exact file bytes**, including whitespace and key order. `COMPILER_SEMANTICS_VERSION = 1` identifies this implementation's rewrite, turtle and numerical contract. A durable definition reference is `(logical id, content SHA-256, compiler semantics version)`; the logical id is descriptive and the digest must be verified against supplied bytes before they can be used. A digest does not make untrusted content safe; all validation and budgets still apply.

`Segment.identity` records the zero-based axiom byte position, followed by the child byte position in each replacement that produced the final `F`. Copied symbols keep their path. Replacement symbols append one child position. At most eight rewrite steps and 1,024 replacement bytes keep every path bounded; every emitted `F` has a distinct path even when two capsules have identical endpoints. `SegmentReference` scopes the path with content SHA-256 and compiler semantics version. `Structure::segment(reference)` rejects a different revision or semantics version. Paths do not promise correspondence across revisions, even when a path happens to be equal after an edit. GPU buffer order and BVH construction do not enter the identity.

The native preview also tracks a monotonic `PreviewGeneration`. A successful changed-byte reload increments it and clears preview selection. A selection carries generation, revision and segment path; resolving a stale reference fails. An unchanged-byte R reload returns without compilation, scene replacement or BVH rebuild, and retains selection. A changed-byte reload compiles, validates the renderer scene and builds the CPU BVH before committing; on error the active preview remains intact. The renderer compares primitive data and rebuilds its GPU BVH when geometry changes. Byte changes that generate identical GPU primitives need no GPU buffer/BVH rebuild. An actual GPU allocation or device failure after commit is subject to the existing renderer failure policy; hardware allocation cannot be made transactional with CPU state.

## Budgets and failures

Input is read with a 65,537-byte cap and rejected above 65,536 bytes. The author must choose positive budgets no higher than the hard limits below. No partial structure is returned when validation or compilation fails.

| Limit | Hard ceiling |
| --- | ---: |
| Rewrite iterations | 8 |
| Rules | 16 |
| Replacement length | 1,024 ASCII bytes |
| Expanded symbols | 65,536 |
| Active branch stack | 64 |
| Generated capsule segments | 1,024 |
| Visited symbols across rewrite and interpretation | 1,000,000 |
| Absolute position component | 1,000 world units |

`step_length` and `branch_radius` must be finite in `[0.0001,10]` and `[0.0001,1]` world units respectively; yaw and pitch must be finite in `[0,180]` degrees. Origin components must be finite and within ±1,000. Expansion checks the symbol and work budgets before appending; stack depth and segment count are checked before allocation; generated positions are checked after every step. The GPU preview additionally rejects structures wider than 400 world units for its current camera range. GPU storage and adapter limits are checked by the renderer and can reject a structure even when grammar compilation succeeds. No geometry is silently truncated.

`AuthoringError` distinguishes I/O, byte limit, JSON/schema, unsupported version, invalid field, budget and generated geometry errors. JSON parser errors include their location. Arbitrary code, shader source, file imports and external references are not accepted.

## Numerical and persistence contract

Generation uses finite `f64` vectors and trigonometric rotations. Results are repeatable for identical input in one supported computational environment; cross-platform bitwise identity is not promised. Floating-point orientation drift can accumulate over many turns. This version does not certify field distance bounds, collision unions, or watertightness.

`Structure::source()` retains the **exact original JSON bytes**. `snapshot_json()` creates a self-contained JSON object with `snapshot_version: 1`, `source_format_version: 1`, `compiler_semantics_version: 1`, lowercase `content_sha256` and the source bytes as a bounded JSON byte array. `load_snapshot_json()` rejects unknown versions, malformed or oversized data, digest mismatch and invalid source before returning a structure. `compile_json_expected()` rejects a newer or otherwise mismatched external file instead of silently substituting it. The snapshot digest checks integrity relative to its stored digest, not authenticity; an attacker who can rewrite both fields can forge it. An independently trusted expected digest is required when authenticity matters.

World-save version 6 embeds exact authored bytes once per revision and validates them on load. Recording only `id` and author-declared `revision` would permit silent replay drift. Generation uses finite `f64` and is repeatable within the documented computational model; cross-platform bitwise geometry is not promised.
