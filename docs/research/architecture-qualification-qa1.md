# Genesis Architecture Qualification — QA-1

**Status:** bounded local qualification, 2026-10-11. **Source baseline:**
`living-world` at `63d4389db32cc0c1c7f7e8868cccfb85a2dbf205`.
**Verified implementation HEAD before this report commit:**
`11a84e2b4c4d05379a9e2c2820861305a7748073`. The final documentation
commit is identified in the delivery report and Git history. This report is
about observed behavior on the stated machine and finite workloads. It is not
a proof of correctness for arbitrary mathematical fields or a production
performance claim.

## Evidence and environment

The baseline tree was clean. QA-0 and the restored Living World 4 report were
read before changes. Windows 11 Pro 10.0.26200, Ryzen 5 7500F (6 cores/12
threads), 32 GiB RAM, NVIDIA RTX 4070 SUPER driver 591.86, Vulkan, Rust/Cargo
1.94.0, Clippy 0.1.94, rustfmt 1.8.0, cargo-deny 0.20.2, Node 24.14.0 and
npm 11.9.0 were used. Linux checks used WSL Ubuntu 24.04.4 and Rust 1.94.0,
with one build job, incremental compilation disabled and a separate target
directory. Vulkan and DX12 on this computer are two backends on the **same**
physical GPU. No second adapter or hosted CI run was available.

The authoritative capacity contracts are in `crates/world/src/life.rs:8-20`
(512 spheres, four organisms of at most 48 nodes, four sources),
`crates/world/src/authored.rs:13-18` (four definitions, eight instances, 4096
total derived segments), `crates/world/src/field.rs:13-15` (four definitions,
eight instances), and `crates/authoring/src/field_graph.rs:20-28` (32 graph
nodes, depth 12, 64 visits, four parameter channels). Saves v7–v10 are
accepted in `crates/runtime/src/persistence.rs`; graph tracing uses the
`GraphTracePolicy` tolerance of 0.001 in
`crates/renderer/src/field_graph_gpu.rs:55`. The GPU visual guard widens f32
bounds empirically in `crates/renderer/src/gpu.rs:1456-1486`. The compiler
cache holds four pipelines (`gpu.rs:510`). These are independent limits; the
test worlds below passed the **combined** production validator.

`target/qa1/` holds raw local logs, JSONL profiles, extracted historical
producers and large temporary builds; it is ignored by Git. Compact source
tests and historical JSON fixtures are tracked. `.gitattributes` disables
line-ending conversion for those fixture bytes on future Windows checkouts.
Timings in different tables
come from different clocks and must not be added or interpreted as the same
measurement. The release CPU probes use `Instant`, three warmups and 11–51
samples. GPU compute uses timestamp queries. Offscreen time includes waiting
for readback and excludes a displayed window. The window interval includes
Fifo presentation pacing. Percentiles are nearest-rank values from the
recorded samples; a seven-sample p95 is just the maximum.

## Qualification matrix

| Area | Result | Evidence and limit |
| --- | --- | --- |
| Frozen QA-0 and Living World 4 baseline | PASS | QA-0 scaling and representative GPU oracle reproduced; LW1–LW4 CLI experiments and v10 replay exited successfully, replay tick 240 digest `9103ee2194ffe042`, `equal: true`. |
| Independent spatial oracle | PARTIAL | Finite f64 sphere, transformed box, two-sphere hard union and axial capsule corpora passed in their unambiguous subsets. Smooth union and arbitrary graph surfaces lack independent coverage. |
| Spatial guard and uncertainty | PARTIAL | No silent supported hit loss in the qualified subset with guard on or off; one-iteration `Exhausted` and inside-start `Uncertain` remained visible. Empirical widening has no general conservative proof. |
| Maximum valid world | PASS for tested construction | 512 spheres, 192 organism nodes, four sources, four field definitions/eight instances, four authored definitions/eight instances and a body validate, save/load and tick together. Dense variant reaches 4096 authored segments and 4993 analytic primitives. This is not a proof that every individually valid maximum composes. |
| Historical persistence | PASS for four fixtures | Bytes emitted by original v7, v8, v9 and v10 producer commits load, re-save and continue three ticks as the historical producer did. Other legal historical states remain untested. |
| Compiler churn | PARTIAL | Seven requests, four residents, eviction and interpreter fallback retained tested identity/depth/normal. Obsolete work, invalid reload, unsupported graph and every parameter-layout combination were not independently stressed here. |
| Real window behavior | PARTIAL | Two bounded 600-frame World Lab windows passed; the near-capacity world was measured offscreen, not in a window. GPU timestamps were sparse in window profiles. |
| DX12 portability | PARTIAL | Existing FXC path reproduced shader error `X3511`; the installed Windows SDK DXC control passed on the same card. Other driver/card combinations are unverified. |
| Cross-platform local regression | PASS for listed commands | Windows workspace/opt-in tests and Linux ordinary tests; Linux native GPU opt-in tests and hosted CI are unverified. |

Final local Windows commands `fmt`, strict Clippy, build, ordinary workspace
tests, all opt-in workspace tests, rustdoc and cargo-deny all exited 0.
Ordinary tests: **175 passed, 0 failed, 77 ignored**; the separate opt-in run:
**77 passed, 0 failed**. Release QA-1 numerical/churn/valid-world probes:
**4/1/5 passed** respectively; the release QA-0 scaling probe passed 2/2.
Final WSL fmt, strict Clippy, build, ordinary tests and rustdoc exited 0;
Linux ordinary tests: **175 passed, 0 failed, 77 ignored**. The site built
**202 pages** and checked **20,114 local links**. Its existing nonfatal
sitemap warning says `site` is not configured; this did not fail the build.

## Independent numerical oracle

`crates/renderer/tests/qa1_numerical.rs` uses independently coded f64
ray/sphere quadratic roots, an f64 slab-face expectation for boxes, the
minimum of two sphere-entry distances for a hard union, and closed-form f64
side/cap entry for an axial capsule. It uses actual GPU
diagnostic readback, including identity, depth, normal, uncertainty and
exhaustion; it does not judge only RGB. The deterministic fixture sources and
rays are in the test. It covers scales 0.01–100 for spheres and 0.1–10 for
boxes, world translation 0 or 999, inside starts, near tangency and guard
enabled/disabled for Field Graph probes; the analytic capsule compares CPU,
direct GPU and GPU BVH traversal. The reference is **analytical within f64 arithmetic**, not
interval-certified. Bounds and shader operations use f32, so the test excludes
a deliberately broad band near the surface: `abs(offset - 1) * scale >
4*tolerance` for spheres, half that transverse distance for boxes, and
`abs(offset - 1) > 0.02` in both. The depth tolerance is
`max(0.003, 0.003*scale)`; normal component tolerance is 0.03. These were
chosen before classifying results and were not increased to make failures
pass. Rays in the excluded band are recorded as ambiguous, **not passes**.

| Corpus | Qualified hit | Qualified miss | Ambiguous | Observed qualified errors |
| --- | ---: | ---: | ---: | --- |
| Sphere, guard on/off | 32 | 12 | 56 | 0 false misses, false hits, wrong depths or hidden uncertainty. |
| Transformed box, guard on/off | 30 | 16 | 34 | 0 asserted hit/depth/identity/normal or miss errors. |
| Hard union, guard on/off | 10 rays | — | — | 0 first-boundary, identity, depth or normal errors. |
| Axial capsule, direct/BVH GPU | 8 per path | 22 per path | 5 excluded rays | 0 hit/miss, identity, depth or normal errors against the independent closed form. |

The retained case `y = 1 + 1e-8`, scale one, x zero has f64 miss but direct
GPU and mixed GPU hit at about 3.9557457. It lies **inside** the excluded
uncertainty band and shows why a boundary pixel is not an exact sign oracle.
It is neither counted as a qualified false hit nor silently called correct.
The capsule's excluded five rays are within 0.01 of the squared-radius
boundary; the 30 qualified rays include side, endcap and clear misses. It
does not test every capsule orientation or near-degenerate endpoint spacing.
One-iteration box tracing reported `Exhausted`; an inside-box start reported
`Uncertain`, rather than a false `Miss`. The direct `GraphGpuRenderer` API
rejects scale 0.01 when its normalized `far/scale` exceeds 10,000; the native
mixed viewport handled that scale. This is an explicit API domain rejection,
not a silent renderer miss. No independent smooth-union root
bracketing, near-coincident competing-depth proof, arbitrary graph interval
bound or nonfinite-intermediate proof was completed. Existing differential
GPU tests cover some of these cases but share implementation assumptions.

**Conclusion:** no supported hit loss was observed in this bounded,
unambiguous corpus. The empirical f32 guard is not proven conservative for
all supported transforms and fields. A future correctness claim needs either
a mathematically justified restricted domain, a conservative interval bound,
or a visible fallback on cases where the bound cannot be established.

## Valid world and scaling measurements

`examples/first-light/tests/qa1_valid_world.rs` builds real validated worlds
with different graph definitions, mutable field parameters, organism trees,
resource sources, authored geometry and one kinematic body. The 128/256/512
sphere series has 4×48 organism nodes, four sources, eight field instances
from four definitions and eight authored instances throughout. Each row is 21
release samples after three warmups; milliseconds are p50/p95. `tick` measures
one deterministic `Runtime::tick()` from a cloned initial state and excludes
the clone; the other rows time only the named operation. `snapshot` includes
scene insertion. The authored definitions in this series are smaller than the
dense cap case.

| Spheres / analytic primitives | Validate | Tick | Scene snapshot | BVH build | Serialize | Deserialize |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 128 / 677 | .015/.032 | .198/.291 | .180/.220 | .429/.676 | .291/.469 | .257/.480 |
| 256 / 805 | .017/.019 | .242/.297 | .314/.466 | .544/.773 | .396/.493 | .370/.446 |
| 512 / 1061 | .024/.028 | .335/.417 | .440/.570 | .619/.808 | .446/.557 | .338/.383 |

The denser **still valid** construction has 512 spheres, 4×48 nodes, 4096
authored segments, four sources, eight field instances and 4993 analytic
primitives, with 3841 BVH nodes of depth 12. Eleven release samples after
three warmups: validation .070/.077 ms, snapshot .924/1.139 ms, BVH build
3.516/3.606 ms, tick 1.585/1.669 ms; save size 284,290 bytes. Save/load
equality passed. No custom renderer material override was included. It is a
constructed simultaneous limit case, not a measured
worst case over all valid worlds or graph topologies. Resource allocation,
parent lookup, field derivation and buffer packing were not timed as separate
phases, so attributing the tick or snapshot total to one of them would be
speculation.

`crates/simulation/src/life.rs:39-65,384-393,416-425` still performs nested
parent scans. A separate one-tick probe with four organisms measured node
counts 1/24/48 at .036/.049, .074/.117 and .109/.165 ms p50/p95 (51 samples).
The graph of these totals is not evidence of current frame failure or an
isolated complexity exponent. Keep the algorithmic risk P2; a parent-index
rewrite needs component timing and unchanged allocation/ID/replay tests.

QA-0 confirmed that `Scene::push` rescanned every earlier ID. A narrow change
in `crates/renderer/src/lib.rs:420-550` now checks a `HashSet<u32>` while
preserving primitive order, duplicate-ID rejection, clone behavior and
fallible allocation handling. The existing scene, BVH and native parity
tests still pass. Separate local release runs, not paired A/B trials:

| Workload | Before p50 | After p50 | Interpretation |
| --- | ---: | ---: | --- |
| 4096 standalone renderer-only pushes | 3.556 ms | .363 ms | Measured removal of the quadratic scan; this scene is not an authoritative world. |
| Dense validated 4993-primitive snapshot | 6.281 ms | .924 ms | Material effect inside supported combined world. |
| Dense BVH build | about 3.5–3.7 ms | 3.516 ms | Dominant remaining CPU construction cost; no claimed improvement. |

The index adds host memory whose exact cost was not measured. Repeated
`Scene::from_world` and field validation/packing on every mixed draw remain
in `crates/renderer/src/gpu.rs`; cache invalidation tied to world revisions
would require an independent correctness design. No capacity limit changed.

## GPU and frame observations

The dense valid world was rendered at 1280×720 on the Vulkan RTX 4070 SUPER.
The release offscreen probe records two warmups and seven measured frames.
Values are milliseconds. Its first interpreter frame had 319,552 primitive
bytes and 245,824 BVH bytes, CPU write **enqueue** .066 ms, BVH build/flatten
4.713 ms and 8.987 ms total offscreen in the final run. Enqueue time does
not include GPU upload completion. The renderer allocated 262,144 BVH bytes
and rebuilt once;
warm frames reported zero primitive/BVH bytes enqueued. The first requested
Combined frame may still execute Interpreter while compilation completes;
the test prints the **actual active** mode for that frame.

| Warm execution, 720p | CPU submit p50/max | GPU compute p50/max | Full offscreen p50/max |
| --- | ---: | ---: | ---: |
| Interpreter | 1.200/1.337 | 3.309/3.632 | 4.707/5.000 |
| Combined | 1.205/1.635 | .310/.312 | 1.604/2.167 |

A smaller 1061-primitive valid mixed world was also measured at 720p, 1080p
and 1440p in the `combined_world_offscreen_gpu_frame` test. For the final
local run, offscreen p50 was about 4.05/7.73/11.67 ms in Interpreter and
.95/1.26/1.73 ms in Combined. This is a distinct scene and measurement from
the dense table. These offscreen figures exclude swapchain presentation,
input, full application simulation and steady interactive pacing.

World Lab's built-in bounded profiler produced two 600-frame JSONL files at
`target/qa1/world-lab-heterogeneous-{auto,combined}.jsonl`, Vulkan/Fifo.
The Auto window used Interpreter for all 600 frames and requested no compile.
The Combined window used Interpreter/compiling for 113 frames and Combined
for 487, with two requests. Presentation interval p50/p95/p99/max was
6.063/6.266/6.523/8.087 ms for Auto and
6.053/6.296/6.631/6.908 ms for Combined. Neither run recorded an interval
over 16.7 or 33.3 ms. Surface acquisition p50 was about 5.65–5.68 ms, so
Fifo waiting dominates these interval measurements. Each profile contains
only **two** GPU timestamp samples; GPU throughput and production FPS cannot
be inferred. World Lab's scene is much smaller than the dense maximum-valid
world; that world was not exercised in a native window.

DX12 with the existing FXC shader path failed while creating the pre-existing
analytic pipeline (`X3511`, process exit 101; `target/qa1/dx12-fxc.log`).
Using the already installed Windows SDK `dxcompiler.dll` passed the same
targeted native test on DX12 (exit 0; `target/qa1/dx12-dxc.log`). No driver,
system-wide graphics setting or proprietary file was changed or added.
This isolates a backend/compiler dependency for this machine, not universal
DX12 compatibility. Linux native GPU opt-in tests were not run; WSL is not a
second physical adapter.

## Authentic historical saves

The fixture provenance and individual SHA-256 digests are in
`crates/runtime/tests/fixtures/qa1/README.md`. Each source commit was
extracted with `git archive` into ignored `target/qa1/producers/`; the
historical serializer emitted both original bytes and a three-tick
continuation. A distinct Cargo target directory was used per producer. A
first shared-target experiment selected zero tests for later versions and was
discarded, not counted. The current reader compares **full validated runtime
state** after three ticks to historical continuation, plus re-save/load,
identity, field-definition availability, source counts, downgrade rejection
and atomic rejection of truncated input.

| Version | Original producer commit | Original bytes | Current load / re-save / three-tick semantic continuation |
| --- | --- | ---: | --- |
| v7 | `0b1dfaa4808b0270e676e95cc919621c2396b740` | 8,271 | PASS |
| v8 | `dd274bfe00bb9f7f4acb252c9ee96f97143890e0` | 8,504 | PASS |
| v9 | `440b6e6effead1307db69f7eca2e18190fd6e73b` | 10,166 | PASS |
| v10 | `81ea2c0f0ff6d8f09229684d0aa7f7519582d47a` | 14,880 | PASS |

These are representative original-implementation saves, not all valid old
states. Semantic equality after three ticks is stronger than current-code
round-trip, but byte-for-byte equality across serializers or arbitrary
platforms is not claimed. No save schema was changed.

## Compiler churn

`crates/renderer/tests/qa1_compiler_churn.rs` walks sequential valid
single-definition worlds through radii .5–1.2, checks the current revision,
field identity, distance and front-facing normal on **every** diagnostic
frame, and waits for specialization. All worlds respect the simultaneous
definition cap. The bounded test observed seven compilation requests, four
resident pipelines and 209 interpreter fallback iterations; requesting the
evicted .5 key increased requests to eight. Pipeline timings for individual
keys were about 291–321 ms. Each waiting iteration deliberately sleeps 10 ms,
so 209 iterations are not a real frame count, frame-time distribution or FPS
estimate. No stale pipeline was observed in this exact-key sequence.

Structural/Combined revision parity and parameter cases also pass in the
existing opt-in suite. The new churn probe does **not** cover every requested
A→B→C obsolete completion order, invalid reload, unsupported topology,
multi-definition parameter layout or renderer recreation. Cache reliability
is therefore PARTIAL, not certified. Keep interpreter fallback on uncertain
or unavailable compiled programs.

## Decisions and next gate

| Priority | Finding, source and evidence | Decision / acceptance criterion |
| --- | --- | --- |
| P1 | Spatial widening in `gpu.rs:1456-1486` is empirical. Bounded f64 probes passed, but boundary ambiguity and untested field classes remain. | Research a conservative bound or explicit restricted domain. Accept only with independent smooth-union and competing-depth references, broader capsule orientations and difficult transform/float-conversion corpus, and no silent hit/uncertainty loss in the declared domain. |
| P1 | DX12 FXC `X3511` on the tested Windows stack; DXC control passes. | Document and test an explicit supported DXC setup before claiming DX12 support; run on a second physical GPU/driver before portability claims. |
| P2 | Repeated full scene preparation in `gpu.rs` and dense snapshot .924 ms, BVH build 3.516 ms p50. | Instrument revision-keyed costs in an actual near-capacity window before changing cache policy; accept only with unchanged outputs during geometry and parameter mutation. |
| P2 | `Scene::push` quadratic ID scan was measured and locally corrected. | Keep duplicate-ID/order/fallible-allocation and GPU parity regression; monitor host memory overhead. No further scene rewrite justified. |
| P2 | Nested organism parent scans in `crates/simulation/src/life.rs` remain, but current 48-node tick .109 ms p50 in the isolated probe. | Defer optimization until parent/resource phases are isolated and authoritative replay parity is frozen. |
| P2 | Four-entry compiler cache causes measurable compilation/fallback churn; exact-key outputs stayed correct. | Expand adversarial revision/parameter and obsolete-work tests before changing Auto policy or cache size. |
| P2 | v7–v10 historical compatibility gap closed for four representative states. | Preserve fixtures and add more only when a concrete legacy migration or failure is found; no schema rewrite. |
| P3 | Native maximum-valid-world window and second adapter not exercised. | Measure bounded displayed near-capacity frames and acquire/present tails before an interactive performance claim. |

No P0 defect was reproduced. **GO** for a targeted QA-2 focused on numerical
guard contracts and complete near-capacity frame instrumentation. **NO-GO**
for a universal field correctness claim, capacity expansion, arbitrary-field
contact authority, automatic dynamic compilation policy change, public FPS
claim, showcase/SDK work or broad architecture consolidation on this evidence.

Answers to the nine QA-1 questions: Q1 PARTIAL (zero errors in the qualified
finite corpus, gaps remain); Q2 PARTIAL (empirical guard only); Q3 PARTIAL
(largest *measured* valid case is 4993 primitives with .924 ms snapshot,
3.516 ms BVH, 1.585 ms tick and 4.707/1.604 ms Interpreter/Combined warm
offscreen p50, not a universal worst case); Q4 PASS for measured scene-ID
cost and correction, PARTIAL for parent lookup; Q5 PASS for the four authentic
fixtures; Q6 PARTIAL for cache and fallback coverage; Q7 PARTIAL for bounded
World Lab and dense offscreen observations; Q8 PASS for prioritizing spatial
correctness and repeated frame preparation by evidence; Q9 **GO only for
targeted QA-2**, NO-GO for broad consolidation or expansion.

## Reproduction and verification record

From repository root, with the pinned toolchain and native packages already
installed:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture
cargo doc --workspace --no-deps --locked
cargo deny check advisories licenses
cargo test --release -p analytic-renderer --test qa1_numerical --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test qa1_compiler_churn --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p first-light --test qa1_valid_world --locked -- --ignored --nocapture --test-threads=1
cargo test -p world-runtime --test historical_saves --locked
cargo run --release -p first-light -- --living-experiment
cargo run --release -p first-light -- --living-implicit-experiment
cargo run --release -p first-light -- --living-morph-experiment
cargo run --release -p first-light -- --living-rules-experiment
cargo run --release -p first-light -- --living-morph-replay docs/research/evidence/living-world-4/history-bundle.json
cd site && npm run build && npm run check-links
```

The complete Windows opt-in command includes expensive GPU tests and requires
a native adapter; ordinary tests alone do not run them. WSL checks use
`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/tmp/genesis-qa0-target`
to avoid the resource failure observed in QA-0. The final command status and
test totals are recorded in the local `target/qa1/final-*.log` files. Hosted
GitHub CI, independent second hardware and a full near-capacity World Lab
window remain **NOT VERIFIED**.
