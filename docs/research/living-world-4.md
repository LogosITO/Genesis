# Living World 4: bounded authored morphology

**Status:** local experimental research on `living-world`, starting at
`b3f688b`. This report records local results, not hosted CI or a human
playtest. `WorldState` remains the source of truth; mathematical GPU buffers
and shaders are derived visual resources.

## Starting architecture and decision

Living World 3 had a fixed two-sphere morphology type. Its two radii had
world-owned state, used surface-derived source sampling, and shared the finite
resource allocator with analytical organisms. The Field Graph parser already
had exact-byte definition revisions, validated operations and conservative
bounds. GPU interpreter, Structural and Combined evaluators already existed,
but compiled mixed rendering exposed only RGB. The new work reuses those
contracts instead of adding another organism-specific simulation subsystem.

The runtime schema is `FieldParameters` version 1. At most four ordered
`ParameterRule` channels bind an authored node ID and supported component to
an initial value, current value, maximum, uptake, gain per allocated unit,
one signed-axis sample feature, allocation counters and a monotonic parameter
revision. The immutable graph source and SHA-256 definition revision never
change on an ordinary growth tick. Different occurrences of one source may
have different current values. The validated derived graph and bounds are
rebuilt when values change. A source revision identifies bytes; a structural
compiler key identifies operation topology; parameter values live in the GPU
node buffer. A compiled Combined program is reused across compatible numeric
updates. Exact `Auto` specialization falls back to the interpreter for this
dynamic schema.

Supported components are sphere radius and each axis of box half extent.
Translation, uniform scale and smooth-union width were evaluated as possible
later additions but left outside this first parameter contract. The rule
vocabulary is deliberately one bounded resource-driven growth law. Authoring
uses a versioned JSON scene, local sibling graph files, and explicit numeric
limits; it does not run user code or WGSL. A developer can change ordinary
coefficients, positions and supported bindings without editing Rust. The
[guide](../guides/living-world.md) gives the complete workflow. The v1 scene
requires exactly two fields to keep this experiment bounded; it is not an
unrestricted world authoring format.

## Feedback and state transitions

For channel *c* and finite source *s*, demand per fixed tick is
`source.sample(site_c) × uptake_c × step_seconds`. The site is a chosen sphere
or box axis feature transformed along a unique graph path and through the
instance transform. Aliased DAG paths are rejected because one binding would
have multiple sites. With feedback enabled, current parameters determine the
site; with feedback disabled, the authored initial geometry determines it.
Each channel's remaining growth headroom caps its demand. The existing
deterministic shared allocator distributes finite stock once among analytical
organisms and every field channel. Allocated units update world-owned values,
which update the graph, bounds and later sampling sites. These are point
samples of a proximity field. They are not area integration, nearest-surface
queries, physical contacts or collision authority. A feature may be hidden
inside a union.

The checked-in scene combines a hard union of translated sphere lobes with a
translated box, plus the existing analytical ecosystem. The two graphs use
the same channel and allocation code. A source move at tick 61 is the only
intervention; no final shape is scripted. Source references are restricted to
plain sibling file names, and the scene/graph byte limits are 64/16 KiB.
Invalid scene reload constructs no replacement world. Numeric and duplicate
bindings are rejected before mutation; world updates use candidate copies.
An external-scene reset test rejected malformed JSON without changing the
running state, then accepted a corrected rule. A 2,000-tick CPU run remained
valid through saturation with save/load every 500 ticks.

## Fixed causal experiments

Release-profile `--living-rules-experiment` on the checked-in scene ran to
tick 240. Control and intervention states were equal through tick 60 and
diverged at tick 61. A fresh replay from the v10 initial save and event
matched the completed intervention state. Values below are the two sphere
radii followed by the box x half extent, rounded only for presentation.

| Case, tick 240 | No intervention | Intervention | Intervention without geometry feedback |
| --- | --- | --- | --- |
| Checked-in scene | 0.97279, 1.08503; 0.60846 | 0.70556, 0.83768; 1.10000 | 0.71568, 0.82758; 1.10000 |
| Box x shifted to 2.3 | 1.01585, 1.14417; 0.46489 | 0.72304, 0.87221; 1.10000 | 0.73439, 0.85829; 1.10000 |
| Left-lobe uptake set to 0.8 | 0.81512, 1.15467; 0.62854 | 0.63156, 0.86722; 1.10000 | 0.63564, 0.85913; 1.10000 |

All three fixed cases had equal repeated replay, different intervention and
no-intervention worlds, and different feedback/ablation worlds. In the
checked-in case the analytical node counts were `[19,16]` without intervention
and `[19,8]` with it; the first finite source ended at stock `0` versus
`0.47987`. The box saturates after intervention in all three cases, so its
feedback ablation has no end-value difference. Radius feedback differences
are modest relative to the direct intervention. Three chosen deterministic
conditions are a bounded mechanism test, not a statistical or biological
claim. The full JSON, including source allocations and site positions, is in
the local review package.

## GPU spatial verification

The native mixed shader now has an opt-in readback flag. The production draw
still binds one small dummy result record; a diagnostic call allocates up to
16,384 result records and reads the *same dispatch's* selected hit, distance,
normal, frame-local analytic primitive or stable field instance ID, frame-local
material index, uncertainty/exhaustion/ambiguity bits and maximum observed
field iterations. The authored definition revision is mapped from the
selected field ID in the validated CPU world after readback. The camera and
pixel index determine ray origin and direction; world hit position is derived
from the returned distance. This does not certify the underlying f32 result.

Independent one-pixel tests selected the implicit sphere at distance
approximately 3.0, then the nearer analytical sphere at 1.5. A transformed
box returned its expected 3.4 distance and outward normal, while an
inside-sphere start returned explicit uncertainty rather than a false miss.
Actual readback distances were 2.999040, 1.5 and 3.398912, respectively;
the box normal was `[0,0,-1]`. The machine-readable values are in the local
review package.
Two occurrences sharing exact source bytes but holding radii 0.5 and 0.75
returned separate identities and expected depths 3.5 and 3.25; this catches
incorrect GPU node-buffer sharing. These cases had correct identity and
status. On RTX 4070 SUPER / Vulkan, native
readback compared interpreter and Combined at ticks 0, 60, 61, 120 and 240
for both A/B worlds: **40,960 pixel pairs**, with zero status, identity,
definition, material or visual-guard classification mismatches. Every selected
depth agreed within 0.005 world units and normal component within 0.02. The
two paths share the same ray-step algorithm, so this is
differential execution evidence rather than an independent mathematical proof
for general graphs. The older Living World 3 independent sphere oracle remains
separate. Near-tangent, hidden-Exhausted and broad analytic capsule cases were
not newly qualified through this mixed diagnostic API.

Ten actual 640×360 BMP frames were captured; control and intervention images
were identical before the event and visibly different at tick 240. One
Combined compilation request was observed across all tested parameter updates
and both topologies. Resident GPU node uploads increased as the world evolved.
The renderer's parameterized-instance buffer sharing and exact-specialization
fallback were corrected after detecting stale-value risks in the first draft.
Visual bounds remain display guards only; uncertainty and exhaustion remain
observable rather than being promoted to a miss certificate.

## Dynamic offscreen performance

RTX 4070 SUPER, Vulkan driver 591.86, release profile, tick-120 intervention
world advanced one tick before each sampled frame. Each entry is the median
and p95 of seven samples after two warmups. The same initial state and tick
sequence were used for each execution mode. GPU values are compute timestamps;
offscreen frame values include submission/readback waiting and exclude window
presentation and VSync.

| Resolution | Interpreter GPU ms p50 / p95 | Combined GPU ms p50 / p95 | Combined offscreen frame ms p50 / p95 |
| --- | ---: | ---: | ---: |
| 1280×720 | 9.029 / 9.196 | 0.158 / 0.159 | 0.347 / 0.666 |
| 1920×1080 | 19.701 / 20.211 | 0.313 / 0.314 | 0.579 / 0.687 |
| 2560×1440 | 32.507 / 33.825 | 0.549 / 1.008 | 0.907 / 1.290 |

Measured CPU simulation tick medians ranged from 0.015 to 0.039 ms across
these runs. Field binding medians ranged from 0.011 to 0.025 ms; resident node
data was 224 bytes and node uploads rose on every changing tick. Compilation
requests stayed at one. These timings do not measure interactive window frame
intervals, pipeline startup latency in the benchmark table, other hardware,
or simultaneous maximum-capacity worlds. No 60 FPS guarantee follows from
compute timestamps alone. No separate DX12/FXC qualification was made.

## Persistence and remaining risk

Generic parameter state requires v10. Older v7/v8/v9 saves retain their
existing selection and loading contracts. Loading validates exact source
bytes, hashes, parameter schema and allocation consistency, then reconstructs
the derived graph and bounds. F5/F9 completed saves and F6 initial-state plus
event-history bundles remain distinct. The local test covered an intermediate
v10 save/load, resumption, mismatched version and tampered state; native
history replay and invalid event order tests also passed; the exported v10
bundle replayed from the release CLI at tick 240. Cross-platform
bitwise replay is not promised.

The architecture now supports two distinct authored mathematical structures
under one rule mechanism. It remains a deliberately narrow research model:
no arbitrary parameters or growth expressions, no physical field contact,
no proof of arbitrary graph ray correctness, no second adapter, no device-loss
test and no human playtest. Source proximity is a design choice whose effect
should be tested on broader scenes before making it a general engine law.
The next step should broaden independent mixed spatial oracles, then test one
additional parameter family only if it can keep bounds and GPU execution
sound.

| Final question | Decision | Evidence and boundary |
| --- | --- | --- |
| A. Generality | PASS | Sphere-union radii and translated-box extent use the same channel state and allocator. |
| B. Authoring | PASS | Supported bindings and coefficients are editable in bounded JSON without engine code edits. |
| C. Environment feedback | PASS | Three fixed conditions diverged from their no-feedback ablations; the effect on radii was small. |
| D. GPU correctness | PARTIAL | Actual mixed identity/depth readback and independent sphere/box cases passed; arbitrary graph and threshold coverage is incomplete. |
| E. Numerical uncertainty | PARTIAL | Inside-start uncertainty is explicit; the new diagnostic corpus did not independently exhaust difficult boundary classes. |
| F. Persistence | PASS | v10 round-trip, tamper rejection, intermediate resume and typed history replay passed. |
| G. Compilation | PASS | One Combined request across the tested dynamic sequence, with changing node uploads. |
| H. Performance | PARTIAL | Offscreen GPU and CPU work were measured on one adapter; native presentation intervals were not. |
| I. Maintainability | PARTIAL | One new generic rule path serves two shapes; older dedicated morphology remains for compatibility. |
| J. Developer value | PASS | The checked-in sibling-file scene and guide reproduce a second supported form through data. |

## Reproduction

```text
cargo run --release -p first-light --locked -- --living-rules-experiment
cargo test -p analytic-renderer --locked --test mixed_gpu native_mixed_diagnostic_selects_nearest_mathematical_object -- --ignored --nocapture
cargo test -p first-light --locked two_topologies_native_gpu_identity_and_images -- --ignored --nocapture
cargo test --release -p first-light --locked two_topologies_native_gpu_resolution_measurement -- --ignored --nocapture
```

Set `GENESIS_LW4_CAPTURE_DIR` to an absolute local directory before the GPU
identity/image test to write BMP captures. The ignored local
`target/living-world-4-review/` package holds logs, source/revision metadata,
experiment JSON and captures when reproduced; it is not published.
