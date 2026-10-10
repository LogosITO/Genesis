# Performance Research 3: dynamic mathematical execution

**Decision: CONDITIONAL.** Verified start `8665850` on `efficiency-research`;
implementation commits `48c1563` and `934d3dc`. The renderer now has an
**explicit experimental structural mode**. Automatic World Lab rendering still
uses the proven exact-revision compiler and interpreter fallback. No mesh or
voxel representation, collision authority, new persistence format, or remote
Git operation was introduced.

## 1. Baseline and question

The [clean starting-HEAD baseline](performance-research-3-baseline.md) ran the
Research 2 A/B/C/D matrix on `8665850`, not just its earlier implementation
commit. For the 32-node 1080p scene, GPU compute p50/p95 was 52.069/55.695 ms
(interpreter), 23.620/26.290 ms (interpreter with guard), 0.361/0.361 ms
(constant specialization), and 0.197/0.197 ms (constant specialization with
guard). All values are local Vulkan timestamp measurements.

The question here is whether one compiled mathematical **structure** can
render distinct authored revisions as their numeric parameters change,
without compiling for every revision. Exact source revisions must remain
world-owned and auditable.

## 2. Execution architecture

The new `GpuFieldExecution::Structural` mode derives a bounded key from the
validated graph's root index, ordered operation codes and child links, plus
compiler semantics version. It ignores numeric parameter values **only for
program compatibility**. `WorldState` still stores exact source bytes and
SHA-256 revisions; each graph remains subject to authoring validation.
Parameters must additionally survive finite `f32` conversion and retain
positivity where an operation requires it. Authored boxes already require
half extents of at least `0.01`. The generated WGSL contains deterministic
functions for reachable nodes and reads the current parameter vector from the existing
field-node storage buffer, using the current instance's root offset. Distinct
revisions with identical node links can share one pipeline and have independent
transforms and materials. A topology change cannot use that pipeline.

The existing one-worker, one-in-flight, four-entry device-local cache holds
both exact-revision and structural keys. A result for an obsolete incompatible
key cannot be selected. Shader source remains generated only from validated
operations and capped at 32 KiB. Current graph/world limits are 32 nodes per
definition, four definitions, and eight instances. Unsupported or mixed
topologies use the interpreter for **all** instances, preserving depth,
identity, and uncertainty logic in the existing mixed pass. Structural mode is
opt-in through the renderer API and profiling harness; it is not the automatic
production choice.

The code also exposes CPU timings for world-to-scene conversion, analytical
and field binding, pipeline selection, and command encoding/submission, plus
CPU time spent queuing the last field-node buffer write. These timings do not
measure actual GPU transfer or driver pipeline memory.

## 3. Matched GPU compute results

Clean code HEAD `934d3dc`, NVIDIA GeForce RTX 4070 SUPER, Windows, Vulkan,
driver 591.86, Rust 1.94.0, wgpu 30.0.1, release. Three warmups and seven
samples per mode, with identical world, camera, 128-step trace policy,
materials, normal/depth work, and GPU timestamp query. The following 1080p
values are **p50/p95 ms**; D embeds constants and E reads runtime parameters.
Both use the same visual bounds guard.

| Nodes | D constant | E structural | E pipeline source bytes |
| ---: | ---: | ---: | ---: |
| 1 | 0.073/0.073 | 0.074/0.074 | 18,276 |
| 8 | 0.068/0.068 | 0.082/0.082 | 19,313 |
| 16 | 0.093/0.094 | 0.141/0.146 | 20,539 |
| 32 | 0.196/0.197 | 0.383/0.384 | 22,987 |

The full 85-row A/B/C/D/E matrix also covers 720p, 1080p, and 1440p, plus
empty, occupied, overlap, shared-instance, and analytical/implicit scenes.
It is retained as machine-readable local data. Structural execution is slower
for these static single-revision graphs. Its reason to exist is pipeline reuse
under edits and across compatible revisions.

The first local structural shader/pipeline creation observed for the 1/8/16/32
graph cases was roughly **315–440 ms**. An unchanged second process run showed
roughly **2.4–2.9 ms** for representative 1080p cases. This is consistent with
a warm driver cache; the experiment does not control or guarantee cold-cache
conditions. Four cache entries are bounded in Rust, while driver pipeline
object memory is not exposed by wgpu and was not measured. No compilation
debounce or speculative queue was added: window measurements below did not
justify a policy yet.

## 4. Parameter edits and multiple definitions

At 720p with a 32-node graph, two series of 12 **world-authoritative** radius
revisions used the same structural pipeline. Each revision changed the source
hash and queued a new 1,024-byte node buffer; compiler requests after initial
readiness remained **zero**.

| Delay between edits | World replacement p50/p95 | Node enqueue p50/p95 | CPU draw/submit p50/p95 | GPU compute p50/p95 |
| ---: | ---: | ---: | ---: | ---: |
| 0 ms | 0.028/0.036 ms | 0.0017/0.0021 ms | 0.102/0.118 ms | 0.227/0.232 ms |
| 30 ms | 0.046/0.055 ms | 0.0029/0.0033 ms | 0.211/0.443 ms | 0.250/0.263 ms |

The node write figure is CPU enqueue time, not GPU upload completion. Blocking
offscreen frame wall time includes timestamp readback and is not presentation
latency. The small differences between series may include system scheduling.

The bounded multi-definition matrix used **two instances per definition** at
720p/1080p/1440p. Current `WorldState` allows four definitions and eight
instances; an eight-definition authoritative scene is unsupported and was not
fabricated. Selected 1080p GPU compute p50/p95:

| Topology | Definitions | Auto path | Auto ms | Structural path | Structural ms |
| --- | ---: | --- | ---: | --- | ---: |
| Shared | 1 | Specialized | 0.116/0.117 | Structural | 0.139/0.139 |
| Shared | 2 | Interpreter | 8.129/9.613 | Structural | 0.237/0.238 |
| Shared | 4 | Interpreter | 18.890/23.147 | Structural | 0.453/0.453 |
| Heterogeneous | 2 | Interpreter | 4.620/5.595 | Interpreter | 4.517/5.296 |
| Heterogeneous | 4 | Interpreter | 19.300/21.903 | Interpreter | 17.756/21.993 |

Only A/B/C/D/E **within the same scene** are matched. The shared and
heterogeneous fixtures have different mathematical operations; their raw
times should not be compared as a speedup. The structural approach is useful
for multiple revisions of one law, but does not solve heterogeneous topology.

A combined bounded shader could dispatch among several validated topologies
in one pass, but must bound source/pipeline growth and preserve nearest-hit,
depth, identity and uncertainty order. Definition-grouped passes would need
intermediate depth/diagnostic buffers and a correct merge, with extra dispatch
and upload cost. Neither strategy was implemented or timed; production
heterogeneous specialization remains **INCONCLUSIVE**. The current complete
interpreter fallback remains the accepted behavior.

## 5. Real World Lab frame intervals

An opt-in internal scenario ran **600 actual window frames** each in Auto and
Structural mode on Vulkan. Both used the same showcase world and deterministic
100-frame phases: static, moving camera, material changes, instance transform
changes, rapid source revisions, slower revisions, and invalid source edit
rejection. The source edits change actual `WorldState` revisions; the scenario
is a profiling fixture, not a new persistent simulation rule. The window used
`Fifo` present mode. Reported intervals exclude the first 30 frames; 570
samples remain for each run.

| Mode | Interval p50 | p95 | p99 | Worst | Frames >16.7 ms | Compiler requests |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Auto | 6.058 ms | 6.294 ms | 6.554 ms | 7.052 ms | 0 | 6 |
| Structural | 6.059 ms | 6.295 ms | 6.499 ms | 7.003 ms | 0 | 1 |

Auto spent 113 frames in interpreter/compiling and 487 in compiled mode.
Structural spent three frames compiling and 597 using the structural pipeline
in this **warm-cache** final run. A preceding first structural window run took
57 frames before readiness, still without a >16.7 ms interval. That run had a
HUD classification bug (the path displayed as interpreter after activation);
the HUD and final profile now explicitly report `STRUCTURAL`. Cold driver
behavior across fresh machines remains unqualified.

In the final structural run, p50/p95 CPU stage times were: world snapshot
0.0027/0.0051 ms, analytical binding 0.0279/0.0499 ms, field binding
0.0095/0.0161 ms, pipeline selection 0.0010/0.0015 ms, command encoding and
submit 0.1261/0.2002 ms, surface acquisition 5.6875/5.8076 ms, and present
call 0.0444/0.0639 ms. The frame interval is dominated here by `Fifo` surface
acquisition, so similar interval distributions **do not** prove equal GPU
throughput. Only two periodic GPU timestamp samples occurred per 600-frame
run; the offscreen matrix supplies GPU distributions. Event handling recorded
zero input time because this automated scenario mutated the scene internally;
manual mouse/keyboard interaction was not profiled.

## 6. Correctness, changing world, and backend results

- GPU readback of two, then four distinct authored revisions sharing one
  structure matched interpreter RGBA8 output within one channel level. A
  radius edit changed the actual image with no new compile request. An
  incompatible box revision forced a byte-identical interpreter fallback.
  Two Vulkan and two DX12 before/after BMPs are in the ignored review folder;
  they depict the sphere radius change and were visually inspected.
- The expanded direct-query/guarded-viewport test compared **327,680 pixel
  rays** in five supported Vulkan cases, including overlap, offscreen objects,
  0.1× and 100× scales. No supported hit or uncertainty was hidden in this
  corpus. The 0.01× native differential image and preserved near-tangent
  `y = 1 + 1e-8` fixture also passed. This is empirical, not a floating-point
  enclosure proof; structural mode itself has image parity but no equivalent
  327,680-ray independent corpus.
- A 40-step offscreen fixture modified actual `WorldState` transforms and
  exact source revisions for two definitions/four instances, comparing two
  copies after each deterministic update. It spent 35 frames on the structural
  pipeline (GPU compute p50/p95 0.083/0.091 ms) and five on the interpreter
  while one graph became a box (2.699/2.820 ms). The sphere structure returned
  to the cached pipeline on step 25; compiler requests stayed at one. This
  fixture does not add ecology rules or new persistence semantics.
- Default World Lab launched and profiled successfully on Vulkan. Forcing
  native DX12 with system FXC reproduced `X3511` forced-loop-unroll failure
  while compiling the **existing analytical shader**, before the Field Graph
  compiler. With already installed Windows SDK DXC 1.8.2502.11, both native
  exact-revision lifecycle and structural four-definition/fallback tests
  passed on DX12 on the **same** RTX 4070 SUPER. Genesis does not ship DXC,
  and no DX12 performance matrix was run. Another physical GPU was not tested.

## 7. Decisions and remaining risks

| Experiment | Decision | Reason |
| --- | --- | --- |
| Structural shader with runtime node parameters | **CONDITIONAL** | Real world revisions reuse one pipeline; static 16/32-node graphs are slower and first compilation can be expensive. |
| Shared-topology multi-revision rendering | **CONDITIONAL** | Two/four definitions preserve images and gain substantially over interpreter in tested scenes; explicit mode only. |
| Heterogeneous-topology specialization | **INCONCLUSIVE** | Correct interpreter fallback is retained; combined and grouped strategies need a bounded implementation and parity tests. |
| Automatic selection/debounce | **DEFERRED** | No long window frames on one warm Vulkan setup; scene coverage, cold start, and another GPU are missing. |
| Visual spatial guard as physical authority | **REJECTED** | No certified floating-point enclosure; it remains display-only. |
| DX12 via system FXC | **BLOCKED** | Existing analytical shader fails before new compiler; SDK DXC is not a shippable dependency here. |

| Campaign stage | Status | Evidence or limit |
| --- | --- | --- |
| 1 Baseline | PASS | Clean `8665850`, 68 matched A/B/C/D rows. |
| 2 Window profiling | PARTIAL | Two real 600-frame runs and CPU stages; no manual input timing or controlled cold-cache run. |
| 3 Dynamic parameters | CONDITIONAL | Validated structural shader, actual parameter edits and reuse; slower static 16/32-node work. |
| 4 Multiple definitions | PARTIAL | One/two/four definitions measured; eight exceed world limit, heterogeneous topology interprets. |
| 5 Cache lifecycle | PARTIAL | Bounded single worker, no stale activation in tested edits; driver contention and memory not fully qualified. |
| 6 Spatial guard | PARTIAL | 327,680-ray Vulkan differential corpus, no rounding proof or equal structural corpus. |
| 7 DX12 | BLOCKED for FXC | Default Vulkan works; FXC `X3511`; installed DXC test controls pass. |
| 8 Evolving world | PASS for fixture | Forty world-owned update steps and replay equality; no new simulation rule. |
| 9 Decisions | PASS | Conditional/deferred outcomes above. |
| 10 Regression | PASS locally | Standard/opt-in gates pass; hosted CI and other GPU not verified. |
| 11 Delivery | PASS locally | Canonical documents and ignored JSONL/BMP review package. |

The next research program should first prototype a bounded heterogeneous
single-pass or hybrid evaluator with exact depth/identity/uncertainty parity,
then measure its compilation and dynamic-world costs. Separately, derive and
test a conservative GPU bounds error model; isolate cold driver cache behavior
and real input frame pacing on a lower-tier physical GPU; resolve DX12 startup
without depending on an unshipped proprietary DLL. Do not auto-select the
structural path solely from these measurements.

## 8. Verification and reproduction

At implementation HEAD `934d3dc`, `cargo fmt --all -- --check`, strict locked
workspace Clippy, locked workspace build, ordinary workspace tests
**143 passed / 0 failed**, serial opt-in tests **53 passed / 0 failed**,
rustdoc, and `cargo deny check advisories licenses` passed. The expected
DX12/FXC negative test failed with `X3511`; DX12/DXC controls passed. The
Astro/Starlight site built **182 pages** under `/Genesis/` and checked **16,304
local links**. The build still reports existing nonfatal empty-i18n,
duplicate-404, and unset-sitemap-site warnings. Hosted GitHub CI and a
different physical GPU were **not verified**.

Representative commands from the repository root:

```text
cargo test --release --target-dir target/performance-research-3-build -p analytic-renderer --test mixed_benchmark native_compiler_matched_matrix --locked -- --ignored --nocapture --test-threads=1
cargo test --release --target-dir target/performance-research-3-build -p analytic-renderer --test mixed_benchmark multi_definition_matrix --locked -- --ignored --nocapture --test-threads=1
cargo test --release --target-dir target/performance-research-3-build -p analytic-renderer --test mixed_benchmark structural_parameter_edit_latency --locked -- --ignored --nocapture --test-threads=1
cargo test --release --target-dir target/performance-research-3-build -p analytic-renderer --test mixed_benchmark evolving_visual_world_stress --locked -- --ignored --nocapture --test-threads=1
cargo test --workspace --locked -- --ignored --test-threads=1
```

For an actual window profile, set `GENESIS_WORLD_LAB_PROFILE` to a writable
JSONL file, `GENESIS_WORLD_LAB_PROFILE_FRAMES=600`, and
`GENESIS_WORLD_LAB_PROFILE_SCENARIO=auto` or `structural`, then run
`cargo run --release -p world-lab -- examples/world-lab/scenes/showcase.json`.
The scene must match the checked-in showcase for this deterministic fixture.
The profile closes the window at its frame limit. Do not combine `Fifo` frame
intervals with offscreen GPU compute timestamps as though they were one timer.

For opt-in DX12 test qualification only, set `GENESIS_TEST_GPU_BACKEND=dx12`
and `GENESIS_TEST_DXC_PATH` to a compatible installed `dxcompiler.dll` before
running `structural_parameters_reuse_pipeline_for_two_revisions` or
`native_compiler_revision_lifecycle`. These test variables do not fix the
normal World Lab DX12 startup path.
