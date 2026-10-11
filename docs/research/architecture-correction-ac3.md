# Architecture Correction AC-3: authoring compilation and shader lifecycle

**Status:** conditional local acceptance of an explicit authoring preference. This is a
research result on one NVIDIA RTX 4070 SUPER (driver 591.86), Windows 11,
wgpu 30.0.1/Vulkan, Rust 1.94.0. It does not certify other GPUs or a
production compiler. The source baseline was clean `living-world` at
`80ccf8576877935ed68811350b8572d46859bbcd`; the implementation commit is
`0217ab2` (`perf(renderer): reuse topology programs during authoring`). The
final documentation commit and complete local package are recorded by the
campaign's final Git status and `target/ac3-review/README.md`.

## Evidence categories and architecture map

**Established in current code:** World Lab reads bounded JSON and referenced
definition bytes in `LabScene::load`; `WorldState` validates and owns the
result. Authored byte content has a `DefinitionRevision`, while a live field
occurrence can additionally expose current runtime parameters through
`WorldState::field_graph`. The renderer packs GPU nodes, instances, materials
and topology selectors from that world. `field_compiler::structural_key` uses
the validated root and ordered operation/child links, independent of numeric
values, and rejects nonfinite or nonpositive packed values. Its key includes
`SEMANTICS_VERSION`. Exact WGSL embeds numbers; Structural and Combined WGSL
read current node parameters from GPU storage. Combined dispatches a sorted
set of at most four distinct structural keys. The fallback interpreter remains
the general supported rendering path. Shader generation, wgpu validation and
pipeline creation run in one background worker at a time. The renderer polls
completion, keeps at most four ready or failed results in an LRU-like deque,
and selects a pipeline only when its complete key matches the current world.
The cache and pipelines belong to a renderer and its wgpu device; a device
mismatch is rejected. These GPU resources never enter `WorldState` or saves.

**Decision:** `Auto` remains Exact for immutable compatible definitions and
Combined for enabled dynamic occurrences, as established in AC-2. The new
`Authoring` preference uses the existing Structural key and emitter when the
enabled world has one distinct supported topology. It uses the existing
Combined path for two to four distinct supported topologies. World Lab asks
for `Authoring` by default because source edits are expected there. Other
runtime callers retain `Auto`. No edit-frequency heuristic, debounce queue,
new shader IR or persistent pipeline store was added. The policy never edits
source bytes, revisions, simulation results or runtime parameters.

| Request and current world | Selected attempt | Fallback |
| --- | --- | --- |
| `Auto`, immutable compatible revision | Exact | Interpreter while pending, failed or incompatible |
| `Auto`, enabled runtime parameter/morphology | Combined | Interpreter when unsupported or over budget |
| `Authoring`, one supported topology | Structural | Interpreter while pending, failed or unsupported |
| `Authoring`, two to four supported topologies | Combined | Interpreter while pending, failed or unsupported |
| `Authoring`, more than four topologies | None | Interpreter, explicit topology-limit reason |
| Explicit Structural / Combined | Corresponding existing path | Interpreter under existing contracts |
| Explicit Interpreter | None | Interpreter |

Source revision alone is insufficient for Structural/Combined because current
numeric values are held separately. Conversely, a structural key is not an
Exact key: an Exact shader embeds the numeric source revision. The existing
schema key deliberately excludes materials, transforms and instance IDs;
those are supplied through current GPU buffers. Backend, shader configuration
and device are not serialized into the compiler key because the entire cache
is renderer/device-local, and `SEMANTICS_VERSION` covers emitter semantics.
This local ownership is a compatibility condition, not a cross-device cache
format.

## Reproducible editing baseline and policy result

The checked-in `showcase.json` and `bridge.json` were used unchanged. The
showcase file SHA-256 was
`8233458b86094fa452c04666084ffc3f5d91774fb8711a6e3510da369c07624c`.
The existing World Lab 600-frame profile performs deterministic camera,
material, transform and source edits, including 23 distinct first-instance
revisions, a topology change in its heterogeneous variant and an invalid
edit rollback. Baseline binaries were rebuilt at the starting HEAD before
measurement. Each row below is a separate release process on Vulkan/Fifo at
1280×720. `Fallback` counts actual interpreter/compiling frames; intervals
include Fifo pacing and desktop noise. A fresh process was **not** a controlled
cold driver cache.

| 600-frame run | Requests | Fallback | Active compiled frames | Interval p50 / p95 / p99 ms |
| --- | ---: | ---: | ---: | --- |
| Baseline `Auto` | 7 | 168 | 432 Exact | 6.040 / 6.751 / 7.679 |
| Baseline explicit Structural | 1 | 3 | 597 Structural | 6.053 / 6.616 / 7.086 |
| Baseline heterogeneous `Auto` | 0 | 600 | 0 | 6.058 / 6.602 / 7.178 |
| Baseline heterogeneous Combined | 2 | 4 | 596 Combined | 6.055 / 6.322 / 6.546 |
| New `Authoring` | 1 | 3 | 597 Structural | 6.049 / 6.507 / 15.597 |
| New heterogeneous `Authoring` | 2 | 5 | 595 Combined | 6.057 / 6.311 / 6.903 |

The first new Authoring run had six intervals above 16.7 ms and a 24.476 ms
maximum; no causal link to shader selection was established. Two subsequent
paired, alternating current-code runs used identical revision schedules:

| Pair | Auto requests / fallback | Authoring requests / fallback | Auto p50 / p95 / p99 ms | Authoring p50 / p95 / p99 ms |
| --- | --- | --- | --- | --- |
| 1 | 8 / 115 | 1 / 3 | 6.059 / 6.359 / 6.609 | 6.056 / 6.331 / 6.717 |
| 2 | 9 / 130 | 1 / 3 | 6.061 / 6.373 / 6.614 | 6.054 / 6.276 / 6.528 |

Within each paired run, the edited phase had 150 frames. Auto fell back on
112 and 127 edited frames respectively; Authoring fell back on **zero**.
The World Lab profile now records revision, cache hits, failures, fallback
reason, source bytes and compilation timings as well as frame stages. Each
Authoring paired run ended with 597 ready-cache selections, zero compiler
failures and one request. Its last selected shader reported 3.618 and 3.316 ms
pipeline time. Those are worker pipeline creation timings, not end-to-end
activation latency. The native profile has only two sparse GPU timestamp
samples per 600-frame run; it is unsuitable for a GPU p95 claim. Fifo paced
intervals do **not** show a reliable FPS benefit.

## Static GPU cost and heterogeneous benefit

The existing release `multi_definition_matrix` executed 234 offscreen rows
after adding an Authoring row. Its seven GPU timestamp samples per row are a
small local comparison, not a population estimate. The values below are the
reported median GPU compute milliseconds on the same mathematical worlds.

| Scene and selected paths | 720p Auto / Authoring | 1080p Auto / Authoring | 1440p Auto / Authoring |
| --- | --- | --- | --- |
| One shared topology, Exact / Structural | 0.0549 / 0.0649 | 0.1141 / 0.1283 | 0.1948 / 0.2176 |
| Four heterogeneous definitions, Interpreter / Combined | 10.433 / 0.240 | 21.988 / 0.489 | 34.156 / 0.837 |

Structural is measurably slower than Exact for the stable single-topology
fixture (about 12–18% in this matrix), so changing global `Auto` would be a
regression. Authoring is selected only by the editing application. For the
heterogeneous fixture, the huge gap is between the interpreted and already
compiled paths; it does not include first-use compilation or Fifo presentation.
The 1440p heterogeneous Authoring p95 had one noisy 1.291 ms value versus
Combined's 0.851 ms; no universal equivalence or speedup is inferred.

## Correctness and reliability

**Tested:** Existing real Vulkan mixed-image tests now request Authoring for
both one and several topologies, compare with the interpreter, and preserve
the previous Structural/Combined controls. A diagnostic readback checks the
Authoring-selected Combined path against the interpreter for hit identity,
definition revision, material, distance, normal, uncertainty and exhaustion.
The extended mixed test's reported six 640×360 cases had zero maximum RGB
channel delta for Combined versus Interpreter and no visual guard delta.
The real `structural-before/after.bmp` and
`combined-interpreter/compiled-640.bmp` captures were inspected; the edited
sphere visibly changed size and the compared Combined/Interpreter BMPs have
the same SHA-256. They are selected cases, not exhaustive visual proof.

The completion boundary is now `CompilerCache::poll_finished`. A deterministic
unit test delivers controlled failures representing source generation, shader
validation, pipeline creation and a late revision, followed by a disconnected
worker. It verifies pending resolution, error retention, the four-entry bound,
failed-entry eviction and no activation in this cache state. This is an
injected completion test, **not** a real wgpu validation or device-fault test.
The existing exact-revision GPU churn test exercises cache eviction and
current-revision readback. Existing GPU lifecycle tests exercise obsolete
topologies and return to a prior key. A new test begins an Authoring compile,
discards that renderer, creates a new wgpu device and renderer, verifies the
empty new cache, then obtains interpreter and compiled readback from the
unchanged authoritative world. No old pipeline crosses that boundary.
Actual device-loss recovery is unsupported and was not simulated.

**Known limits:** The cache still has one in-flight request and four entries.
The World Lab sequence had at most two requests under Authoring, so it did
not justify a larger cache, coalescing or a scheduler. The failure test does
not establish that every real wgpu validation failure is recoverable; nor
does it inject a successful obsolete completion in a controlled worker
ordering. Existing key-gated GPU tests cover stale selection in naturally
observed transitions, with scheduling dependence. Queue delay, cache memory
and worst-case shader first-use latency were not separately measured. Real
external-file reload and invalid-input rollback pass their existing tests;
the 600-frame native performance scenario edits `WorldState` directly and is
not a substitute for timed end-to-end external reload. Human keyboard/mouse
usability was not tested.

## Numerical, backend and architecture decisions

The policy changes no field operators, visual guard, ray budget, persistence
or simulation laws. Existing opt-in suites include near-tangent, bounded
visual-guard, dense-world, GPU BVH, dynamic parameters and Living World
regressions. Their pass is evidence for those fixtures, not a proof of
conservative f32 visual culling. The established `y = 1 + 1e-8` and 496
hidden-Exhausted counterexamples remain; diagnostic mode bypasses the visual
skip. No tolerance was widened.

Default DX12/FXC was retried once and failed with `X3511` at translated
analytical shader lines 111 and 465. Inspection found bounded and dynamic
loops in `analytic.wgsl`, but did not isolate one source construct with a
controlled equivalent replacement. No shader rewrite was accepted. An
already installed Windows SDK x64 `dxcompiler.dll` passed the targeted mixed
diagnostic on the **same** RTX 4070 SUPER through DX12. That DLL is not a
shipped dependency or a distribution solution. No second physical GPU or
Linux native GPU was tested.

Exact, Structural and Combined currently share operation emission in
`field_compiler::field_source`; Combined still rewrites generated WGSL text
fragments. This is a maintainability risk, but no observed operator drift or
generator failure justified an IR or source-emitter rewrite in AC-3. The
semantics version and bounded source size remain unchanged. The compiler is
understandable for the present bounded grammar; external SDK readiness is
not established.

## Qualification and decisions

| Check | Result |
| --- | --- |
| Windows fmt, strict Clippy, locked build, ordinary workspace tests, rustdoc | Pass: 177 ordinary tests, zero failures |
| Windows release opt-in workspace suite | Pass: final 84 tests, zero failures |
| Targeted new Vulkan mixed, recreation and DX12/DXC readbacks | Pass |
| Living World 1–4 release CLI experiments | Exit 0; replay equality reported by 2–4 |
| Historical v7–v10 fixture continuation | Pass: 1/1 |
| WSL fmt, strict Clippy, locked build, ordinary tests, rustdoc | Pass: 177 tests, zero failures, one job, separate `/tmp` target |
| `cargo deny check advisories licenses` | Pass |
| Astro site build and local links including this page | Pass: 210 pages, 21,748 links; existing nonfatal sitemap warning |
| Hosted CI, manual World Lab interaction, second physical GPU | Not verified |

| Improvement | Decision | Reason |
| --- | --- | --- |
| Explicit Authoring preference in World Lab | **CONDITIONAL ACCEPT** | Strong reduction in recompiles and fallback on the measured edit schedule; same compiled paths and GPU parity; stable GPU cost is higher than Exact. |
| Renderer-local completion test seam | **ACCEPT** | Small extraction permits deterministic failed/disconnected completion tests without another compiler abstraction. |
| Increase four-entry cache or add debounce | **REJECT for now** | No measured Authoring cache churn in the qualified sequence. |
| Generator consolidation and DX12 shader rewrite | **INCONCLUSIVE / DEFER** | No proven source defect or qualified cross-backend fix. |
| Production compiler claim | **NO-GO** | Real validation/pipeline faults, driver first use, more hardware and end-to-end external reload performance remain incomplete. |

Next priorities, by evidence: (1) deterministic successful stale-completion
and real validation-failure boundary qualification; (2) instrumented
external-file authoring reload on more than one GPU tier; (3) a distributable
DX12 compiler path or a proven FXC-safe shader correction. Do not globally
replace Exact, enlarge caches, introduce a scheduler or change mathematical
operators on this evidence.

## Reproduction

From the repository root with Rust 1.94, a native Vulkan adapter and a
release build, set `GENESIS_WORLD_LAB_PROFILE` to a writable JSONL path,
`GENESIS_WORLD_LAB_PROFILE_FRAMES=600`, and
`GENESIS_WORLD_LAB_PROFILE_SCENARIO=auto` or `authoring`, then run:

```powershell
cargo run --release -p world-lab --locked -- examples/world-lab/scenes/showcase.json
cargo test --release -p analytic-renderer --test mixed_gpu --locked -- --ignored --test-threads=1
cargo test --release --workspace --locked -- --ignored --test-threads=1 --nocapture
```

Use `heterogeneous-auto` or `heterogeneous-authoring` for the checked-in
third-definition control. The complete ignored local evidence package is
`target/ac3-review/`; it is not committed or published.
