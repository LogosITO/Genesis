# Geometry Research GR-1 — representation and complex geometry

**Status:** conditional research result, 11 October 2026. **Starting revision:**
`c7b76b934c078145a4122272116d7bec278442e7` on `living-world`.
**Final revision:** the local commit containing this report (`git rev-parse HEAD` after
the GR-1 commits). No remote operation is part of this result. Raw local evidence,
including real GPU captures, is in ignored `target/gr1-review/`.

## Question and scope

Can compact mathematical definitions describe, update and evaluate complex
Genesis geometry without unacceptable expansion or numerical ambiguity? This
experiment compares the current flat Field Graph and materialized grammar path
with a bounded CPU spatial hierarchy over shared graph instances. The hierarchy
is a **research-only occupied-point index**, not a production field evaluator,
GPU traversal path, distance estimator or collision authority.

### Verified representation map

| Source | CPU path | GPU path | Mathematical and spatial contract | Current limit |
| --- | --- | --- | --- | --- |
| Analytic sphere, box, capsule | `analytic-field`, `Scene`, analytic ray intersections and BVH | Direct or flattened GPU BVH | Analytic geometry; BVH only accelerates it | World and renderer budgets |
| Field Graph JSON | `world-authoring::field_graph::compile_json` validates a DAG and `Graph::sample` recursively visits it | Interpreter or generated WGSL through Exact, Structural, Combined, Auto or Authoring selection | A primitive alone may be exact SDF; union/smooth composition is a signed unit-Lipschitz field in ideal arithmetic. Bounds do not certify all floating-point rounding. | Source 16 KiB; 32 nodes; depth 12; 64 expanded visits; generated WGSL 32 KiB |
| Authored grammar JSON | Bounded rewriting materializes `Segment` capsules and structural identities | Existing analytic primitive renderer and BVH | The capsules, not a lazy grammar evaluator, are queried and rendered | 1,024 segments globally; fixture budgets can be lower |
| `WorldState` Field Graph instances | Definition revision and per-instance transform, material and parameters | One compiled program can serve multiple instances | World remains authoritative; GPU buffers and pipelines are derived | Four definitions, eight instances; compiler cache four pipelines |

Data flow: JSON source → validated immutable definition → authoritative
`WorldState` instance state → CPU graph/segment snapshot → renderer preparation
→ interpreter or specialized WGSL and instance buffers → GPU output. The graph
keeps DAG references but recursive evaluation revisits a shared child for each
path. WGSL specialization emits functions for graph nodes, subject to the shader
budget. Grammar derivation loses the ability to skip an unexpanded branch once
it has produced all segments; structural segment identity remains available.
World-owned instances already share a graph definition, while this experiment's
128-instance CPU scale is deliberately outside the eight-instance world budget.

These numeric caps are implementation/research budgets, not proofs of inherent
mathematical limits. GPU adapter limits and useful performance thresholds must
be measured separately. There is no evidence here for raising production caps.

## Hypotheses and discriminating tests

| Hypothesis | Supporting evidence sought | Contrary evidence / result |
| --- | --- | --- |
| H1: flat graphs suit small local fields but repeated work grows | Graph visits, shader size, GPU time across complexity | The 16-node organic graph renders; larger valid graph sizes and matched complexity scaling were not established. **Partial.** |
| H2: hierarchy skips separated substructures | Same graph and points, flat versus bounded index | Strong CPU point-query gains for separated instances; fully overlapping bounds make the index slower. **Supported only for this query/distribution.** |
| H3: bounded grammar can be queried lazily | Same geometry without full segment expansion | No lazy evaluator was implemented; the branching fixture materializes 80 capsules. **Unverified.** |
| H4: repeated definitions share work and independent state | One definition, eight instances, compiled program reuse after one transform edit | World Lab compiled one Structural program; changed one instance scale without a new request and produced a different captured image. **Supported at current world scale.** |
| H5: current operators express organic smooth forms naturally | Seamless continuous specimen with bounded graph | Four smooth pairs joined by hard union retain visible seams; nested smooth unions are rejected under the present exact-child contract. **Contrary evidence.** |
| H6: richer composition complicates numerical safety | Independent boundary cases and explicit uncertainty | Analytic arch points and index parity passed; GPU field ray/normal/uncertainty parity for the new specimens was not established. **Partial.** |

## Three deterministic specimens

| Specimen | Source and mathematical structure | Construction / visible result |
| --- | --- | --- |
| A: branching organism | `examples/authoring/gr1-branching.json`; one rule `X → F[+X]F[-X][&X]`, depth four, analytic capsule derivation | 437 source bytes, 521 expanded symbols, 80 segments, maximum stack four, work 759. One release CLI invocation took 0.307 ms including load; this is not a timing distribution. Real 640×480 GPU capture `gr1-branching.bmp`; Direct/BVH image bytes matched. |
| B: organic loop | `examples/field-graph/definitions/gr1-organic-loop.json`; eight translated spheres in four polynomial smooth-union pairs and three hard unions | 16 nodes, 23 expanded visits, SHA-256 `41bf3793a87e7823916dddabd43d93c709c7f7b5f997c724234c1fa9a6ede5bc`. Real Vulkan GPU capture `organic-loop.png` shows four smooth lobes with visible hard seams. The resulting field is signed unit-Lipschitz under the documented ideal contract, not an exact SDF. Solid query returned `Indeterminate(UnsupportedOperation)` for the tested center ray. |
| C: repeated mathematical arcade | `gr1-arch.json` plus `gr1-arcade-scene.json`; a 7-node box/translate/union motif occurs eight times | One definition, eight independent WorldState instances. Real 320×180 captures before and after one instance scale changes from 1 to 1.25. The compiled Structural path became active after one request; no new compilation request followed that edit. |

The specimen A derivation is an expanding grammar: at depths 1, 2, 3, 4 it
produced respectively 2, 8, 26, 80 capsules and 14, 53, 170, 521 symbols.
The isolated release CLI timings in `branching-depth.jsonl` are samples, not
robust medians. GPU render time and BVH build time for this exact fixture were
not separately timed; its GPU output and Direct/BVH agreement were tested.

## Alternative prototype and numerical contract

`crates/authoring/tests/gr1_hierarchy.rs` shares each validated `Graph` through
`Arc`, holds an independent translation per occurrence, and builds a deterministic
bounded binary index (at most 128 occurrences). Splits sort bounds centers with
an identity tie-breaker. A point is passed to a leaf only when it is within the
translated graph bounds, enlarged by one `f64` ULP per coordinate. Leaves use
the existing `Graph::sample(point - translation) <= 0` predicate. The result is
an **occupied-set Boolean for finite tested points**, with zero classified as
occupied. It does not return a signed field value, Euclidean distance, normal,
surface hit, or formal `Miss` for physics.

The graph's ideal occupied-set bounds come from the existing validator. One-ULP
rounding at the instance translation is a pragmatic local margin, not a formal
proof that all multi-operation floating-point error is enclosed. GPU `f32`
rounding is outside this prototype. Nonfinite source and points are rejected
by existing `Vec3` and graph validation. A production index would need a proved
or conservatively qualified exclusion contract, a failure/uncertainty path, and
fresh bounds after every relevant parameter or transform edit. No stale-index
use is permitted by the research test: it explicitly rebuilds after moving an
arch and checks old and new points.

The broad 4,096-point grids compare the index to a full flat scan for both
graphs, at 1, 8, 32 and 128 separated instances and 32 fully overlapping
instances. Both paths share `Graph::sample`, so this is **indexing parity**, not
independent leaf mathematics. Additional analytic arch inside/boundary/outside
points and moved-instance points passed. Existing workspace tests retain the
`y = 1 + 1e-8` and hidden-Exhausted guard regressions; GR-1 does not recertify
floating bounds, tangencies, internal zero surfaces or GPU ray identity for its
new hierarchy. Gradient and normal queries for the hierarchy are unsupported.

## Matched CPU point-query measurements

Windows x86-64, release profile, 4,096 fixed points × seven repetitions.
Median milliseconds for the complete point batch; full JSONL and construction
times are in `target/gr1-review/hierarchy-benchmark.jsonl`. These are CPU
occupied-point queries, **not** ray tracing or GPU rendering measurements.

| Graph | Instances | Distribution | Flat | Hierarchy | Leaf samples over seven repetitions |
| --- | ---: | --- | ---: | ---: | ---: |
| Arch | 8 | Separated | 2.80 | 0.124 | 504 versus flat 228,480 |
| Arch | 32 | Separated | 11.37 | 0.184 | 2,268 versus flat 901,264 |
| Arch | 128 | Separated | 43.53 | 0.703 | 9,072 versus flat 3,384,640 |
| Organic | 8 | Separated | 6.00 | 0.192 | 2,310 versus flat 224,308 |
| Organic | 32 | Separated | 22.66 | 0.433 | 8,862 versus flat 829,619 |
| Organic | 128 | Separated | 62.56 | 1.492 | 35,189 versus flat 2,254,364 |
| Arch | 32 | Fully overlapping | 9.31 | 10.43 | 917,504 on each path |
| Organic | 32 | Fully overlapping | 23.96 | 26.17 | 917,504 on each path |

For 128 occurrences the index contains 255 nodes and `size_of::<Node>() ×
255 = 18,360` bytes, plus 10,240 bytes of instance records; allocator and
shared graph allocations are excluded. Build time in the JSONL includes graph
parse and creation of all occurrence records, so it must **not** be interpreted
as an isolated BVH build. On full overlap the extra traversal produces no
pruning benefit; small or densely overlapping scenes should retain a flat path.
The measured scaling is workload-dependent. This experiment does not infer an
asymptotic speedup from the finite samples.

## GPU execution and dynamic update

The arcade benchmark runs **the existing World Lab GPU instance loop** in
Authoring and Structural modes. The CPU hierarchy above is not uploaded or
traversed by WGSL. NVIDIA GeForce RTX 4070 SUPER, Vulkan driver 591.86,
Windows, release profile, timestamp queries, 3 warmups plus 11 timed samples
per case. Same eight-instance world, camera and spatial guard; only execution
mode/resolution changes. Median and p95 are GPU compute milliseconds.

| Resolution | Authoring median / p95 | Structural median / p95 |
| --- | ---: | ---: |
| 320×180 | 0.04384 / 0.04413 | 0.04390 / 0.04525 |
| 1280×720 | 0.16502 / 0.16602 | 0.16512 / 0.16794 |

There is no meaningful compiled-path advantage in this spatially sparse scene.
Compiler statistics for the selected Structural program: 20,668 WGSL bytes,
0.0693 ms source generation and 31.1105 ms pipeline creation on its worker;
one request, zero further requests after the instance edit. Those compilation
figures are one observation, not a distribution or cold-start guarantee. The
captured BMPs differ after the edit. GPU buffer upload cost, presentation time,
GPU hierarchy cost, source size across larger motifs, topology-edit delay and
other backends remain **unknown**. The organic preview reported a single
12.7765 ms full-frame execution; it included its preview/readback workload and
is not comparable to the arcade GPU timestamp table.

## Related work and novelty

| System or method | Relevant idea | Comparison status / reuse constraint |
| --- | --- | --- |
| libfive ([project](https://libfive.com/)) | Function-representation geometry and compiled evaluation; useful reference for expression sharing and region-dependent work | Qualitative architecture comparison only; no matched run. Review its exact license and component terms before code reuse. |
| Fidget ([source](https://github.com/mkeeter/fidget)) | Fast implicit-function evaluation and region/interval techniques; relevant to local pruning and repeated expressions | No matched run; backend, authoring and numerical contracts differ. License review required before reuse. |
| Keeter, [Massively Parallel Rendering of Closed-form Implicit Surfaces](https://www.mattkeeter.com/research/mpr/) | Compiled implicit programs and GPU parallel evaluation | Literature comparison, no measured Genesis superiority or novelty claim. |
| Snyder, *Interval Analysis for Computer Graphics*; Hart, *Sphere Tracing* | Bounds and conservative step reasoning for implicit geometry | Established principles; this prototype has not certified its floating-point bounds to their standard. |
| Prusinkiewicz and Lindenmayer, *The Algorithmic Beauty of Plants* | Bounded grammar as a compact generative source | Genesis currently expands its grammar; no lazy derivation claim. |

No reference implementation was executed under matched input/hardware. The
concept of hierarchical pruning or shared implicit subprograms is not novel.
The contribution here is a reproducible local counterexample pair: separated
instances benefit from occupancy pruning, while dense overlap does not, plus a
visible smooth-composition expressiveness failure. A publication claim would
need matched competitors, formal numerical bounds, GPU hierarchy, ray parity
and varied hardware/geometry families.

## Decision and next milestone

**CONDITIONAL: retain flat Field Graph as a local evaluator and investigate a
bounded, definition-aware spatial layer.** The CPU occupancy experiment supports
that direction for separated repeated structures. WorldState definition sharing
and compiled program reuse already work at eight instances. Do **not** replace
the renderer, increase limits, create a second general compiler, or call the
materialized grammar lazy. Do **not** grant the research index collision authority.

GR-2 should focus first on **numerically qualified hierarchical spatial
queries**, before production GPU acceleration. Acceptance criteria:

1. A conservative or explicit-uncertainty exclusion contract for transformed
   bounded subgraphs, with near-boundary, large-coordinate and `f32`/`f64`
   counterexamples in tests.
2. Independent ray/identity/normal parity against analytic references, plus a
   correct overflow/fallback policy; re-evaluate overlap and tiny-scene costs.
3. Only if (1–2) hold, bounded GPU traversal of shared programs with matched
   direct/accelerated frames, update costs, memory and compile measurements.

Revisit a richer smooth operator separately after its distance, gradient,
bounds and GPU fallback contracts are specified. Lazy grammar remains an open
question requiring conservative bounds for entire unexpanded derivations.

## Qualification and reproduction

On Windows, `cargo fmt --all -- --check`, `cargo clippy --workspace
--all-targets --locked -- -D warnings`, `cargo build --workspace --locked`,
`cargo test --workspace --locked`, `cargo test --workspace --locked --
--ignored --test-threads=1 --nocapture`, `cargo doc --workspace --no-deps
--locked`, and `cargo deny check advisories licenses` all passed. Ordinary
workspace tests: **179 passed, zero failed, 89 ignored**. The ignored pass:
**89 passed, zero failed**. WSL Ubuntu 24.04 with Rust 1.94.0, one build job
and `CARGO_TARGET_DIR=/tmp/genesis-qa0-target` passed fmt, Clippy, build,
ordinary workspace tests and rustdoc. Native GPU observations were on Windows
only. `cd site && npm run build && npm run check-links` passed with this report:
214 generated pages and 22,584 checked local links. The existing nonfatal
sitemap warning remains because the public `site` URL is unset. `actionlint`
was unavailable locally; GR-1 did not edit workflow files.

Focused commands:

```powershell
cargo test -p world-authoring --test gr1_hierarchy --locked
cargo test --release -p world-authoring --test gr1_hierarchy --locked benchmark_flat_and_hierarchical_occupancy -- --ignored --nocapture
cargo run --release -p authoring-inspect --locked -- examples/authoring/gr1-branching.json --measure
cargo run --release -p field-graph-preview --locked -- examples/field-graph/definitions/gr1-organic-loop.json
cargo test --release -p world-lab --locked gr1_arcade_renders_and_updates_shared_definition -- --ignored --nocapture --test-threads=1
$env:GENESIS_AUTHORING_CAPTURE_DIR = (Resolve-Path target/gr1-review).Path
cargo test --release -p first-light --locked authored_gpu_images_match_direct_and_bvh -- --ignored --nocapture --test-threads=1
```

The local review README lists exact evidence files, commands, test inventory
and final HEAD. The capture directory is ignored and is not a durable public
artifact. Hosted CI, DX12, Linux GPU and an external implementation comparison
were not observed.
