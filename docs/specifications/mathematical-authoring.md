# Mathematical structure authoring format v1

## Scope

This is a **static structural grammar**. It is neither a scalar field nor an ecological growth rule. The source JSON is authoritative for generation; the generated `Structure` contains analytic capsule endpoints, radii and parent relationships. A renderer `Scene` is a disposable snapshot. `WorldState` remains authoritative for mutable simulation, and authored preview structures are not inserted into its organism collection or save format.

Two runnable definitions and commands are in `examples/authoring/README.md`. The format is independent of the GPU API. `world-authoring` has no renderer dependency.

## Schema

Version 1 requires `format_version: 1`, a lowercase ASCII `id` (1–64 characters, starting with a letter), positive `revision`, `axiom`, `rules`, `iterations`, `step_length`, `yaw_degrees`, `pitch_degrees`, `branch_radius`, and `budgets`. Optional `instance.origin` is a three-component world-space translation and defaults to `[0,0,0]`. Unknown fields, duplicate JSON fields, malformed UTF-8/JSON, unsupported versions, nonfinite values, and invalid ranges are rejected. A `revision` is an author-supplied label, **not** a content hash or proof that two files have identical meaning.

Each rule is `{ "symbol": "X", "replacement": "..." }`; its left side is one ASCII uppercase letter. No two rules may use the same left side. `F` may itself have a rule. Other uppercase symbols are inert at geometric interpretation and must have a rule. Replacement strings may be empty. Every axiom and individual replacement must have balanced square brackets; the final expansion is also interpreted with checked pushes/pops. This restriction avoids rule fragments whose brackets rely on other rules to close them.

## Parallel rewriting and typed geometry

At each iteration, scan the previous string left to right. Replace each symbol with its rule's full right side when one exists; copy it otherwise. Replacements generated in an iteration are **not** processed until the next iteration. After exactly `iterations` passes, scan the final stream once with the turtle below. Expansion and drawing order are deterministic. There is no random seed because v1 has no stochastic choice.

The turtle begins at `instance.origin` with heading `+Y`, local right `+X`, and local up `+Z` in the project's right-handed coordinates. `F` draws one closed-segment analytic capsule from the current point to `point + heading × step_length`, then advances. `+` and `-` rotate heading toward/away from local right around local up by `yaw_degrees`; `&` and `^` rotate heading toward/away from local up around local right by `pitch_degrees`. Angles are degrees in the file and converted to radians for evaluation. `[` saves position, orientation and the current parent segment; `]` restores them. Variables draw nothing. The branch radius is constant for v1. A nonempty drawing is required.

Each `F` allocates a one-based `Segment.id` in depth-first interpretation order. `Segment.parent` is the most recently drawn segment in that branch, or `None` for a root. Branch return restores this parent, so siblings do not become accidental children. Identity is stable for identical definition bytes and compiler behavior; changing rules or iteration count can renumber later segments. The typed `Segment` record stores world-space endpoints and radius. Preview conversion uses `analytic_field::Capsule` and `analytic_renderer::Primitive::capsule` with the same numeric ID. Capsule overlap is **not** claimed to form a watertight CSG solid.

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

`Structure::source()` retains the **exact original JSON bytes**. A future save containing an authored instance must embed those bytes or a collision-resistant content digest plus an immutable retrievable definition. The current world save schema does not contain authored structures. Changing the external file and pressing R compiles a complete replacement before swapping the active static preview; a failed reload keeps the prior geometry. Recording only `id` and `revision` would permit silent replay drift and is insufficient.
