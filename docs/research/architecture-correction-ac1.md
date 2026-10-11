# Genesis Architecture Correction — AC-1

**Status:** local correction and bounded qualification, 2026-10-11. **Starting
branch/HEAD:** clean `living-world` at
`ceea6df56683d96efa2440d23635e04bb7de1c18`. The final report commit is
identified by `git log -1` in the delivery record; a commit cannot contain its
own hash. **Verified implementation checkpoint:**
`d6f414bc77aa3b0db34f71856c39fb90661820eb`. Neither `main` nor
`efficiency-research` was changed. No remote
operation was performed.

## Evidence and workload

The actual saved fixture is `target/qa2/dense-world.json` (ignored, 284,298
bytes), SHA-256
`2a354e779afa51936a483fc884327e7b34d68356bbf10996f1541559ca0fa9d4`.
It is reproducible from the checked-in `dense_authored_world_reaches_combined_limits`
test with `GENESIS_QA2_FIXTURE_OUT`. `Runtime::load_file` and `WorldState::validate`
accept it: 512 ordinary spheres, four 48-node organisms, four sources, eight
authored occurrences with 4,096 segments, eight Field Graph occurrences, one
body, and 4,993 derived analytic primitives. The static BVH has 3,841 nodes
at depth 12. Raw JSONL and test logs remain under ignored `target/qa2/`; this
report is the compact checked-in evidence summary.

Windows 11, Ryzen 5 7500F, 32 GiB RAM, NVIDIA RTX 4070 SUPER driver 591.86,
Vulkan/wgpu 30.0.1, Rust 1.94.0. These are **one machine and one physical GPU**.
GPU timestamps are sparse compute-pass samples; `queue.write_buffer` timings
measure CPU enqueue, not transfer completion. Each native profile uses a real
surface, 64 warmup frames and 1,000 measured frames. The camera yaw follows
the same function of frame index. Fifo runs are compared with Fifo; Immediate
is a separate control. Desktop activity, compositor scheduling and driver
caches were not controlled.

## Root cause and correction

Before AC-1, `App::redraw` called `App::scene()` for picking and display
metadata, then `GpuRenderer::draw_mixed_with_execution` called
`Scene::from_world` again. Both constructed and validated the same analytic
world snapshot. The application already needed its scene in the frame, so
the second derivation was redundant. The renderer still needs `WorldState`
for field occurrences and authoritative parameter/revision data.

`GpuRenderer::draw_mixed_prepared` now accepts the current `(world, scene)`
pair. The native mixed frame passes the freshly derived scene. The existing
`draw_mixed_with_execution` API retains its own validation, construction,
error behavior and scene-stage timing for other callers. No persistent scene
cache, `WorldState` clone, new authoritative state or renderer policy change
was added. The native frame rebuilds its snapshot after each simulation tick,
reload or edit; camera-only changes cannot leave a camera-dependent scene
behind. Surface resize or renderer recreation also retains no snapshot cache.
Callers of the new explicit API must pair a snapshot with the world from which
it was derived. The `Scene::from_world` constructor validates that world;
the renderer cannot independently prove that a caller did not later mutate a
different `WorldState`. The native path enforces this by local lifetime and
frame order.

The alternative of a revision-keyed persistent cache would add invalidation
for growth, source markers, authored geometry, materials, field parameters,
load/reload and renderer ownership. The measured duplicate had a simpler
solution. Reusing the scene across frames or changing the legacy API would
increase stale-data or compatibility risk without a measured need.

## Correctness qualification

The ordinary and opt-in workspace suites cover existing static scene,
camera, world edit, save/load, invalid input, organism growth, compiler
revision/fallback, numerical oracle, World Lab and The Passage paths. The
native `App::scene()` continues to serve picking and the renderer in each
mixed frame. A GPU regression renders the same world through the legacy and
prepared entry points and compares actual pixel readback; the compiler
lifecycle test then uses the prepared path over camera/material/revision
changes. A second diagnostic regression changes a live sphere radius,
confirms an additional node upload, and checks Interpreter versus active
Combined identity, definition, material, status, depth and normal; a pending
Combined request is compared with Interpreter fallback.

The real dense dynamic readback test uses the full fixture generator, source
moves at ticks 30 and 60, four graph topologies, 289 rays per checkpoint, and
ticks 0/30/60. It observed 87 field hits at each checkpoint. Interpreter and
**active** Combined agreed on hit/field/uncertain/exhausted/ambiguous flags,
identity, definition revision and material; selected depth differed by less
than 0.02 world units and each normal component by less than 0.04. At least
one selected field hit with an explicitly assigned material was asserted at
each checkpoint. One
Combined compile request was recorded. These are differential GPU checks,
not an independent proof of the field equations. Existing QA-1/QA-2 CPU and
independent f64 oracles retain their narrower tested domains.

For the native dynamic experiment, each profiled frame advances exactly one
fixed simulation step. Both modes start from identical saved bytes, move the
same source every 30 ticks, use the same camera, and finish at tick 1,064.
At 720p and 1080p, every recorded tick, source position/resource amount and
primitive count matched. The complete serialized final `WorldState` values
were equal. The ordinary runtime's invalid-event atomicity and v7–v10
historical-save tests ran separately; no invalid event was injected into a
timed window run.

## Static before/after, real window

Values are milliseconds, measured-frame p50 unless otherwise stated. The
**before** binary was built from the verified starting HEAD. The **after**
binary has the shared-scene correction. Static world, Fifo, 1,000 frames per
row. `App + renderer scene` is the sum of two separately measured medians,
so it is an approximate preparation comparison, not a per-frame sum.

| Resolution | App scene before/after | Renderer scene before/after | Approx. scene CPU total before/after | Presented interval p50 before/after | GPU compute p50 before/after (sample count) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 720p | 0.731 / 0.735 | 0.735 / 0 | 1.466 / 0.735 | 8.888 / 8.616 | 8.451 (40) / 8.402 (40) |
| 1080p | 0.726 / 0.843 | 0.741 / 0 | 1.467 / 0.843 | 18.846 / 19.059 | 19.091 (49) / 18.457 (48) |

The direct CPU saving is approximately 0.73/0.62 ms at 720p/1080p. Median
presented interval improved by 0.272 ms at 720p but worsened by 0.213 ms at
1080p in separate runs. GPU timestamps and surface acquisition also varied,
so no causal frame-interval gain is established. Static measured frames
had zero BVH rebuilds in both versions. The old wall-clock-driven dynamic
profiles are retained under `target/qa2/ac1-before-dynamic-*.jsonl`, but are
**not** a matched A/B with the new one-tick-per-frame dynamic schedule.

## Matched dynamic Interpreter versus Combined

Each mode pair used one corrected release binary and the same fixture,
event sequence, camera path and 1,000 post-warmup frames per mode. The Fifo
pairs were rerun after a profiler correction: a resize-triggered skipped
redraw now cannot advance the simulation without recording a frame. The
profiler also aborts if a recorded dynamic frame's tick diverges from its
expected sequence; a focused unit test and a native 10-frame smoke passed.
The four 1,000-frame Fifo logs were collected after the skip correction and
before this additional self-check, which does not change tick or rendering
behavior. The 720p Immediate pair predates that correction but had zero resize recoveries;
its per-frame tick, source state and final world were independently checked
equal. Fifo and Immediate intervals include their different presentation
behavior. GPU p50 is computed only over sparse timestamp samples; frame p50/p95/p99 uses all
1,000 presented intervals. Counts use thresholds 16.7 and 33.3 ms.

| Surface | Mode | Interval p50/p95/p99 ms | >16.7 / >33.3 | GPU compute p50 ms (n) | BVH builds in measured frames |
| --- | --- | ---: | ---: | ---: | ---: |
| 720p Fifo | Interpreter | 8.967 / 14.598 / 33.657 | 37 / 11 | 7.710 (41) | 33 |
| 720p Fifo | Combined | 6.058 / 7.644 / 10.493 | 1 / 0 | 0.911 (37) | 33 |
| 1080p Fifo | Interpreter | 15.866 / 24.855 / 72.161 | 420 / 38 | 15.343 (47) | 33 |
| 1080p Fifo | Combined | 6.059 / 7.607 / 10.550 | 2 / 0 | 1.422 (36) | 33 |
| 720p Immediate | Interpreter | 8.697 / 14.914 / 25.439 | 39 / 1 | 8.267 (40) | 33 |
| 720p Immediate | Combined | 2.297 / 3.400 / 6.598 | 0 / 0 | 0.570 (35) | 33 |

All three Combined profiles selected Interpreter for frame 0 while one worker
request was pending, then selected Combined from frame 1 onward. There were
zero compiler failures and one request per run. Last selected shader-source
generation was 0.065–0.095 ms and worker validation/pipeline creation
3.545–5.169 ms in these warm-driver-cache runs. They do **not** measure a
genuinely cold driver cache or bound first-use stalls on other hardware.
The native dynamic runs each recorded 140 cumulative field-node uploads and
36 cumulative BVH builds (initial plus 35 source moves); 33 builds occurred
after warmup. The corrected Fifo runs matched every tick, source position,
source resource amount and final serialized world despite 0–2 window resize
recoveries. The earlier rerun without the profiler fix is excluded: resize
skips created one or two extra ticks in one member of a pair. No stale
numerical parameter selection was observed in the
diagnostic corpus, and the compiled key includes topology while dynamic
numbers are read from uploaded storage. Compiler-cache compatibility with
arbitrary future graph edits is not proven by these runs. `Auto` remains on
Interpreter for dynamic morphology/parameters, as before. This A/B supports
considering a **separately reviewed** dynamic policy change; it does not
change the default in AC-1.

## Visual guard decision record

The WGSL guard runs only with `spatial_guard=true` and `diagnostic=false`.
`bounds_culled=true` means at least one field occurrence was skipped by an
empirically widened f32 slab bound. The skipped tracer was not run. Its
possible `Uncertain` or `Exhausted` status is unknown, and an apparent miss
is **not** a certified numerical `BoundsMiss`. A caller requiring actual
tracer status uses `diagnostic=true`; the shader then traces without the
visual cull. The `MixedGpuResult` and `GpuFieldRenderMode` comments and the
Field Graph specification now state this contract.

The one-iteration 64×64 GPU regression reproduced **496** visual culls
hiding **496** otherwise reported `Exhausted` results. Diagnostic mode
preserved all of those outcomes and reported no culls. This proves hidden
tracer uncertainty, **not** 496 missed mathematical surfaces. The QA-1
`y = 1 + 1e-8` rounding case, inside starts, tangencies, small scales and
mixed-depth tests remain in the opt-in suite. The full f32 guard has no
cross-adapter conservative floating-point proof; ordinary visual use remains
conditional, and certified absence must use the separate CPU query contract.

## BVH invalidation finding

In the 4,993-primitive fixture, CPU snapshot comparisons across the tick-30
and tick-60 `MoveSource` events each found exactly one changed analytic
primitive: index 892, `Source(EntityId(4))`. Geometry order and count stayed fixed.
The renderer compares GPU primitive records and marks its BVH dirty when any
record differs. Thus each source move rebuilds the whole BVH. Measured
median build/flatten cost on affected corrected dynamic frames was about
3.98–4.27 ms, with 33 affected measured frames per 1,000. Unchanged static
frames did not rebuild. A one-leaf refit is technically conceivable, but the
Combined Fifo p99 was under 10.6 ms with only 1–2 isolated intervals above
16.7 ms; none of those three outlier frames rebuilt the BVH. A safe
refit needs parent links, conservative bounds and more invalidation tests.
Its likely median-frame benefit is small at the current event rate.
**Decision:** retain the deterministic full rebuild; defer refit until a
measured workload misses its frame-tail target because of BVH construction.

## Regression and limits

Windows: rustfmt, strict Clippy, locked build, ordinary workspace tests
(176 passed, 0 failed, 83 opt-in ignored), rustdoc, cargo-deny, Living World
1–4 CLI experiments (`replay_equal=true` in all four), authentic v7–v10
fixtures, dense numerical readback and the visual guard regression passed.
The serial opt-in workspace suite passed 83 tests, 0 failed, including
the source-delta probe; after strengthening the material-hit assertion, that
affected GPU test passed again in a focused release run. The Passage and World
Lab automated paths are in the workspace suites; no manual playtest was
claimed. Linux/WSL rustfmt, strict
Clippy, locked build, ordinary tests (176 passed, 0 failed, 83 ignored) and
rustdoc passed with one build job and incremental compilation disabled. The
site built 206 pages and its link checker verified 20,918 local links; the
existing nonfatal missing-`site` sitemap warning remains. Hosted CI, a
second physical GPU and a cold driver-cache run were not executed locally.
The known default DX12 FXC `X3511` issue is unchanged; no driver or deployment
setting was modified.

## Acceptance and next work

| Question | Result | Reason |
| --- | --- | --- |
| A. Duplicate preparation removed without stale derived state? | **PASS** | One fresh native scene per frame; legacy API and field authority retained. |
| B. CPU and presented-frame benefit? | **PARTIAL** | About 0.62–0.73 ms direct CPU saving; static interval medians moved in opposite directions across resolutions. |
| C. Visual skip distinct from certified absence? | **PASS** for API contract; **NO-GO** for certification | Flag and unguarded diagnostic path are explicit; f32 guard remains empirical. |
| D. Same changing world under Combined? | **PASS** in tested fixture | Exact final world equality and 0/30/60 dense diagnostic parity. |
| E. Reproducible dynamic p50/p95/p99 gain? | **PARTIAL** | Large matched single-adapter gains; no independent repeated-session or second-adapter qualification. |
| F. Bounded first-use compilation? | **PARTIAL** | One fallback frame and no failure in warm-cache runs; cold driver-cache tail unresolved. |
| G. Targeted BVH optimization justified now? | **NO-GO** | One changed marker and 33/1,000 rebuilds; none of the three Combined outliers rebuilt it. |
| H. Complexity reduced? | **PASS** | One redundant derivation removed without a cache framework or new world state. |

Next narrow work: repeat the matched dynamic experiment on an independent GPU
and a documented cold-ish process/cache state, then evaluate an opt-in dynamic
`Auto` policy with per-revision diagnostic parity. Separately, derive a valid
f32 exclusion-error budget before making any certified GPU guard claim.
Do not increase world limits, introduce a refit framework, create new
organisms, or broaden the compiler policy based on this single-machine run.

### Reproduction

From the repository root in PowerShell, with Rust on `PATH`:

```powershell
$env:GENESIS_QA2_FIXTURE_OUT = (Join-Path (Get-Location) 'target/qa2/dense-world.json')
cargo test --release -p first-light --test qa1_valid_world --locked dense_authored_world_reaches_combined_limits -- --ignored --nocapture
cargo test --release -p first-light --test qa1_valid_world --locked dense_dynamic_combined_diagnostic_parity -- --ignored --nocapture
cargo test --release -p first-light --test qa1_valid_world --locked dense_source_move_primitive_delta -- --ignored --nocapture
cargo test --release -p analytic-renderer --test qa2_guard --locked visual_guard_exhaustion_contract -- --ignored --nocapture
$env:GENESIS_WINDOW_SIZE = '1280x720' # repeat with 1920x1080
$env:GENESIS_QA2_FRAMES = '1000'
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json dynamic target/qa2/ac1-interpreter.jsonl
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json dynamic-combined target/qa2/ac1-combined.jsonl
```

Run the two native profiles sequentially on an otherwise idle desktop. For
static before/after, the pre-correction executable must be built from the
recorded starting commit without changing the current branch; the retained
local `ac1-before-static-*.jsonl` captures are the actual baseline. Do not
compare the old wall-clock-driven `dynamic` profile with the fixed-step AC-1
`dynamic` profile as if their event schedules matched.
