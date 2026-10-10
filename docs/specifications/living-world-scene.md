# Living World 1 scene format

The optional external scene is a version-1 JSON document describing an
**initial condition**. The simulation laws remain the existing fixed-step
Genesis growth and finite-resource allocation code. The file is never executed
as Rust, WGSL, or a general scripting language.

The supported keys are `format_version`, `seed`, `organisms`, and `sources`.
Unknown keys are rejected. Version 1 requires exactly two organisms with
different growth parameter sets and exactly two finite sources. Each organism
has a world-space `root` array and positive, bounded `node_radius`,
`segment_length`, `threshold`, and `uptake` numbers; `max_children` is 1 or 2.
Each source has a world-space `position`, positive `radius`, nonnegative
`strength`, and finite `stored`, `capacity`, and `replenish_per_tick` values.
The existing `WorldState` constructors enforce numeric and spatial limits.
The entire file is capped at 64 KiB. See the checked-in
`examples/first-light/scenes/living-world.json` for exact syntax.

The parser builds a complete candidate `WorldState`, validates it, and only
then replaces the running world on **R**. Failed reads, malformed JSON,
unsupported versions, nonfinite or out-of-range values, equal strategies, and
budget violations leave the running state unchanged. The format has no include
directive or path references. The command-line scene path itself is chosen by
the user and read directly.

The authored file is immutable input. Once launched, organism nodes, resource
stocks, and event effects live in authoritative `WorldState`; GPU buffers and
BVHs are disposable derived data. A version-7 runtime save stores the effective
validated parameters and current state. It does not embed the original scene
bytes or a content revision, so the save alone cannot prove which external
file created it. Add a source-revision field only with a deliberate save-format
migration if provenance becomes necessary.

The native demonstration uses a 16,666,667 ns fixed step. The bounded A/B
experiment uses the same scene and step. Floating-point replay equality is
verified on the tested build and hardware; cross-platform bitwise identity is
not guaranteed.
