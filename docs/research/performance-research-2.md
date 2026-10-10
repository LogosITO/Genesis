# Performance Research 2: revision-scoped mathematical shader compilation

**Status:** CONDITIONAL native integration. Implementation revision `67f4956` on
`efficiency-research`, based on `6fc22c6`. The source and measurements below
are local research, not a cross-device performance or numerical guarantee.

## Question and equivalence audit

Can a validated, immutable Field Graph be lowered to WGSL for native mixed
rendering while retaining the interpreter's visual contract during edits and
compiler failures?

Performance Research 1's 34.74 → 0.186 ms result used the same standalone
camera, ray interval, field, trace policy, normal samples, and GPU timestamp
method for both evaluators. It did **not** include the native analytic depth
pass, per-occurrence transforms, materials, or mixed composition. That result
motivated this work; it is not used as a native speedup claim.

The new A/B/C/D benchmark uses **one native mixed renderer** and the same
`WorldState`, camera, near/far interval, 128-step trace policy, finite-difference
normal, material/depth logic, and timestamp query. Only the evaluator and a
runtime spatial-guard bit differ:

| Mode | Field evaluator | Expanded visual bounds guard |
| --- | --- | --- |
| A | Interpreter | Off |
| B | Interpreter | On |
| C | Specialized WGSL | Off |
| D | Specialized WGSL | On |

The benchmark is offscreen; compute timestamps exclude CPU preparation,
presentation, and readback. Seven samples after three warmups produce p50/p95.
The 1/8/16/32-node rows are **complexity-scaling scenes**; A/B/C/D within one
row are matched algorithms on identical geometry.

## Implemented execution path

`WorldState` still owns source bytes, exact SHA-256 definition revisions, and
mutable instances. The renderer reads an already validated `Graph`. The new
emitter uses only its six typed operations: sphere, box, translation, positive
uniform scale, hard union, and bounded smooth union. It does not accept WGSL
from JSON or a user-provided string. It emits one deterministic function per
reachable node. Shared DAG nodes share code, **not** sampled values; calls
receive their own transformed point. Authoring already rejects unreachable
nodes, cycles, more than 32 nodes, more than 12 levels, and more than 64
expanded visits. The emitted field source is capped at 32 KiB. Naga tests and
wgpu validation check the generated module. Explicit WGSL module markers
replace the previous accidental function-name/source-position splits.

The native specialization is eligible when all enabled field occurrences
reference **one exact definition revision**. Multiple instances of that
revision keep independent transforms and materials. Scenes with multiple
revisions use the interpreter. The compiled shader replaces only `sample()`;
the same native `field_trace`, uncertainty classification, normal estimation,
analytic depth comparison, shading, and output code remain. The spatial guard
is a runtime uniform, so enabling it does not create another pipeline.

The cache is owned by one `GpuRenderer` and its exact `wgpu::Device`. Its key
is `(DefinitionRevision, compiler semantics version)`. The fixed native shader
configuration is owned by that renderer; transforms, camera, materials, light,
trace policy, and guard are runtime inputs. At most four successful or failed
revision entries are retained with LRU eviction, plus one in-flight request.
One worker thread creates pipelines through wgpu's synchronous API. The draw
call never waits for worker completion, but a driver could still serialize
GPU work across threads. A completed obsolete revision is cached under its
own key and cannot be selected for a newer revision. A failed compilation is
cached as failure and the interpreter continues to draw the full world.
Renderer recreation owns a new cache and device; passing another device to
the renderer is rejected. No new GPU feature is required beyond the existing
native mixed renderer.

World Lab now starts through the interpreter, reports `INTERPRETER/COMPILING`,
`COMPILED`, or `INTERPRETER/COMPILE ERROR` in its HUD, and selects the compiled
path after validation. Its external-scene GPU test passed an edit, an actual
revised image, and invalid-input rollback. The Passage and CPU world model do
not depend on this cache.

## Numerical and image qualification

The production emitter replaced the prior test-only generator. On each of
Vulkan and DirectX 12, five standalone graphs (sphere, box, nested transform
and union, 32-node shared-expression graph, smooth union) compared 65,536
deterministic rays per graph: **327,680 rays per backend**. Interpreter and
specialized status agreed on every ray. The 720p images had zero changed
background/object-occupancy pixels, and changed channels differed by at most
one RGBA8 level. Maximum Vulkan hit-distance difference was 0.000800 world
units, on the nested transform; maximum normal-component difference was
0.000592. The corresponding CPU `f64` tracer distance was 5.50475838; the
interpreter and specialized `f32` distances were 5.50395870 and 5.50475836.
Both sampled values at the earlier distance were within the configured 0.001
hit threshold. This is threshold-sensitive floating-point stepping, not a
status or occupancy disagreement. It remains a numerical limit, not proof of
equivalence for all rays. DirectX 12's largest distance difference was about
0.0000019 in this corpus.

The native mixed test compared real 640×360 readbacks, including a 32-node
graph, two instances, an analytic sphere, material changes, camera movement,
transform changes, A→B→C edits, cache reuse, and invalid-edit rollback.
Interpreter/direct and compiled/guarded images differed by at most one color
level. Two real captures are in
`target/performance-research-2-captures/native-{interpreter,specialized}.bmp`.
The local `target/performance-research-2-review/` package retains separate
Vulkan and DX12 pairs. They were visually inspected; the same forms are
present. The separate
direct-query/viewport test compared 83,205 pixel rays across isolated,
overlapping, offscreen, 0.1×, and 100× occurrences **after forcing the
compiled path ready**, plus the preserved `y = 1 + 1e-8` and other near-tangent
rays. A 0.01× occurrence was compared between native interpreter/direct and
compiled/guarded images. That last check is differential image evidence, not
an independent CPU or direct-query proof at its large local ray interval.

The expanded bounds are display acceleration only. Neither the compiler nor
the guard grants physical collision authority to Field Graphs. No floating
point enclosure or formally certified absence of false bounds misses has
been established.

## Native GPU compute measurements

RTX 4070 SUPER, driver 591.86, Windows, Vulkan, Rust 1.94.0, wgpu/Naga
30.0.1, release profile. The following are **p50/p95 milliseconds** from the
clean implementation HEAD `67f4956`; raw 68 JSON rows are in the ignored
`target/performance-research-2-matrix-final.txt`.

| Scene | Size | A | B | C | D |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 node | 720p | 3.980/4.529 | 1.116/1.457 | 0.046/0.047 | 0.038/0.038 |
| 1 node | 1080p | 9.039/9.767 | 2.011/2.044 | 0.088/0.088 | 0.066/0.067 |
| 1 node | 1440p | 14.885/15.774 | 3.319/4.053 | 0.151/0.152 | 0.113/0.114 |
| 8 nodes | 720p | 6.265/6.926 | 1.417/1.462 | 0.059/0.060 | 0.038/0.040 |
| 8 nodes | 1080p | 14.471/15.242 | 2.542/3.159 | 0.125/0.126 | 0.068/0.068 |
| 8 nodes | 1440p | 25.499/26.333 | 3.904/5.864 | 0.216/0.217 | 0.116/0.116 |
| 16 nodes | 720p | 11.131/12.222 | 3.429/3.704 | 0.090/0.091 | 0.053/0.055 |
| 16 nodes | 1080p | 23.549/25.904 | 6.068/7.908 | 0.190/0.191 | 0.093/0.094 |
| 16 nodes | 1440p | 43.031/43.865 | 12.042/13.322 | 0.329/0.330 | 0.158/0.158 |
| 32 nodes | 720p | 22.073/23.265 | 10.860/14.014 | 0.172/0.173 | 0.108/0.112 |
| 32 nodes | 1080p | 48.463/52.730 | 21.872/23.478 | 0.361/0.362 | 0.196/0.198 |
| 32 nodes | 1440p | 86.623/88.278 | 35.396/39.248 | 0.618/0.619 | 0.328/0.330 |
| Offscreen 32 | 1080p | 17.513/17.526 | 0.040/0.040 | 0.152/0.152 | 0.044/0.045 |
| Nearly filled 1 | 1080p | 13.939/14.036 | 13.919/13.980 | 0.121/0.121 | 0.148/0.149 |
| Overlap 8 | 1080p | 28.537/29.050 | 4.917/6.446 | 0.229/0.230 | 0.113/0.114 |
| Shared revision 8 | 1080p | 28.344/29.068 | 5.066/6.825 | 0.228/0.229 | 0.106/0.106 |
| Analytic + 32 | 1080p | 48.944/50.000 | 22.731/23.553 | 0.364/0.365 | 0.202/0.203 |

The spatial guard is slightly slower on the nearly filled specialized scene
(C 0.121 vs D 0.148 ms), while the guarded interpreter is slightly faster
than compiled/direct on the empty scene (B 0.040 vs C 0.152 ms). These are
reasons to retain both mechanisms and to measure scene coverage. The large
compiled gains are specific to these bounded static shaders, this adapter,
and this compiler/driver. They do not establish frame-rate targets for a
changing world, a lower-tier GPU, or another material workload.

In the clean matrix, source generation was about 0.02–0.06 ms and the
**warm driver-cache** pipeline creation recorded about 2.4–3.0 ms for the
representative unique graphs (4.1 ms in one earlier 16-node run). Camera,
transform, material, and resolution changes reused an existing pipeline.
The cache retained at most four entries in these runs. GPU memory consumed
inside driver pipeline objects is not exposed by wgpu and was not measured.

## Compilation and edit latency

An opt-in 720p benchmark applied 12 successive 32-node source revisions,
rendering one real interpreter frame after each edit. One first local run
recorded a **331.5 ms** final shader/pipeline creation on the worker;
subsequent identical-source runs recorded **2.8–3.0 ms**, consistent with a
driver cache but not a controlled cold-cache experiment. At clean HEAD
`67f4956`, four worker requests were made during 12 edits (one in flight at
a time), and the final revision became active after a 256 ms offscreen wall
interval that included earlier work and GPU waits. During edits, CPU draw and
submit p50/p95 were 0.214/0.252 ms; blocking offscreen frame wall p50/p95
were 15.08/17.46 ms, with GPU compute 14.72/17.11 ms. These are **not**
window presentation intervals or evidence that driver-level compilation can
never stall another machine. Raw runs are in
`target/performance-research-2-edits*.txt`.

On this Windows installation, DirectX 12's default FXC compiler failed
while compiling the unchanged native `analytic.wgsl` pipeline (`X3511`,
forced loop unroll). Thus ordinary native DX12 initialization could not
reach the Field Graph compiler. The installed Windows SDK DXC 1.8.2502.11
was selected **only in the test instance**; the native 640×360 revision and
image lifecycle then passed on the same GPU through DX12. Standalone field
specialization also passed the 327,680-ray corpus with DX12/FXC, but its
specialized pipeline creation varied roughly 130–371 ms. No DX12 native
A/B/C/D performance matrix or lower-power GPU test was run. The project does
not package a DXC DLL, so this is qualification evidence, not a claim that
the shipped World Lab DX12 path is fixed.

## Decision and remaining work

**CONDITIONAL:** keep automatic specialization for the current single-revision
native visual subset, with the interpreter active during compilation and as
the permanent fallback for multi-revision scenes or failures. The measured
static-scene gain justifies retaining this bounded path. The worker and cache
preserve source identity without changing `WorldState`, persistence, contact,
or the analytical renderer. This is still a research-stage execution mode.

Priority follow-up:

1. **Certify the visual bounds guard more broadly.** Derive a GPU rounding
   model for f64→f32 bounds, transforms, slabs, and trace thresholds; test a
   larger adversarial corpus, especially 0.01× local intervals, large
   coordinates, and uncertain outcomes. Acceptance requires no hidden
   supported hit/uncertainty on each tested backend. Conservative widening
   may cost the measured empty-space benefit.
2. **Control compilation during editing.** Measure presented frame intervals
   with rapid A→B→C World Lab reloads, cold/warm pipeline creation, cache
   memory, and compiler queue delay. If cold compilation or driver contention
   harms interaction, add a measured stability delay or explicit compile
   priority. Preserve the interpreter; never compile unbounded revisions.
3. **Qualify platform and graph breadth.** Fix or avoid the existing FXC
   analytic-shader failure without silently requiring an unshipped DXC DLL;
   run the native four-mode matrix on DX12/DXC and a lower-tier GPU. Test more
   multi-definition and changing-world workloads. No vendor assembly is
   justified by current evidence.

## Reproduction and qualification status

```text
cargo test --release -p analytic-renderer --test mixed_benchmark native_compiler_matched_matrix --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --test mixed_benchmark rapid_definition_edit_latency --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --lib specialized_wgsl_feasibility --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test mixed_gpu native_compiler_revision_lifecycle --locked -- --ignored --nocapture --test-threads=1
cargo test -p world-lab external_graph_edit_changes_actual_gpu_image --locked -- --ignored --nocapture --test-threads=1
```

For the opt-in DX12 test, set `GENESIS_TEST_GPU_BACKEND=dx12`. To use an
already installed compatible DXC for the native mixed test, set
`GENESIS_TEST_DXC_PATH` to its absolute `dxcompiler.dll` path. This test-only
setting does not change normal World Lab startup.

After the final shader change: `cargo fmt --all -- --check`, strict workspace
Clippy, locked workspace build, ordinary tests (141 passed, 0 failed),
serial opt-in tests (49 passed, 0 failed), rustdoc, and
`cargo deny check advisories licenses` passed locally. The opt-in set includes
native World Lab edit/reload, The Passage GPU states, and compiler benchmarks;
the ordinary suite includes persistence/replay and renderer surface tests.
The static site built under `/Genesis/` and checked 15,591 local links.
GitHub CI was not run; no remote operation or release was performed.

| Campaign stage | Status | Limit |
| --- | --- | --- |
| 1 Baseline integrity | Complete | Standalone result separated from native work. |
| 2–4 Compiler, generation, cache | Complete for bounded grammar | One revision per native pipeline; four-entry device cache. |
| 5 Non-blocking lifecycle | Partial | One worker; no physical presentation interval or driver-wide stall proof. |
| 6 Fallback | Complete for tested transitions | Actual shader compile-failure injection remains untested. |
| 7 World Lab integration | Partial | Real scene GPU reload tested offscreen; interactive controls not inspected. |
| 8–9 Numerical and bounds | Partial | Differential evidence, no certified error enclosure. |
| 10 Performance | Partial | Full matrix on one Vulkan GPU; DX12 native matrix and lower tier absent. |
| 11 Decision | Complete | Conditional acceptance above. |
| 12 Local regression | Complete | Hosted CI and interactive window unverified. |
| 13 Research record | Complete | Raw data and captures remain local and ignored. |
