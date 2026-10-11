# Architecture Correction AC-4: GPU compiler reliability

Status: **qualified for the tested paths; research stage**. Starting revision:
`8c8858303bf97c2ff153be671425f0cdd9bff0dc` on `living-world`, clean.
Implementation revision: `1b1f2781cbe97053162c19980422d98bfaa4f12b`.
All work was local. There was no push, merge, rebase, tag, release or deployment.
Raw local logs and captures are in ignored `target/ac4-review/`.

## Architecture and code audit

**Source fact.** `GpuRenderer::compiled_pipeline` selects a key from the
currently enabled `WorldState` field graphs: exact source revision plus
semantics version, one structural topology plus semantics version, or a sorted
set of at most four topologies plus semantics version. `Auto` keeps Exact for
immutable worlds and uses Combined for runtime parameters. `Authoring` chooses
Structural for one topology and Combined for several. An unsupported or
invalid selection stays on Interpreter. A single worker owns cloned `Device`
and `PipelineLayout` handles and sends `Result<CompiledArtifact, String>` over
a bounded channel. `CompilerCache::poll_finished` inserts the completed result
under its original key. The current frame can activate a ready result only
after its freshly selected key matches. Four results, including failures, are
kept per renderer. Buffers, pipelines, worker receiver and cache are
renderer/device scoped. `WorldState` owns none of them.

```
World Lab external JSON -> LabScene::load -> WorldState
WorldState -> scene/field buffers -> GpuRenderer::compiled_pipeline
                                  -> key lookup -> ready pipeline or Interpreter
                                  -> one worker -> error scopes -> channel
                                  -> poll_finished -> four-entry renderer cache
```

**Confirmed defect.** World Lab's normal redraw used `Authoring`, while
`App::reload` used `draw_mixed`, whose default is `Auto`. The reload preflight
could therefore enqueue an Exact shader for an edited immutable definition,
occupying the one worker before the Authoring draw. The reload now calls
`draw_mixed_with_execution` with the same policy as redraw. Both use
`App::gpu_execution`. This changes no world data or shader mathematics.

**Tested existing decisions.** Exact still benefits stable immutable graphs.
No global policy, cache size, worker count, shader source emitter, graph
operator or persistence schema changed. The source emitter still rewrites
generated WGSL fragments for Combined; operator drift is an architectural
risk, with no reproduced defect in this campaign. The source-size and graph
limits remain bounded.

## Controlled lifecycle and real validation

**Executed test.** `obsolete_successful_completions_follow_current_world_key`
uses the production `compiled_pipeline` selection and real successfully
created GPU pipelines. A controlled channel delivers A after the requested
world has become B, then B after it has become C. Neither obsolete program
activates. C activates under its full key; returning to cached A succeeds.
Another 300 alternating cached selections preserve the three-entry bound and
do not queue new work. A test-only cached-error barrier prevents an unrelated
OS-scheduled worker from obscuring the ordering. This is deterministic
completion ordering, not a timing-dependent race probe. The existing
`exact_pipeline_churn_preserves_current_revision` test checks eviction and
recompilation after returning to an evicted key, including GPU identity,
distance and normal readback. The existing failure test checks four injected
errors plus a disconnected worker. These are distinct evidence.

**Executed real wgpu test.** `real_validation_error_falls_back_without_poisoning_next_world`
submits invalid test-only WGSL from a worker thread through the production
shader/pipeline error-scope function. wgpu 30.0.1 reported a nonempty actual
validation error. The error entered the keyed cache, retained the failure
reason, did not retry the same key, and the unchanged world produced a real
Interpreter GPU diagnostic hit. A different valid world then activated a real
compiled pipeline and returned the expected distance, with no new failure.
This establishes recovery for this class of shader validation error; it does
not establish recovery from device loss, out-of-memory, arbitrary backend
panics or every possible pipeline error.

**Executed lifetime test.** A pending old renderer was dropped; its receiver
closed, and a controlled late successful send could not reach a new renderer.
The new renderer on a newly requested `Device` had an empty cache, rejected
the old device, fell back while requesting its own shader, and retained the
unchanged `WorldState`. Existing opt-in readback additionally tests compiled
activation after recreation. Actual GPU device-loss recovery is not
implemented or claimed. A running worker can briefly retain its cloned
device after renderer drop; no stale result can enter another renderer.

## Backends and numerical contracts

**Executed on one NVIDIA GeForce RTX 4070 SUPER, driver 32.0.15.9186.** Native
Vulkan opt-in tests passed. The default DX12/FXC targeted control failed with
exit 101: `X3511: forced to unroll loop, but unrolling failed`, at lines 111
and 465 of translated analytical HLSL during renderer creation, before Field
Graph compilation. The same test passed through DX12 when explicitly pointed
to an already installed Windows SDK `dxcompiler.dll`. This is a conditional
test control, not a deployable DX12 solution. The translated HLSL and one
minimal equivalent source construct were not isolated, so no speculative
WGSL rewrite or silent Vulkan fallback was accepted. Default DX12 remains
**blocked** on this setup; another physical GPU and driver are untested.

**Executed regression.** The opt-in Vulkan suite covers analytical/BVH parity,
mixed depth and identity, material association, near tangent and inside
starts, structural/combined/exact paths, dynamic parameters, visual guard,
Living World oracles and the known bounded uncertainty cases. It does not
turn visual spatial culling into a certified mathematical Miss. The existing
`y = 1 + 1e-8` and hidden-Exhausted counterexamples remain documented; no
numeric tolerance changed. Current-run BMPs in the review package show the
edited structural object changing size. Interpreter and Specialized captures
were visually compared; their different file hashes alone are not an error
metric. Existing tests provide bounded pixel and diagnostic comparisons.

## World Lab and first use

**Executed native window measurement.** Two separate current release World Lab
processes ran the unmodified showcase for 32 frames at 1280×720, Vulkan,
Fifo, `Authoring`. Neither run cleared the driver cache, so the first is a
fresh process **with unknown pre-existing cache state**, not a cold driver
measurement. Both requested one pipeline, used Interpreter for three frames
and first selected Structural at frame 4. Reported source generation was
0.0663 / 0.0529 ms and worker pipeline creation 3.5221 / 3.8643 ms. The
31 measured frame intervals had p50 5.9463 / 6.0405 ms, p95 6.6466 / 6.3717
ms, p99 6.7568 / 6.4415 ms and maxima 6.8323 / 6.8957 ms. Neither sample
contained an interval over 16.7 ms. The first frame's event handling was
20.44 / 18.08 ms; startup and first frame are not included in interval
percentiles. These are two small observations, not a performance guarantee.

**Executed external-file test.** A copy of the actual World Lab showcase and
its graph JSON was edited on disk. `App::reload` changed the authoritative
revision, two compatible numerical edits reused Structural without another
request, a topology edit fell back and requested one new program, invalid
JSON preserved the last valid world, and restoring the original source
reused its cached program and produced the original BMP bytes. Whole
`App::reload` durations in this headless integration test were 0.6663,
0.5008, 0.3841, 0.3840 and 0.4929 ms for the two numerical edits, topology
edit, invalid JSON and restore, respectively. They include file reads,
parsing and world construction, but do **not** include the native window's
GPU preflight or presentation. Those subphases were not individually timed.
Human key-driven reload and rapid incompatible external edits in the native
window remain unverified. No percentage improvement is claimed against AC-3
because timing sources and edit schedules differ.

## Qualification

| Check | Observed result |
| --- | --- |
| Windows fmt, strict Clippy, locked build, ordinary tests, rustdoc | Passed; 177 ordinary tests, zero failures |
| Windows sequential opt-in workspace | Passed; 87 opt-in tests, zero failures, including three new native tests |
| Real external JSON/GPU readback | Passed; five reload states and BMP checks |
| Living World 1–4 release CLI experiments | Four exits 0; each output reported `replay_equal: true` |
| Authentic historical v7–v10 load/continue/resave | 1/1 passed |
| WSL Ubuntu 24.04 serial Clippy, build, ordinary tests, rustdoc | Passed; 177 ordinary tests. First build attempt had an incorrect test-command `PATH`, then succeeded with `/usr/bin` restored. |
| `cargo deny check advisories licenses` | Passed |
| Astro/Starlight site build and local links | Passed; 212 pages and 22,164 checked local links. Existing nonfatal sitemap warning: no configured public `site` URL. |
| Default DX12/FXC / installed SDK DXC control | Failed as known `X3511` / passed, on the same GPU |
| Hosted CI, second physical GPU, device loss, true cold driver cache | Not verified |

The source-only fix was accepted because the reload policy mismatch is
demonstrable from call sites and the corrected application path and external
GPU reload tests pass. Adding a worker scheduler, increasing cache capacity,
globally switching `Auto`, rewriting shaders for FXC, adding compiler IR and
changing numerical limits were rejected or deferred for lack of evidence.

## GO / NO-GO

| Question | Status |
| --- | --- |
| Obsolete successful compilation under tested keys | **PASS** |
| Real invalid-WGSL validation error, fallback and recovery | **PASS for this class** |
| Renderer recreation and device-scoped cache | **PASS for normal recreation**; device loss not verified |
| External compatible JSON edits | **PASS in headless integration**; live key-driven timing partial |
| First-use latency | **PARTIAL**; two existing-cache process samples |
| Bounded cache, failures and eviction | **PASS for tested scenarios** |
| Default distributable DX12 | **FAIL on tested FXC path** |
| Numerical identity, depth, material and uncertainty | **PASS for existing fixtures** |
| Limited external compiler SDK | **NO-GO**; device-loss, DX12 distribution and wider hardware remain open |

The next milestone may be a focused mathematical-geometric expressiveness
research program **only if it keeps the compiler research-stage and does not
depend on default DX12 or device-loss recovery**. Before an external SDK,
qualify a redistributable DX12 path, a second GPU tier, native-window
external edit latency, and explicit device-loss behavior. Existing operator
parity tests should accompany any future emitter consolidation.

## Reproduction

From the repository root, with Rust 1.94.0 and a native Vulkan adapter:

```text
cargo test -p analytic-renderer --lib --locked obsolete_successful_completions_follow_current_world_key -- --ignored
cargo test -p analytic-renderer --lib --locked real_validation_error_falls_back_without_poisoning_next_world -- --ignored
cargo test -p analytic-renderer --lib --locked renderer_recreation_does_not_inherit_old_completion -- --ignored
cargo test -p world-lab --locked external_graph_edit_changes_actual_gpu_image -- --ignored --nocapture
cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture
cargo test -p world-runtime --test historical_saves --locked
cargo deny check advisories licenses
```

For first-use samples, build `world-lab` in release mode, set
`GENESIS_WORLD_LAB_PROFILE_FRAMES=32`,
`GENESIS_WORLD_LAB_PROFILE_SCENARIO=authoring` and
`GENESIS_WORLD_LAB_PROFILE` to a writable JSONL path, then run
`target/release/world-lab.exe examples/world-lab/scenes/showcase.json`
twice as separate processes. The DX12 controls use
`GENESIS_TEST_GPU_BACKEND=dx12`; the conditional DXC control additionally
sets `GENESIS_TEST_DXC_PATH` to a locally installed compatible DLL. Do not
ship the Windows SDK DLL by copying this test configuration.
