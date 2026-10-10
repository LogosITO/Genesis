# Performance Research 1: field execution and GPU ray domains

**Status:** local research on branch `efficiency-research`, starting from
`eee79d23363a7fcfe44ba0bce07be5e58a59bf45`. Results are specific to the
scenes, hardware, driver, and build described here. Raw logs and real GPU
captures are in the ignored `target/performance-research-1-*` paths; they are
not part of source releases.

## Question and execution path

What dominates the cost of a complex visual Field Graph, and which bounded
changes improve it without altering the authoritative mathematical world?

| Frequency | Owner and work | Evidence or status |
| --- | --- | --- |
| On JSON load/edit | `world-authoring::field_graph::compile_json` parses, validates, computes expanded visit budget and outward `f64` bounds. `world-state` retains exact source revision and an `Arc<Graph>`. | Existing immutable graph; not on every pixel. |
| On world tick | `world-simulation::advance_life_cached` clones candidate `WorldState`, applies growth, validates, then commits atomically. | Measured clone below; atomicity retained. |
| Per native frame | `GpuRenderer::draw_mixed` derives an analytic `Scene`; `mixed_binding` validates the world and repacks bounded graph/instance arrays. | GPU buffers compare against resident copies and upload only when changed. CPU draw/submit in this matrix is usually below 0.3 ms. |
| Per pixel and enabled field occurrence | `mixed.wgsl::render_mixed` generates a ray, traces analytic primitives, then invokes `field_trace`. | The original path invoked every occurrence regardless of spatial separation. |
| Per field ray step | `field_graph.wgsl::sample` interprets graph nodes through a 16-frame stack and at most 64 expanded visits. | Direct GPU query sampled 65,536 rays; distribution below. |
| Per field hit | Six further `sample` calls estimate a display normal; material and depth composition follow. | Finite-difference normal remains display-only. |
| Once per renderer creation | Rust joins fixed WGSL source and creates compute pipelines. | String composition is not per frame; specialization is experimental below. |

The graph is a DAG of validated operations. An indexed child can be evaluated
at different transformed coordinates, so node ID alone is not a valid sample
cache key. `WorldState` remains authoritative. No mesh, voxel source, contact
authority, or GPU dependency was added to simulation.

## Environment and reproducibility

Windows 11 Pro build 26200; AMD64 Family 25 Model 97 CPU; NVIDIA GeForce RTX
4070 SUPER, driver 591.86; Vulkan; Rust 1.94.0; `wgpu`/Naga 30.0.1; Cargo
release profile. The offscreen mixed benchmark uses one camera at `(0,0,-6)`,
0.9 radian vertical FOV, 3 warmups and 7 serial samples per row. It prints
25 `MIXED_BENCH` JSON records with scene definition hashes, adapter, source
commit/dirty state when run in the repository, timing source, and p50/p95.
The reference came from a `git archive` of the starting HEAD, and the current
code was built in a separate Cargo target directory to prevent a shared-target
artifact collision. GPU time is a compute-pass timestamp; CPU draw/submit and
blocking timestamp read are different quantities. No VSync or window is
involved. The graph-size rows change geometry as well as complexity; they are
**scaling cases**, not matched-geometry algorithm comparisons.

```text
cargo test --release -p analytic-renderer --test mixed_benchmark native_mixed_compute_matrix --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --lib gpu_ray_workload_distribution --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p analytic-renderer --lib specialized_wgsl_feasibility --locked -- --ignored --nocapture --test-threads=1
cargo test -p analytic-renderer --test mixed_gpu bounded_mixed_viewport_matches_direct_field_queries --locked -- --ignored --nocapture --test-threads=1
```

The local `target/performance-research-1-reference-repeat.txt` and
`target/performance-research-1-accepted-matrix.txt` contain the exact serial
runs used below. Seven samples do not establish a stable long-tail p95 or
cross-machine performance.

## Baseline and ray-domain experiment

The renderer now sends outward `f64` graph bounds converted to widened local
`f32` bounds with each field instance. WGSL intersects the finite ray segment
with the expanded box **before** calling the existing graph tracer. The direct
GPU graph query is unchanged. The box is an acceleration hint for display,
not geometry or a new physical collision contract. Widening includes the
largest supported hit tolerance (0.1 local units) and further slack for
`f32` graph/ray arithmetic. Nonfinite slab results fall back to full tracing.
The hit tracer, uncertainty colors, step policy, and material path are intact.
The Rust layout test checks all four 16-byte WGSL instance fields, and GPU
integration tests exercise the resulting storage buffer on Vulkan.

GPU compute p50, milliseconds, identical scene and resolution:

| Field nodes | 720p reference → bounded | 1080p reference → bounded | 1440p reference → bounded |
| ---: | ---: | ---: | ---: |
| 1 | 4.70 → 1.17 | 10.07 → 2.65 | 16.28 → 4.67 |
| 8 | 8.59 → 1.95 | 16.52 → 3.76 | 28.85 → 5.83 |
| 16 | 13.21 → 5.15 | 29.02 → 8.54 | 50.16 → 13.77 |
| 32 | 28.01 → 14.15 | 56.98 → 30.93 | 99.91 → 53.54 |

Additional 1080p p50/p95, milliseconds:

| Scene | Reference | Bounded |
| --- | ---: | ---: |
| Mixed analytic + 8-node field | 16.32 / 16.56 | 3.11 / 3.76 |
| Mixed analytic + 32-node field | 62.98 / 63.87 | 31.89 / 32.68 |
| 32-node field moved offscreen | 19.73 / 20.27 | 0.04 / 0.04 |
| One radius-3 nearly full-screen field | 15.81 / 16.26 | 15.85 / 16.68 |
| Two overlapping 8-node instances | 33.68 / 34.45 | 6.55 / 7.33 |
| Two shared-definition 8-node instances | 33.75 / 34.12 | 6.42 / 7.29 |
| Analytic-only | 0.03 / 0.03 | 0.03 / 0.03 |

The full-screen negative control is essentially unchanged. The first, tighter
bound trial was faster, but the retained version uses wider arithmetic slack
and still shows a large gain. Individual runs varied, so no per-row speedup is
claimed beyond this adapter and matrix. No incremental BVH, CPU cache rewrite,
or simulation clone removal was justified by these observations.

## Workload and CPU evidence

The opt-in raw GPU query profiler reads existing shader status and step fields;
it does not add atomics to the normal render pass. On a 256×256 ray grid:

| Graph | Rays | Miss / hit / uncertain / exhausted | Total field samples | Sample p50 / p95 / max | Charged maximum node visits |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sphere, 1 node | 65,536 | 60,448 / 5,088 / 0 / 0 | 1,081,432 | 15 / 23 / 123 | 1,081,432 |
| `balanced-16`, 32 stored nodes, 47 expanded visits/sample | 65,536 | 53,084 / 12,452 / 0 / 0 | 1,314,156 | 18 / 31 / 117 | 61,765,332 |

Sample totals include six normal samples on hits. “Charged maximum visits”
multiplies each sample by the graph's validated worst-case visit count; it is
**not** a measured actual-node-visit count. The production mixed pass does
not yet expose exact bounds-rejection or per-node counters. Thus these figures
support a high repeated-work cost, not a precise attribution of GPU cycles.

Existing release CPU fixtures gave median `WorldState` clone time of 0.0016 ms
for the real 381-primitive scene, 0.0575 ms to derive its render snapshot,
and 0.4141 ms to build the render BVH (25 samples). One authored 8-instance,
208-segment fixture measured 0.0396 ms snapshot and 0.1184 ms BVH median.
These are separate scenes, not total frame costs. The tiny clone measurement
does not justify changing transactional simulation. Parse/validation phase
costs were already sampled by the existing authored-world fixture; no parser
cache change was made.

## Specialized WGSL feasibility

An opt-in test generates functions only from the validated `Graph` enum and
numeric parameters. It emits one function per reachable node, preserving
the current sphere, box, translate, scale, union, and smooth-union formulas.
The generated shader is **not** installed in the native mixed viewport or
given authority over world state. There is no general language parser and
no user-controlled WGSL text. The experiment uses the same 720p image and
camera for interpreter and specialized GPU pipelines, with 3 warmups and 7
timed runs. GPU compute p50:

| Graph | Interpreter | Specialized | Compared query rays | Different 720p pixels | Changed occupancy pixels |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sphere, 1 node | 4.37 ms | 0.038 ms | 16,641 | 135 | 0 |
| `balanced-16`, 32 nodes | 34.74 ms | 0.186 ms | 16,641 | 122 | 0 |
| Smooth bridge, 4 nodes | 6.72 ms | 0.049 ms | 16,641 | 472 | 0 |

All 49,923 query rays retained status, with maximum hit-distance difference
`4.77e-7` world units and maximum normal component difference below `0.0002`.
All differing image channels differed by at most one 8-bit level. The six
real GPU captures are in `target/performance-research-1-captures/` and were
visually inspected; the tested shapes remain present. Pipeline creation time
varied strongly with driver/cache state: an initial fixed interpreter build
took about 234 ms and a specialized build about 24 ms, while repeated builds
were roughly 1–18 ms. These are not reliable cold-start distributions.
Generated WGSL source construction itself was below 0.05 ms in these cases.

**Decision: CONDITIONAL.** Static lowering appears highly promising for these
bounded examples, but these are standalone graph shaders. A production compiler
must safely cache by exact graph revision and compiler semantics version,
invalidate on world edits, handle multiple definitions/instances and shader
compile failures, and avoid blocking the native frame loop. Wider numerical
and backend testing precedes integration. The direct interpreter stays as the
fallback and correctness reference.

## Correctness, limitations, and decisions

- **ACCEPT, qualified visual acceleration:** The bounded mixed viewport
  preserved 83,205 direct GPU pixel-ray classifications across isolated,
  overlapping, offscreen, 0.1×, and 100× transformed field scenes, plus three
  tangent/near-tangent rays. Uncertain/exhausted reference outcomes were
  required to remain visible. A preserved native mixed capture and the final
  accelerated capture were byte-identical. This is empirical `f32` display
  validation, not a proof of interval-correct occupancy or collision safety.
- **REJECT for now, CPU transaction rewrite:** The 381-primitive `WorldState`
  clone median was 0.0016 ms. Removing atomicity would target a cost not shown
  to matter here.
- **CONDITIONAL, mathematical compiler:** The test-only specialized WGSL was
  much faster on matched static graphs, but compilation/cache/invalidation and
  portability remain open. It is not yet a production engine feature.
- **Known gap:** Exact node-visit counts, GPU bounds-rejection counts, native
  presentation/frame intervals, Nsight counters, a 0.01× scaled occurrence
  under the direct query interval limit, large evolving multi-field scenes,
  and non-Vulkan adapter parity were not measured. A previous narrow CPU
  `f64`→GPU `f32` exclusion caused a false miss; see
  [native mixed performance](native-mixed-performance.md). This experiment's
  wider local-domain guard must remain visual-only until a formal GPU error
  model or broader differential suite can certify stronger claims.

## Qualification actually run

The final local source passed `cargo fmt --all -- --check`, strict workspace
Clippy, `cargo build --workspace --locked`, `cargo test --workspace --locked`
(139 passed, 0 failed; 46 opt-in ignored by that command), and
`cargo doc --workspace --no-deps --locked`. A separate serial
`cargo test --workspace --locked -- --ignored --test-threads=1` executed all
46 opt-in tests: 46 passed, 0 failed, including the 25-row mixed GPU timing
matrix, field-GPU parity, analytic-GPU parity, compiler/workload experiments,
World Lab external reload, and The Passage GPU states.
`cargo deny check advisories licenses` passed. `npm run build` generated the canonical research
page and `npm run check-links` checked 15,240 local links. The Starlight build
emitted pre-existing i18n/404/sitemap warnings but exited successfully. No
CI run, non-Vulkan GPU, physical display frame interval, or interactive World
Lab control inspection was performed. Nsight Compute is installed, while
Nsight Graphics and RenderDoc were not found; no external shader counter
capture was claimed.

## Stage status and next hypothesis

| Stage | Status | Basis |
| --- | --- | --- |
| 0 Architecture | Complete | Source/ownership/frequency map above. |
| 1 Baseline | Complete | 25 reference and 25 current machine-readable release GPU rows. |
| 2 GPU profiling | Partial | Exact ray/sample/status counts, worst-case visits; no actual per-node or mixed rejection counters. |
| 3 CPU/runtime | Partial | Real clone/snapshot/BVH and existing parse fixtures; no detailed `mixed_binding` phase split. |
| 4 Spatial experiment | Complete | Integrated guard, matched matrix and GPU parity. |
| 5 Compiler feasibility | Complete | Generated validated WGSL, query/image parity and timings. |
| 6 Integration | Complete for spatial guard | Compiler intentionally remains test-only. |
| 7 Scalability | Partial | 720p/1080p/1440p on one consumer GPU; no low-end adapter. |
| 8 Regression | Partial | Rust/site/security gates and opt-in tests passed locally; no second GPU backend or interactive native check. |
| 9 Architecture decision | Complete | Decisions and next hypothesis here. |

The next research hypothesis is to lower **validated immutable graph revisions**
to specialized WGSL, with asynchronous compilation, bounded cache entries,
and interpreter fallback. Acceptance requires equal hit/miss/uncertainty
classification over a larger deterministic ray corpus, controlled frame
timings in changing World Lab scenes, and safe revision invalidation. The
current spatial guard should first be checked on another backend and with
more adversarial numerical cases. No vendor-specific assembly is justified
by current evidence.
