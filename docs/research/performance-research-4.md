# Performance Research 4: heterogeneous mathematical execution

**Baseline:** `21c598d` on `efficiency-research`. Conditional local result; all
results below are local to the reference hardware and checked-in scene limits.

## Architecture decision before implementation

The interpreter correctly handles every validated mixture but pays expanded
graph traversal per pixel and occurrence. A grouped approach would require
separate dispatches and a reduction carrying nearest depth, identity, material,
and uncertainty. A hybrid approach has the same reduction problem if compiled
and interpreted objects run in separate passes. A bounded combined shader can
retain the existing single mixed-depth loop and switch only the field sample
function per occurrence. This is the smallest experiment that preserves the
current depth and diagnostic policy.

The experiment will use validated topology keys in deterministic order, the
existing node parameter buffer, and an explicit per-instance local selector.
It will retain the interpreter inside the combined shader for an invalid
selector and use the ordinary interpreter pipeline while a compatible program
is absent or compiling. A four-entry device-local cache and one worker remain
the compilation limits. No authored identity, persistence format, or world
capacity changes are proposed.

The choice remains **experimental** after the qualifications below. The spatial
guard remains display-only.

## Verified baseline and complete execution path

The worktree was clean at `21c598dddcf562e8d77ca1838a901fa09182fa4a`.
`main` was `eee79d2`; no remote operation occurred. A fresh release target
ran the starting `multi_definition_matrix` on Windows, Rust 1.94.0,
wgpu/Naga 30.0.1, NVIDIA GeForce RTX 4070 SUPER, driver 591.86, Vulkan.
It yielded 54 JSON rows with three warmups and seven timestamp samples per
row. At 1080p, two/four heterogeneous definitions selected the interpreter:
guarded Auto p50/p95 was 4.50/4.63 and 17.22/17.31 ms respectively.
Shared-topology structural execution was 0.24/0.24 and 0.45/0.45 ms.
These are starting-revision measurements, not old Research 3 estimates.

Source JSON passes `world-authoring::field_graph::compile_json`, including
finite parameters, operation, topology, depth, and expanded-visit budgets.
`WorldState` retains exact source bytes, SHA-256 revisions, definition and
instance identities. The renderer derives packed GPU nodes and occurrence
records; the CPU compiler key selects a device-local pipeline. The mixed WGSL
loop still traces every enabled instance, compares world-space hit depths,
retains uncertain/exhausted status, selects the nearest field or analytical
primitive, then samples the associated material. No GPU result becomes a
physical contact or persistent world value.

Existing limits remain: 32 nodes, 12 graph levels, 64 expanded visits per
definition, four definitions, eight instances, 32 KiB generated field WGSL,
four cached pipelines, and one in-flight compilation worker.

## Implemented bounded experiment

`GpuFieldExecution::Combined` is explicit and opt-in. For enabled instances,
the renderer derives structural keys from ordered operation/link arrays and
root indices, sorts and deduplicates them, and includes that key vector plus
compiler semantics version in `CompilerKey::Combined`. The cache variant also
separates this execution mode from exact and structural modes; the owning
`GpuRenderer` keeps pipelines scoped to its device. Shader feature choices
such as the visual guard are runtime bits and do not change generated code.

One source contains a generated evaluator per distinct validated topology.
Each evaluator loads current parameters from the existing packed node buffer;
each occurrence carries an explicit local topology selector. This increases
the bounded instance record from 64 to 80 bytes, at most 128 additional bytes
for eight instances. The selector does not replace the stable world ID or
material index. One mixed pass preserves the existing depth and diagnostic
logic. An invalid selector calls the interpreter within the combined shader.
If a graph is ineligible, generated source exceeds the cap, or compilation
has not finished, the ordinary complete interpreter pipeline remains active.
No user-authored shader text is accepted.

The code reuses generated one-line evaluator functions when building the
combined source. This is a concrete maintenance coupling: a change to the
single-graph emitter format must update the combination logic. Naga validation
and native image tests guard the current format. No automatic execution
policy was added. Static single-definition scenes already have a faster
exact-revision path; compilation latency and wider hardware coverage do not
justify switching the default.

## Matched GPU compute matrix

The final local release matrix contains **216 JSON rows** at 720p, 1080p,
and 1440p. Within each scene, camera, geometry, material, ray interval,
trace policy, normal/depth work, and timestamp source are the same. Each mode
has three warmups and seven samples. `direct` means the visual bounds guard
is off; these offscreen GPU timestamps exclude CPU preparation, readback,
window presentation, and shader creation. Its recorded source revision is
`486ace8` with `source_dirty=true`: the final `combined-direct` benchmark rows
were added before the later `16a09b6` commit, with no compiler or shader edit.
The numbers below are 1080p
**p50/p95 ms** for four definitions/eight instances:

| Scene | Interpreter direct | Interpreter guarded | Combined direct | Combined guarded |
| --- | ---: | ---: | ---: | ---: |
| Shared topology | 115.44/117.04 | 20.23/20.42 | 1.35/2.09 | 0.45/0.45 |
| Distinct graph sizes | 113.25/115.13 | 20.00/25.16 | 1.32/2.04 | 0.48/0.48 |
| Sphere, box, smooth union, transformed union | 72.45/75.68 | 6.86/7.70 | 0.88/1.33 | 0.29/0.29 |
| One definition mostly offscreen | 63.60/64.31 | 5.80/5.95 | 0.76/1.19 | 0.27/0.27 |
| Nearly full-screen first field | 84.41/85.69 | 26.07/27.48 | 0.97/1.50 | 0.49/0.95 |

The complete matrix also covers one and two definitions, Auto and Structural
where eligible, CPU scene/binding/pipeline-selection times, node-write enqueue
time, source size, cache selections, and compile requests. The 1080p
four-definition mixed-operator combined shader was 22,620 bytes; the
1/4/8/16-node heterogeneous shader was 25,454 bytes. Resident node and
instance payloads in those rows were 288/640 and 928/640 bytes. Driver
pipeline allocations and GPU transfer completion are not exposed here.
For a single shared graph, exact specialization remained faster than the
new path in a representative run (about 0.12 versus 0.14 ms at 1080p).
The mostly offscreen one-definition scene was about 0.07 ms under both
guarded interpreter and combined execution. No universal speedup is claimed.

First observed creation of new heterogeneous pipelines took roughly
**438–556 ms** across local runs. Repeated process runs reported about
**2–3 ms** for the same combinations. The driver cache was not cleared or
controlled, so these are cold-ish versus warm observations, not a reliable
fresh-install distribution. The single worker accepts one request at a time;
there is no unbounded queue. While an obsolete request is still compiling,
a newer incompatible program waits and all geometry uses the interpreter.
A cached compatible program can become active even while that old request
remains pending. A failed compilation stays in the bounded cache and retains
interpreter fallback. `requests` records accepted compile misses, not driver
cache misses.

## Correctness and changing world

Real offscreen GPU readback compared the combined shader with the existing
interpreter for two and four different topologies. Sphere, box, smooth union,
translation, scale, hard union, analytical depth, and distinct field materials
were visible in the captured images. A parameter revision changed pixels
without another compile. Six 640×360 adversarial viewport cases covered
0.01×, 0.1×, 1×, and 100× instance scales, an inside origin, and an exhausted
one-iteration trace: **1,382,400 pixel positions**, each compared between
interpreter and combined execution. Maximum RGBA8 channel difference was
zero on Vulkan and on the same physical GPU through DX12 with installed SDK
DXC. The test also compares the visual guard toggle in diagnostic mode.
These are differential image comparisons sharing the mixed tracing and depth
policy; they are not an independent high-precision proof of all rays.
Existing CPU field, analytical contact, GPU query, near-tangent, and
`y = 1 + 1e-8` regressions remain in the full suite.

A release-profile 40-step fixture changed authoritative `WorldState` source
revisions and transforms for four definitions, compared a cloned replay after
every deterministic edit, and retained all instance IDs. One structural edit
used the interpreter while a new program compiled; returning to the original
topology reused the cached program. A rejected invalid edit left the world
unchanged. The sequence made two compile requests total; 39 sampled steps
used Combined (GPU p50/p95 0.0685/0.0740 ms at 640×360), one used the
interpreter (4.6075 ms). This is a renderer research fixture, not a new
persistent simulation event or ecology rule.

The guard experiment exposed a real contract issue: with a one-iteration
trace, guard-enabled diagnostic rendering suppressed `Exhausted` colors
for rays rejected by bounds; maximum channel difference from direct tracing
was 229. Diagnostic mode now bypasses spatial culling, so uncertain and
exhausted tracer results remain visible. The six-case regression then had
zero guard-toggle difference. This fix trades performance for diagnostics:
the 1080p four-definition combined direct versus guarded rows above indicate
roughly 0.88 versus 0.29 ms for mixed operators, but they were measured with
diagnostics off and are only an estimate of diagnostic cost. Ordinary guarded
rendering remains visual-only. No general floating-point enclosure proof was
derived for bounds conversion, transformations, slab division, or smooth
union; it must not grant physics authority or be treated as a certified Miss.

## Actual World Lab frame pacing

Two matched 600-frame runs used the checked-in showcase plus an in-memory
validated box, the same deterministic camera/material/transform/definition
phases, replacement of that third instance's box topology with a sphere at
frame 525, restoration of its box topology at 545, and an
invalid edit at 550. The extra object and edits are world-owned during the
run; nothing is written to the authored scene or save format. The window used
Vulkan and `Fifo`. Intervals below omit the first 30 frames (570 samples).

| Mode | Interval p50/p95/p99 | Worst | >16.7 ms | Compiler requests | Active frames |
| --- | ---: | ---: | ---: | ---: | --- |
| Heterogeneous Auto | 6.064/6.257/6.758 ms | 8.405 ms | 0 | 0 | 600 Interpreter |
| Heterogeneous Combined | 6.055/6.333/6.915 ms | 7.344 ms | 0 | 2 | 489 Combined, 111 compiling/interpreter |
| Repeated Combined process | 6.058/6.284/6.696 ms | 7.666 ms | 0 | 2 | 577 Combined, 23 compiling/interpreter |

No frame exceeded 33.3 ms. In the first Combined run, the first compiled
frame was 92; in the repeated process it was 4. The median surface-acquire
time was about 5.6–5.8 ms. Only two GPU timestamp samples were taken per
window run, so these interval distributions do **not** establish equal GPU
throughput or a controlled cold-cache guarantee. Automated changes reported
no real mouse/keyboard handling latency. No non-Fifo present-mode run or
second physical GPU was completed.

## Backend and execution decisions

DX12 with system FXC still fails during creation of the pre-existing
analytical shader: `X3511` forced-loop-unroll errors at translated HLSL
lines 111 and 465, before the heterogeneous compiler is selected. The
analytical WGSL contains dynamically indexed axis and BVH/capsule loops;
the emitted HLSL line mapping has not been isolated enough to justify a
mathematical shader rewrite. The same combined parity test passed on DX12
using an already installed Windows SDK DXC 1.8.2502.11. That library is not
shipped and is not a production DX12 solution. Vulkan remained functional.
Changing APIs on one RTX 4070 SUPER is not testing a second GPU.

| Decision | Result |
| --- | --- |
| A — heterogeneous compilation | **CONDITIONAL:** retain opt-in up to four compatible validated topologies; strong local GPU gains, high first-use cost and limited portability evidence. |
| B — compiler architecture | Keep exact, structural, combined, and complete interpreter paths for now. The text-based evaluator reuse is specific debt; consolidate only when its format changes or another operation requires it. |
| C — numerical safety | Diagnostic mode preserves tracer uncertainty by bypassing guard. Ordinary guard is empirical display acceleration, with no certified enclosure or physical authority. |
| D — living-world readiness | **PARTIALLY:** several structures and parameter revisions render efficiently after compilation on the reference Vulkan GPU. First-use latency, DX12 FXC, other hardware, and a versioned persistent parameter-update rule remain blockers to a larger ecosystem claim. |

## Verification, limits, and reproduction

Implementation checkpoints were `47e29a8`, `180d288`, `486ace8`, and
`16a09b6` on
`efficiency-research`. `cargo fmt --all -- --check`, strict locked workspace
Clippy, locked workspace build, **145 ordinary tests / 0 failures**,
**55 serial opt-in tests / 0 failures**, rustdoc, and
`cargo deny check advisories licenses` passed locally. The Astro site built
184 pages under `/Genesis/` and checked 16,667 local links. The build still
reports pre-existing nonfatal i18n, duplicate-404, and sitemap-site warnings.
Hosted GitHub CI was **not verified**. The Passage received the ordinary
workspace tests but no new manual playtest during this campaign. The
deliberately negative DX12/FXC run failed with
`X3511`; DX12 with installed SDK DXC and Vulkan parity controls passed.

| Stage | Status | Evidence or remaining limit |
| --- | --- | --- |
| 0 Baseline | PASS | Clean `21c598d`, 54 starting-head JSON rows. |
| 1 Architecture study | PASS | Combined, grouped, and hybrid costs and correctness risks compared. |
| 2 Combined prototype | PASS for bounded experiment | Two/four topologies, Naga and real GPU readback, complete fallback. |
| 3 Mathematical parity | PARTIAL | Broad image differential and existing independent fixtures; no independent exact per-pixel outcome corpus. |
| 4 Performance | PASS locally | 216 matched release GPU rows on one Vulkan GPU. |
| 5 Dynamic world | PASS for fixture | 40 authoritative edit steps and matched World Lab phases; no new persistent event. |
| 6 Compiler pacing | PARTIAL | Three 600-frame Fifo runs; uncontrolled cold cache and no non-Fifo/second GPU. |
| 7 Spatial guard | PARTIAL | Hidden `Exhausted` found and diagnostic bypass verified; no general f32 enclosure proof. |
| 8 DX12 | BLOCKED with FXC | `X3511` before field compilation; installed SDK DXC control passes. |
| 9 Execution policy | CONDITIONAL | Explicit mode retained; automatic selection lacks first-use evidence. |
| 10 Regression | PASS locally | Rust, opt-in GPU, rustdoc, dependency and site checks; hosted CI not verified. |
| 11 Consolidation | PASS locally | This report and ignored JSONL/BMP review package. |
| 12 Decision | PARTIALLY ready | Bounded multi-topology rendering works; product blockers above remain. |

The machine-readable local evidence and real offscreen GPU captures are in
the ignored `target/performance-research-4-review/` directory, with its own
README. They are not versioned or remotely published.

This research did not validate independent exact hit distance,
normal, and world ID for every pixel; the image parity test carries color,
depth, material, and diagnostic effects, while existing CPU/GPU query tests
cover their own bounded fixtures. There is no measured driver pipeline memory,
controlled cold cache, GPU upload completion, non-Fifo presentation, or
different physical adapter. The opt-in mode must remain a research facility.

Run from the repository root:

```text
cargo test --release --target-dir target/performance-research-4-build -p analytic-renderer --test mixed_benchmark multi_definition_matrix --locked -- --ignored --nocapture --test-threads=1
cargo test --release --target-dir target/performance-research-4-build -p analytic-renderer --test mixed_benchmark heterogeneous_world_edit_stress --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test mixed_gpu combined_topologies_preserve_mixed_image_and_fallback --locked -- --ignored --nocapture
```

For window measurement, set `GENESIS_WORLD_LAB_PROFILE` to a writable JSONL
path, `GENESIS_WORLD_LAB_PROFILE_FRAMES=600`, and
`GENESIS_WORLD_LAB_PROFILE_SCENARIO=heterogeneous-auto` or
`heterogeneous-combined`, then run:

```text
cargo run --release --target-dir target/performance-research-4-build -p world-lab -- examples/world-lab/scenes/showcase.json
```

The next narrow engineering step is to qualify first-use compilation on a
second physical GPU and a fresh installation, then test a bounded prewarm or
obsolete-request policy against actual window intervals. Before a persistent
multi-species demonstration, give world parameter changes a versioned,
replayable event contract; do not promote display fields to contact authority.
