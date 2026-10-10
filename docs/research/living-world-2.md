# Living World 2: resource-coupled implicit geometry

**Status:** experimental local research, starting at `d53e7d9` on `living-world`.
The question is whether a persistent implicit organism can acquire the same
finite resources as analytical organisms and change shape without compiling a
new shader on every tick. Results below are local observations, not a claim of
biological realism or hardware-wide performance.

## Architecture and decision

Living World 1 already had a single authoritative `WorldState`, finite source
ledgers, proportional contested allocation, fixed ticks, typed events, stable
IDs, v7 snapshots, and two analytical organisms. Field Graphs already had
validated immutable source definitions, world occurrences, CPU evaluation,
interpreted GPU execution, Structural compilation, and mixed rendering. The
missing connection was a bounded growth law for one field occurrence, a demand
row in the existing allocator, v8 persistence, and a mixed native scene.

The new organism uses the existing two-lobed `twin.json` graph. Its source
bytes, graph topology, semantics version, and SHA-256 definition revision stay
fixed. `FieldGrowth` and `Transform.scale` are mutable world state. The
occurrence ID is stable; its generation tracks updates. The visual GPU
instance, buffers and compiled program are derived from the world. A uniform
scale changes both lobes and their separation without changing topology. This
is one shape degree of freedom, not a general parameter schema or scripting
system. The existing field remains visual only; it gains no implicit physical
collision authority.

At each 16,666,667 ns tick, ordered events apply first, finite sources
replenish, and every analytical organism and growing field requests units from
the same source stock. The field's per-source request is the local source
sample at its world position × `uptake` × step seconds. Demand is capped by
remaining scale headroom divided by `scale_per_unit`. Each source allocates
proportionally when contested, once, and records the actual debit in the
existing ledger. The field's scale becomes
`min(max_scale, initial_scale + total_allocated * scale_per_unit)`.
In the scene: initial scale 0.34, uptake 1.2, scale gain 0.28 per unit,
maximum scale 1.2. Scale and allocation are validated before commitment;
failed ticks leave world and time unchanged. NaN, infinity, out-of-range
coefficients, direct transform changes and definition replacement on a
growing occurrence are rejected. No arbitrary user-supplied field parameter
map was introduced. The current law has no reproduction or adaptive topology.

This shares the original source ledger. The source invariant remains initial
stock + actual replenishment − all allocations = current stock, within its
documented floating-point tolerance. With the source at a different location,
the field receives less resource, leaving a different supply for analytical
neighbors; later branch counts differ as a consequence. Growth saturates at
the scale limit. The allocation order is deterministic within the tested
Rust/toolchain and CPU model; cross-architecture bitwise identity is untested.

## Persistence, provenance, and replay

`Runtime` writes v8 only when a field has `FieldGrowth`; ordinary worlds still
write v7. The v8 completed snapshot contains the growth law, allocation
counters, current scale, exact graph source bytes and revision, world IDs, tick,
sources, organisms, and future queued events. Load validates the complete
candidate. A dynamic field mislabeled as v7, corrupted allocation, or
inconsistent scale is rejected; failed `load_into` leaves the current runtime
unchanged. Repeated save/load cycles and v7 compatibility were tested.

The headless A/B artifact stores a separate **initial v8 snapshot plus one
typed event** (`MoveSource`, source ID 2, tick 61). The exact source bytes in
that initial snapshot make this experiment independent of the external scene
file. Replaying those inputs starts from the initial snapshot, never from a
completed snapshot. Native F5/F9 save a completed state; they do not retain
the history of previously applied keyboard events. An invalid source event
rejects the tick atomically under the existing runtime contract.

## Controlled experiment

Both worlds start from the same initial save, seed, IDs, two source states,
two analytical organisms and one implicit field. Only B schedules the source
move. Both run the same tick implementation to 240, with a save/load at 120.
Fresh replay from the recorded initial snapshot and event equals both final
runtimes on the tested Windows build.

| Tick | Same world | A/B implicit scale | A/B analytical nodes |
| ---: | :---: | ---: | ---: |
| 0 | yes | 0.34 / 0.34 | 1,1 / 1,1 |
| 60 | yes | 0.48 / 0.48 | 13,4 / 13,4 |
| 61 | no | 0.48 / 0.48 | 13,4 / 13,4 |
| 90 | no | 0.55 / 0.50 | 17,6 / 15,5 |
| 120 | no | 0.63 / 0.53 | 18,9 / 17,6 |
| 240 | no | 0.97 / 0.65 | 19,24 / 19,12 |

At tick 240 the field acquired 2.25 units in A and 1.11 in B. Final FNV-1a
save-byte digests are `505fd6ab38ab9a6d` and `82ee35bfc7af85e3`;
`replay_equal=true`. These digests are diagnostics, not cryptographic
attestation. The Living World 1 experiment remains separate and retained its
final digests `3a46d14b9bdef83f` and `296f4ab12a43c0c3`.

## CPU/GPU execution and visual evidence

The native mixed renderer reads the current world and requests Structural
execution with the existing interpreter fallback. Uniform growth changes the
occurrence transform data; it does not change the program compatibility key.
An opt-in actual GPU readback compared interpreter and Structural mixed images
at ticks 0, 60, 120 and 240 for both worlds (640×360). Maximum RGB-byte
difference was 1; A/B images matched before intervention and differed later.
One compilation request and one node upload served all observed growth states.
The eight BMPs were visually inspected: both mathematical lobes and analytical
structures appear, with partial occlusion by the field. This is image parity,
not independent proof of every ray distance, normal, material ID or all
uncertainty cases. Existing Field Graph numerical and compiler regression
suites provide the lower-level checks; they were not replaced by this test.
Compiler pending/failure uses the existing interpreter path. A new structural
edit and device recreation were not separately exercised in this scene.

## Local performance

Windows, Rust 1.94, release profile, Vulkan, NVIDIA RTX 4070 SUPER, driver
591.86. One local run; seven samples after two warmups per offscreen mode and
resolution. Compute timestamps exclude presentation and compilation. All
measurements below use the same tick-240 intervention world and camera.

| Resolution | Interpreter GPU p50/p95 | Structural GPU p50/p95 |
| --- | ---: | ---: |
| 1280×720 | 4.6466 / 4.7137 ms | 0.1102 / 0.1109 ms |
| 1920×1080 | 10.3060 / 10.4804 ms | 0.2111 / 0.2117 ms |
| 2560×1440 | 16.6278 / 16.6454 ms | 0.3693 / 0.3697 ms |

CPU tick p50/p95 across the changing control world was 0.0057/0.0124 ms;
intervention was 0.0045/0.0073 ms. Analytical scene derivation was
0.0040/0.0089 and 0.0028/0.0043 ms respectively. GPU field binding in the
offscreen intervention test was around 0.0067–0.0107 ms p50 across modes and
resolutions. These timings are separate stages and cannot simply be summed
into a window frame prediction. A native 1080p `Fifo` window advanced beyond
tick 2000 with an automated tick-61 intervention; measured frame interval
median was about 6.05 ms and late-window p95 about 8.01 ms. Presentation
acquisition alone commonly occupied 5.4–6.5 ms. The saved review log covers
the first 785 ticks; its late-window interval p50/p95 was 6.0595/7.8885 ms.
This was an automated window
run, not a manual key-flow playtest. Repeated compilation during ordinary
growth was not observed.

## Qualification and limits

| Question | Result | Evidence or limit |
| --- | --- | --- |
| A. Authoritative mathematical state | PASS | World-owned growth and scale survive validation and save/load. |
| B. Resource causality | PASS | One source event changes allocation, scale, and analytical branches. |
| C. Rendering from state | PASS | Actual mixed readbacks follow world states. |
| D. Program reuse | PASS | One Structural request and node upload across tested growth states. |
| E. Persistence | PASS | v8 snapshot, exact source, v7 gate and atomic load. |
| F. Replay | PASS | Initial snapshot plus event reproduces tested final states. Native keyboard history is not archived. |
| G. Correctness scope | PARTIAL | Visual fields remain nonphysical; independent per-ray mixed-scene oracle and device recreation were not run here. |
| H. Responsive reference hardware | PASS | Tested 1080p native run and offscreen timings on one RTX 4070 SUPER; manual usability remains unverified. |

The implementation reuses the existing allocator, Field Graph renderer and
Structural cache. Rejected approaches were a second resource ledger, GPU-owned
growth, rewriting graph JSON on every tick, and granting arbitrary implicit
fields physical contact. The next milestone should independently compare
mixed CPU/GPU ray identity and uncertainty behavior, test compilation fallback
during actual world edits, and repeat native interaction on a second adapter.

A separate 2,000-tick test reaches the scale cap while retaining field identity,
zero further allocation at saturation, bounded source stocks, and a valid v8
round trip. The local review directory under `target/living-world-2-review/`
contains JSON, GPU BMPs, test logs, and reproduction commands.
