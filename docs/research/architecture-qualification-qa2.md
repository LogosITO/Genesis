# Genesis Architecture Qualification — QA-2

**Status:** completed bounded local qualification, 2026-10-11. **Baseline:**
`living-world` at `dd3f02d3e566782c49661164fbfe552649453d99`, clean before
QA-2. **Verified implementation checkpoint:**
`c0cbe208c8c31e481e2c60b4035f3b2025b48986`. The report commit changes
HEAD; its final hash is given by `git rev-parse HEAD` in the delivery record.
This report describes measured behavior, not a proof for arbitrary fields or
hardware. `main` (`eee79d2`) and `efficiency-research` (`bd1ffd0`) were not
changed. The tracked QA-1 report was byte-identical to the baseline; the
corruption in an externally copied version was not present in Git.

## Environment and evidence classes

Windows 11 Pro 10.0.26200, Ryzen 5 7500F, 32 GiB RAM, NVIDIA RTX 4070
SUPER driver 591.86, Vulkan/wgpu 30.0.1, Rust/Cargo 1.94.0, Clippy 0.1.94,
rustfmt 1.8.0, cargo-deny 0.20.2, Node 24.14.0 and npm 11.9.0. WSL Ubuntu
24.04.4 used Rust 1.94.0, one build job, no incremental compilation, and a
separate `/tmp` target. Vulkan and DX12 were tested on the **same** NVIDIA
adapter. No second vendor, hosted CI, or actual GPU transfer-completion timing
was measured.

The following labels are used strictly:

- **Established model:** derivation for ideal real arithmetic, with explicit
  CPU rounding assumptions where stated.
- **Tested:** finite actual GPU readback or timestamp observations on the
  adapter above. Interpreter and Combined share the shader tracing algorithm;
  their agreement is not independent mathematical proof.
- **Unresolved:** no valid oracle, no backend guarantee, or no measurement.

Raw logs and JSONL are local and ignored under `target/qa2/`. Their fixture
identity is SHA-256
`2a354e779afa51936a483fc884327e7b34d68356bbf10996f1541559ca0fa9d4`
for `dense-world.json` (284,298 bytes). The save was emitted from the same
`world(512, MAX_NODES, true)` generator used by QA-1, with only the native
`0.016666667` second step substituted for the QA-1 CPU tick probe's `0.1`
second step. It was loaded through `Runtime::load_file` and validated again
before any native profile. No geometry was reduced for the window.

## Reproduced baseline

The QA-1 release probes passed before edits: valid world 5/5, numerical 4/4,
compiler churn 1/1, historical saves 1/1. This run's exact dense validation
gave 512 spheres, four 48-node organisms, four resource sources, four field
definitions/eight field instances, four authored definitions/eight instances,
4096 authored segments, one body, and 4993 derived analytic primitives.
The BVH had 3841 nodes and depth 12. CPU release p50 from the baseline probe:
validation 0.104 ms, scene derivation 0.817 ms, BVH build 3.669 ms, isolated
tick 1.234 ms. Offscreen 720p warm p50: Interpreter GPU 3.101 ms / full
offscreen call 4.988 ms; active Combined GPU 0.309 ms / full call 1.704 ms.
These offscreen values exclude native surface presentation and are **not** a
direct A/B against the later window profiles.

## Actual GPU visual guard contract

`Graph::bounds()` in `crates/authoring/src/field_graph.rs` bounds the ideal
occupied set `F <= 0`: sphere and centered box AABBs; translated or positive
uniformly scaled child bounds; componentwise hard-union bounds; and exact-SDF
child smooth-union bounds enlarged by `width/4` per axis. The smooth-minimum
reduction is bounded by `width/4`; hard unions and smooth unions remain signed
fields, not automatically exact SDFs. CPU translation/scale arithmetic rounds
outward by one representable `f64` value under the stated IEEE-754 assumptions.
This is an ideal-geometry enclosure, not an enclosure of arbitrary GPU samples.
The separate `f64` CPU region exclusion in `field_graph/spatial.rs` is a
different contract.

The mixed renderer converts each finite `f64` axis endpoint to `f32`, subtracts
or adds `0.25 + 0.001 * max(abs(lo), abs(hi), 1)` **in graph-local coordinates**,
then applies `next_down` or `next_up` (`crates/renderer/src/gpu.rs`). This is an
empirical margin; neither the cast nor the shader evaluation has a validated
worst-case error bound. The instance's positive uniform transform uses
`origin_local = (origin_world - translation) / scale`, divides near and far by
scale, and leaves normalized ray direction unchanged. The shader's slab test
retains touching intervals (`lo == hi`), handles exactly zero direction
components separately, and declines to cull when a computed slab quotient is
nonfinite. CPU/WGSL f32 layout is exercised by actual readback, but no
backend-wide correctly-rounded-operation guarantee is claimed.

`GraphTracePolicy.tolerance` and finite-difference normal delta are **not**
divided by instance scale in the current mixed shader. They are graph-local
values, so their effective world size is multiplied by instance scale. The
former API comment implying a uniform world-unit policy was corrected in the
implementation checkpoint and `docs/specifications/field-graph.md`. The
default local tolerance is 0.001; the accepted policy range is
`[0.00001, 0.1]`. This tolerance may classify a positive field value near a
zero set as an approximate hit, while the guard is based on occupied-set
bounds. Its fixed margin is not a proof that all such threshold hits or f32
rounding effects remain inside.

The guard runs only with `spatial_guard=true` and `diagnostic=false`. A guarded
readback may set `bounds_culled=true`; that flag means a shader slab rejection
under these f32 bounds, **not** a certified `BoundsMiss`. No field trace is
executed for a culled instance, so its possible `Uncertain` or `Exhausted`
outcome cannot be reported. Diagnostic mode disables culling, preserving the
tracer's uncertainty. QA-1's `diagnose()` passed `diagnostic=true`; its
guard-on/off corpus therefore never exercised actual visual culling. This
corrects the scope of the QA-1 interpretation, not its recorded observations.

**Decision:** CONDITIONAL for visual use in the tested domain; **NO-GO** for a
general numerical-safety or uncertainty-preservation claim. The ideal bounds
are mathematically justified for supported graphs under stated assumptions.
The complete f32 visual guard has no certified nontrivial domain yet. We did
not disable it globally: that would discard a large measured speed benefit
without establishing an alternative numerical contract.

## Independent oracle and counterexample

`crates/renderer/tests/qa2_guard.rs` reads the ordinary visual path with
`diagnostic=false`. A separately coded `f64` ray/sphere quadratic evaluated
16,384 deterministic screen rays over scales 0.01, 1, and 100 and a world
translation of 999. It classified 2720 stable hits, 11,744 stable misses,
and 1920 grazing/threshold-sensitive rays as **AMBIGUOUS** rather than passing
them. The guarded shader actually culled 1984 rays. In the stable subset,
there were zero hidden definite hits, zero false hits, and zero hidden
uncertainties, with identity and qualified depth checked. The ambiguity cut
uses square-root discriminant clearance less than half the sphere scale;
finite threshold tracing is ill-conditioned near tangency. An initial stricter
depth assertion failed at pixel 988 (scale 1): independent entry 3.715470,
GPU approximate entry 3.709894. Both guarded and unguarded paths agreed;
this was a tracer-threshold depth difference, not a guard miss.

An independent analytic reduction of two symmetric exact-SDF spheres gives
the smooth-union entry on `x=0` from
`sqrt(0.9² + y² + z²) - 1.2 = 0`. A nested graph translate/scale plus an
instance transform reduces independently to a sphere centred at `x=2.5`,
radius 0.5. Eight stable rays across those two definitions passed hit,
identity, depth (within 0.02 world unit), and normal (component tolerance
0.04) checks for Interpreter and **actually active** Combined, each with
guard off/on. These share the same GPU field tracer; the formulas are the
independent oracle. QA-1 separately covered a transformed box, hard union,
axial capsule, interior start, shared face, and the `y = 1 + 1e-8` rounding
counterexample. Arbitrarily oriented capsules, all graph compositions,
near-coincident mixed surfaces, and cross-backend guard readback remain
**UNRESOLVED**, not counted as passes.

There is a concrete hidden-uncertainty counterexample. With one sphere,
64×64 rays, and a one-sample policy, 496 rays returned `Exhausted` without
the guard but were culled with `bounds_culled=true` and `exhausted=false` in
ordinary visual mode. Diagnostic mode preserved the 496 exhaustion outcomes
and reported no culls. This demonstrates that visual culling changes tracer
diagnostic semantics; it does **not** establish a lost mathematical surface
hit on those 496 rays. A caller requiring uncertainty preservation must use
the unguarded diagnostic path. No known definite false miss was found in the
qualified finite subset; absence outside it is unverified.

### Matched guard cost, 720p offscreen

Each cell is 15 post-warmup GPU timestamp samples, release build, Interpreter,
same scene/camera for guard off/on within the test; p95 is the largest sample.
The rerun is the final `target/qa2/guard-tests-final.log`.

| Field workload | Guard off p50/p95 ms | Guard on p50/p95 ms | Observation |
| --- | ---: | ---: | --- |
| Small, mostly empty | 3.447 / 3.667 | 0.126 / 0.168 | Large benefit. |
| Screen-covering | 5.976 / 6.669 | 5.955 / 6.666 | Essentially no benefit. |
| Four overlapping | 18.330 / 19.158 | 11.967 / 12.681 | Benefit with remaining field work. |

The dense native 720p static workload with guard on had 9.451 ms median
presented interval and 9.813 ms median from 39 sparse GPU timestamps; the
same saved world/camera path with guard off had 39.441 ms and 40.730 ms,
respectively. These are separate runs on a changing desktop, not paired
per-ray output proof. No shader counter for exact field evaluations avoided
exists; cull counts are available only from the bounded diagnostic readback.

## Native near-capacity frame qualification

`first-light --qa2-profile SAVE MODE OUT.jsonl` loads and validates the exact
save, draws 64 warmup frames, then records 1000 actual native surface frames
and exits. Modes: `static` (unchanged world, Auto→Interpreter), `dynamic`
(native fixed-step `advance_life_cached`, existing source-move event every 30
ticks), `combined` (explicit Combined request, interpreter fallback until
compiled), and `unguarded` (static Auto path without visual guard). Camera
yaw follows a bounded deterministic sine of frame index. Input that would
mutate the world is ignored during profiling. Window size is checked per
frame; one unexpected 1080p resize was recovered in the valid dynamic run.
Earlier interrupted attempts were **excluded**. Fifo and Immediate were
requested only after checking surface-supported present modes.

The JSONL contains per-frame presented-start interval, call-to-present wall
time, tick, scene count, scene derivation, analytic/field binding, pipeline
selection, CPU buffer-write enqueue, BVH build, command encode/submit,
surface acquisition, present call, active compiler path and sparse GPU
timestamps. The field `frame_cpu_ms` includes waiting in acquisition and
occasional timestamp readback; it is **not** pure CPU work. GPU compute is
measured sparsely, so its p95/p99 has far fewer samples than intervals.
`upload_enqueue_ms` times `queue.write_buffer` calls, **not** completed GPU
transfer. Event handling, validation, primitive insertion, and GPU data
packing are not fully separable from the present APIs; their component costs
are therefore **NOT VERIFIED**. No uninstrumented-control overhead estimate
was made. The displayed window was real, but no visual image capture was
claimed as a numerical oracle.

All values below are milliseconds, p50/p95/p99/worst over 1000 measured
presented intervals after warmup. Counts are intervals over 16.7/33.3 ms.

| Workload / surface | Interval p50 / p95 / p99 / worst | >16.7 / >33.3 | Sparse GPU p50 (n) | BVH rebuilds in measured frames |
| --- | ---: | ---: | ---: | ---: |
| Static 720p Fifo | 9.451 / 15.527 / 37.996 / 41.331 | 42 / 28 | 9.813 (39) | 0 |
| Static 1080p Fifo, fixed size | 14.396 / 21.354 / 69.970 / 73.251 | 362 / 36 | 14.258 (44) | 0 |
| Dynamic 720p Fifo | 8.577 / 12.198 / 32.617 / 40.812 | 34 / 8 | 7.593 (39) | 18 |
| Dynamic 1080p Fifo, one size recovery | 17.455 / 23.505 / 71.154 / 77.540 | 590 / 37 | 17.125 (46) | 36 |
| Active Combined 720p Fifo | 6.063 / 6.689 / 7.436 / 30.835 | 1 / 0 | 0.928 (37) | 0 |
| Active Combined 1080p Fifo | 6.063 / 7.157 / 7.783 / 8.894 | 0 / 0 | 1.324 (36) | 0 |
| Active Combined 720p Immediate | 2.036 / 2.776 / 3.078 / 3.797 | 0 / 0 | 0.550 (34) | 0 |
| Static unguarded 720p Fifo | 39.441 / 44.261 / 162.668 / 169.288 | 883 / 883 | 40.730 (58) | 0 |

The static 1080p rerun used a fixed 1920×1080 window and recorded zero size
recoveries; its p99 tail still reached 69.970 ms. The dynamic 1080p valid run
recorded one size recovery and only 1920×1080 in its 1000 recorded frames.
Earlier 1080p runs aborted on external maximization/minimization and remain
in ignored logs for audit, not in the table. The 720p dynamic run used the
same fixed-size code but predates the per-record resolution field; its
window-size invariant and completion were checked at runtime.
Immediate's median surface acquisition was 0.017 ms versus 4.008 ms in
Combined/Fifo at 720p. The difference is presentation behavior under this
desktop compositor, not a claim that mathematical rendering is 3× faster.
Fifo waiting and GPU work are reported separately. 1440p and a second physical
adapter were not run; the two required resolutions were qualified.

### Preparation, invalidation, and compilation

Median `Scene::from_world` in the application plus the second derivation
inside `draw_mixed_with_execution` was about 0.700 + 0.742 ms at static 720p;
at fixed 1080p it was 0.716 + 0.757 ms. This is confirmed redundant work for
an unchanged world, but no cache patch or end-to-end A/B was attempted.
Static scenes built the BVH once in the first warmup frame (about 3.7–4.7 ms
on native runs) and **never** rebuilt it in measured frames, despite camera
motion. Dynamic source moves changed `source_x` between 0 and 2, with world
ticks 39→576 at 720p and 76→1147 at 1080p. Rebuilds rose 2→20 and 3→39;
18/36 measured frames actually built a BVH. Median build cost on those frames
was 3.727/3.832 ms; median CPU write enqueue on those frames was
0.059/0.064 ms. Other frames had zero BVH build cost. Geometry invalidation
therefore operated as expected in this fixture. Whether a narrower source
update could avoid a full rebuild is an unmeasured optimization hypothesis.

At static 720p, median application scene derivation 0.700 ms, renderer scene
derivation 0.742 ms, analytic binding 0.080 ms and field binding 0.116 ms were
small against 9.813 ms sparse Interpreter GPU compute. At 1080p static, the
corresponding medians were 0.716, 0.757, 0.083 and 0.113 ms; GPU p50 was
14.258 ms and acquisition p50 11.963 ms. The dynamic tick p50 was 0.503 ms
at 720p and 0.648 ms at 1080p; a source-move frame additionally paid the BVH
build. These stage clocks overlap or nest, so their medians must not be summed
as an exact frame decomposition.

The Combined request first displayed Interpreter with one compilation pending.
In the 720p Fifo cold-ish run it became active at frame 57 after a reported
629.336 ms worker pipeline/validation phase; in 1080p Fifo it became active
at frame 1 after 3.198 ms. This variation shows why one cold-ish launch does
not isolate driver cache behavior. Measured post-warmup frames stayed active
Combined with one request and no compiler failure. Dynamic Auto stayed
Interpreter with zero requests by existing policy for runtime-dependent
parameters. It was not silently promoted to compiled code. Compiler-worker
activity was not shown to cause a specific presented-frame tail.

An external Windows `Get-Process` sampler observed eight samples during an
additional valid 1080p static run: working set 200.1–249.5 MiB, maximum
observed private bytes 607.2 MiB. These are sampled process-memory values,
not a reliable peak or GPU memory footprint.

## Reliability and regression status

| Check | Local result |
| --- | --- |
| Windows fmt, strict Clippy, locked build, rustdoc | PASS |
| Windows ordinary workspace tests | PASS: 175 passed, 0 failed, 81 ignored |
| Windows separate opt-in workspace run | PASS: 81 passed, 0 failed |
| QA-2 release numerical/guard tests | PASS: 4 passed, 0 failed |
| Living World 1–4 CLI experiments, v10 replay | PASS; replay tick 240 digest `9103ee2194ffe042`, `equal=true` |
| Historical v7–v10 save fixture | PASS: 1 passed, 0 failed |
| World Lab 600-frame Auto and Combined profiles | PASS: 600 records each; Auto stayed Interpreter, explicit Combined became active; these are small showcase scenes, not the dense fixture |
| The Passage and compiler lifecycle | PASS within ordinary/opt-in workspace suites; no manual playtest |
| cargo-deny | PASS: advisories, bans, licenses, sources |
| Local site build/link check | PASS: 204 pages, 20,514 local links with this report |
| WSL fmt, strict Clippy, locked build, ordinary tests, rustdoc | PASS: 175 passed, 0 failed, 81 ignored |
| DX12 default FXC control | FAIL as expected: shader `X3511`, process 101; normal distribution remains blocked |
| DX12 with already installed Windows SDK DXC | PASS: targeted mixed diagnostic 1/1 on same NVIDIA card |
| Hosted CI, WSL native GPU, second vendor, driver packaging | NOT VERIFIED |

No driver, world limit, persistence format, release workflow, or mathematical
engine behavior was changed. The DXC control used an already installed SDK
DLL; it does not establish a shippable DX12 configuration. Rust ordinary and
opt-in tests were counted separately; ignored tests were not called passes in
the ordinary run. The known site sitemap warning (missing `site` option)
remains nonfatal.

## Risks, decisions, and next narrow work

| Priority | Finding and consequence | Decision / exit criterion |
| --- | --- | --- |
| P1 | f32 visual guard has no quantified error enclosure and can hide tracer exhaustion. Numerical `BoundsMiss` and uncertainty-preservation claims cannot use it. | Keep conditional visual use; derive a restricted safe GPU domain or make certified callers take a no-cull/fallback path. Prove the error budget and rerun adversarial readback on independent backends before stronger claims. |
| P1 | Dynamic runtime parameters keep Auto on Interpreter; near-capacity 1080p median 17.455 ms and 590/1000 frames exceeded 16.7 ms. | A bounded dynamic compilation experiment must verify revision, values, identity, fallbacks, and frame tails before changing the default. |
| P2 | Two `Scene::from_world` calls cost roughly 1.4–1.5 ms per unchanged frame in this profile. | Try one shared derived scene in a narrow A/B with save/load, camera, tick, transform, and definition-replacement invalidation tests. No revision cache yet. |
| P2 | Each authoritative source move rebuilt a 3841-node BVH, around 3.7–3.8 ms p50 on affected frames. | Profile which primitive bounds actually change; test safe refit only if frame-tail evidence justifies it. |
| P2 | DX12 FXC `X3511` on this Windows setup. | Document/test a distributable DXC path, then test another physical adapter. |
| P3 | Normal/entry oracle lacks general tangencies, near-coincident mixed depth, arbitrary capsule orientations, and GPU arithmetic bounds. | Keep cases ambiguous/unresolved until independent references exist. |

**Accepted corrections:** a real visual-path oracle; fixed-size bounded native
fixture/profiler; exact local-unit policy documentation. **Rejected/deferred:**
global guard disablement, new cache framework, BVH refit, changed default
compiler policy, world-limit increases, driver installation, and claims based
on offscreen frames alone. The next two highest-priority engineering tasks
are (1) a numerically explicit guard/fallback contract and (2) a narrowly
measured scene-derivation consolidation; dynamic compiler eligibility is a
parallel P1 research experiment, not an automatic policy change.

### Final questions and gate

| Question | Result | Evidence |
| --- | --- | --- |
| Q1. Certified nontrivial current GPU guard domain? | **PARTIAL** | Ideal graph AABBs are derived; complete f32 guard lacks an error bound. |
| Q2. False misses or hidden uncertainty? | **FAIL** for uncertainty preservation | 496 visual `Exhausted` outcomes suppressed; no definite false hit loss in finite qualified subset. |
| Q3. Guard disabled? | **PASS** | Small-field 3.447→0.126 ms with guard, dense native 39.441→9.451 ms median interval; disabling exposes tracer outcomes. |
| Q4. Actual dense full-frame cost? | **PASS** on this setup | 1000-frame native 720p/1080p profiles above, with real surface presentation. |
| Q5. Dominant CPU stage? | **PARTIAL** | Two scene derivations dominate isolated static CPU preparation; acquisition/GPU dominate full frame. Validation/insertion/packing not separately timed. |
| Q6. Unnecessary ordinary BVH builds? | **PASS** for tested world | Static: one warmup build, zero measured rebuilds; source changes triggered builds. |
| Q7. Compilation/fallback tails? | **PARTIAL** | Active path and frame 1/57 transitions observed; cold driver-cache control and causal tail attribution absent. |
| Q8. Intrinsic vs redundant costs? | **PARTIAL** | Pixel/field work and guard benefit measured; duplicate scene preparation observed without A/B correction. |
| Q9. Two evidence-backed corrections? | **PASS** | Explicit guard contract/fallback and one scene derivation per frame; dynamic compiler remains gated research. |
| Q10. Consolidation readiness? | **PARTIAL** | GO for targeted work tied to these findings; NO-GO for broad numerical/scalability claims or raised limits. |

### Reproduction

From the repository root in PowerShell, with Rust on `PATH`, use release
profiles and run GPU work sequentially. Large artifacts stay under ignored
`target/qa2/`.

```powershell
New-Item -ItemType Directory -Force target/qa2 | Out-Null
$env:GENESIS_QA2_FIXTURE_OUT = (Join-Path (Get-Location) 'target/qa2/dense-world.json')
cargo test --release -p first-light --test qa1_valid_world --locked dense_authored_world_reaches_combined_limits -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test qa2_guard --locked -- --ignored --nocapture --test-threads=1
$env:GENESIS_WINDOW_SIZE = '1280x720' # repeat with 1920x1080
$env:GENESIS_QA2_FRAMES = '1000'
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json static target/qa2/native-static.jsonl
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json dynamic target/qa2/native-dynamic.jsonl
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json combined target/qa2/native-combined.jsonl
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json unguarded target/qa2/native-unguarded.jsonl
$env:GENESIS_QA2_PRESENT = 'immediate' # only if the surface supports it
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json combined target/qa2/native-immediate.jsonl
Remove-Item Env:GENESIS_QA2_PRESENT
```

The workspace qualification commands were `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo build --workspace --locked`, `cargo test --workspace --locked`,
`cargo test --workspace --locked -- --ignored --test-threads=1`,
`cargo doc --workspace --no-deps --locked`, `cargo deny check`, and, from
`site/`, `npm run build` and `npm run check-links`. The exact machine-readable
frame samples, skipped-window attempts, DX12 logs, GPU tests, World Lab
profiles and full command outputs remain in ignored `target/qa2/`. They were
not committed because they are machine-specific and comparatively large.
