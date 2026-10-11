# Geometry Research GR-5 — compact structures and materialized GPU execution

**Decision: CONDITIONAL for hierarchical authoring; NO-GO for claiming lazy GPU hierarchy.**
11 October 2026. Branch `living-world`. Starting HEAD
`26335f37f94fbb72ccd7e117a3e9d1ceae138ebe`; implementation commit
`20abec6` (`feat(research): add bounded hierarchical geometry preview`), with
the repeat-boundary correction `4ce16c2` (`fix(research): skip unused final
repeat transform`). The
final local HEAD is the report commit containing this file; use `git rev-parse
HEAD` after checkout. No remote operation was performed. Raw, ignored evidence
is in `target/gr5-review/`. Results are local research observations, not
production or cross-backend guarantees.

## Existing architecture and what was reused

| System | Actual status | GR-5 relationship |
| --- | --- | --- |
| Field Graph JSON, `Graph::sample`, Exact/Structural/Combined/Auto/Authoring | Production visual path; bounded 32-node DAG, 12 depth, 64 expanded visits; 4 definitions and 8 world instances | Retained for local implicit shapes. GR-5 does not enlarge its limits or alter compilation. |
| Analytic `Scene`, Direct and GPU BVH | Production renderer snapshot derived from `WorldState` | Reused unchanged for actual offscreen GPU execution of GR-5's derived spheres/capsules. |
| Authored grammar | Production derivation of identified analytic capsule segments | Existing compact source but materializes segments; GR-5 does not replace it. |
| GR-1 CPU occupied-point index | Test-only research; 128 instances, shared graphs, conservative experimental AABBs | Shows sparse point-query benefit and overlap counterexample; it is not a field or GPU evaluator. |
| GR-2/3/4 qualified queries | Test-only CPU research over restricted hard graphs and snapshots | Numerical contract reference only; not reused as GPU culling or collision authority. |
| GR-5 JSON hierarchy | New research-owned example package | Data-driven DAG grouping/repetition; expands to analytic `Scene`. It is not `WorldState` state. |

Flow: JSON definition → bounded DAG validation → deterministic depth-first
expansion → analytic `Scene` → existing fixed analytic WGSL → Direct or derived
GPU BVH → real offscreen texture/readback. Definition and node IDs live in the
JSON; derived primitive IDs are depth-first ordinals. All GPU resources remain
device-owned by the existing renderer. The hierarchy itself is **not** stored
or traversed on the GPU. The GPU BVH indexes materialized primitives and does
not make this a lazy mathematical hierarchy.

## Falsifiable hypotheses and result

| Hypothesis | Discriminating workload and rejection criterion | Observation |
| --- | --- | --- |
| H1 compact source | Two different structures; reject if source grows with every derived feature | 1,701-byte branching source describes 242 leaves; 1,093-byte arcade source describes 84. Supported for these fixtures, with verbose diagnostic flat JSON as comparator. The one-leaf variant retains all unused authored nodes (4,225 bytes versus 159-byte flat dump), a counterexample to unconditional compactness. |
| H2 definition reuse | Several differently transformed leaves share one named definition; reject if shader or definition is duplicated per occurrence | Two definitions in branching, three in arcade; fixed analytic WGSL does not grow with leaves. Supported for materialized analytic leaves. |
| H3 spatial locality | Compare Direct/BVH with identical scene/camera; reject a universal benefit if a small scene loses | BVH helped 84/242-leaf scenes but was slower for one leaf. This measures derived BVH, not hierarchy evaluation. |
| H4 dense overlap | Repeat identical motifs at one place; reject universal sparse-scene inference | 84-leaf overlap still favored BVH in this frame; its BVH median rose from 0.117 to 0.139 ms, but camera framing changed, so an isolated overlap penalty is unproved. |
| H5 dynamic updates | Rebuild scale 1.0→1.3 with same renderer; reject if image/bounds do not change or source recompiles | Bounds and actual pixels changed; same fixed pipeline was reused. Materialization and buffer updates are still required. |
| H6 compilation scalability | Hold executable fixed while derived leaves change 1→242; reject if generated source grows | Analytic WGSL input remains 11,016 bytes. Pipeline creation is device/driver dependent; this does not prove a general compiled Field Graph scaling result. |
| H7 expressiveness | Branching and architecture through unchanged Rust execution; reject if specimen-specific rendering code is needed | Both render through one grammar and the same renderer. Continuous smooth surface fails this grammar's analytic-leaf vocabulary. |
| H8 authoring usability | Change root or repeat count in JSON; reject if Rust editing is required | Ignored level-2, count-3, one-leaf and overlap variants ran from data changes alone. Schema is low-level and lacks ergonomic authoring tools. |

These observations are finite tests. They do not establish scientific novelty,
asymptotic GPU complexity or a useful production threshold.

## Representation and mathematical contract

The committed source fixtures are
`examples/geometry-research/definitions/branching.json` (SHA-256
`a6eecc397ab5b740d774b25dec4a350dfd7ca10a1f4e48e3d263b68a55b43e19`)
and `examples/geometry-research/definitions/arcade.json` (SHA-256
`20feb822ada307a7a66da2ac2b092530be87cc7c9ba4b004f874cfc9ef4744ec`).
The complete sources are the canonical authored definitions; ignored variants
and their exact bytes remain in `target/gr5-review/`.

Version-1 JSON has named sphere/capsule `definitions`; `leaf`, `group` and
`repeat` nodes; one root; and child placement (`at`, `angle_deg`, `scale`).
Parent placement transforms child translation, adds rotation around +Z, and
multiplies positive uniform scale. The DAG rejects cycles, dangling references,
duplicates, malformed/nonfinite numbers and unsupported source versions. The
current limits are 16 KiB source, 64 nodes, 64 definitions, 12 depth, 16 group
children, 32 repetitions per repeat, 2,048 expanded visits and 512 leaves.
The last bound limits primitive records to 32 KiB before allocator overhead;
the renderer separately validates device buffer limits. Expansion errors are
explicit. An empty group produces an empty scene, not a fictitious primitive.

Spheres and capsules have exact signed-distance formulas in ideal arithmetic.
The occupied structure is a hard union of those solids; a pointwise minimum of
their fields is **not** generally an exact SDF. GR-5's `Bounds` is computed
`f64` endpoint/radius extrema after materialization. It is not an interval
proof of ideal occupancy and is not used to certify a Miss. CPU Direct/BVH use
existing analytic ray arithmetic and share leaf intersection code. Their
agreement is parity, not an independent proof of all tangencies or normals.
GPU pixels and timestamps inherit existing `f32`, driver and traversal limits.
No GR-5 result is a physics contact, authoritative world query or save format.

## Three specimens and visual evidence

| Specimen | Source programs / nodes / maximum authored DAG depth | Derived features / visits | Actual execution |
| --- | ---: | ---: | --- |
| A, recursive-looking branching tree | 2 / 7 / 6 | 242 spheres and capsules / 363 | Same materialized analytic GPU path as B; full, close and scaled captures. |
| B, three-tier repeated arcade | 3 / 6 / 4 | 84 spheres and capsules / 109 | Same path; full, close and count-3 variant captures. |
| C, continuous organic loop | Existing 16-node Field Graph (`gr1-organic-loop.json`) | 23 visits per sample; not a GR-5 hierarchy | Re-rendered on existing Field Graph GPU interpreter; four smooth pairs joined by hard unions have visible seams. |

The actual GPU readbacks are `target/gr5-review/branching/{base,grown}-{direct,bvh}.ppm`,
`target/gr5-review/arcade/{base,grown}-{direct,bvh}.ppm`, corresponding
`branching-detail/` and `arcade-detail/` views, plus
`target/gr5-review/continuous-organic.ppm`. PNG copies preserve the readback
pixels for inspection. The A close view visibly shows five branch levels and
fine terminal features. The B close view shows repeated columns, beams and
keystones. Growth changes the actual A/B image. Direct and BVH images match
byte-for-byte for all tested states. These are display results, not a geometric
proof. The C image visibly retains hard seams; its GPU center ray returned
Miss, while the CPU solid query returned
`Indeterminate(UnsupportedOperation)` for that tested ray.

Specimen C identifies a concrete expressive limit. Current Field Graph
`smooth_union` requires two exact-SDF children; its output is a signed
unit-Lipschitz field, not an exact SDF, so nested smooth chains are rejected.
GR-5's hierarchy has analytic leaves and hard occupied-set union only. It
cannot encode a continuously blended multi-part surface through the same
grammar. No ad hoc new shader/operator was added to conceal this failure.

## Matched measurements

Windows x86-64, Rust 1.94.0, wgpu 30.0.1, NVIDIA GeForce RTX 4070 SUPER,
Vulkan, 640×480, release build. Each GPU mode/state has one warm-up and seven
timestamp samples from the same `Scene` and camera; p95 below is the maximum
of seven sorted samples (nearest-rank convention). Direct and BVH consume
identical materialized geometry. `full_frame` in JSONL is a blocking draw plus
texture-copy/map/readback observation, not presentation interval. CPU parse,
materialization and pipeline creation each have one timed observation per
process; they are not stable latency distributions.

| Data-only case, base state | Leaves | GPU Direct p50/p95 ms | GPU BVH p50/p95 ms | Blocking Direct/BVH p50 ms |
| --- | ---: | ---: | ---: | ---: |
| Branching level 4 | 242 | 0.837 / 1.504 | 0.174 / 0.183 | 1.776 / 0.755 |
| Branching level 2 | 26 | 0.074 / 0.074 | 0.024 / 0.024 | 0.527 / 0.478 |
| Single stem | 1 | 0.011 / 0.013 | 0.026 / 0.027 | 0.447 / 0.444 |
| Arcade count 7 | 84 | 0.639 / 0.950 | 0.117 / 0.121 | 1.325 / 0.596 |
| Arcade count 3 | 36 | 0.123 / 0.123 | 0.035 / 0.036 | 0.552 / 0.479 |
| Arcade, zero repeat step | 84, coincident motifs | 0.889 / 1.305 | 0.139 / 0.144 | 1.472 / 0.694 |

The one-leaf case rejects a universal BVH speedup. Overlap and coverage cases
have different camera framing; no causal speedup factor is attributed solely
to overlap. The bounded hierarchy has no GPU traversal benchmark because it
does not execute on the GPU. The existing analytic BVH is the appropriate
materialized baseline, not evidence for a new lazy algorithm.

| Primary base case | Authored bytes | Diagnostic flat JSON bytes | One materialization ms | Primitive bytes | BVH nodes/depth, allocation | One BVH build / enqueue ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Branching | 1,701 | 56,454 | 0.060 | 15,488 | 127 / 7, 8,192 B | 0.325 / 0.022 |
| Arcade | 1,093 | 15,671 | 0.021 | 5,376 | 63 / 6, 4,096 B | 0.070 / 0.020 |

The diagnostic flat JSON preserves actual analytic records but is not a
supported authored input format; its byte ratio is not a fair comparison with
optimized binary instance data. BVH allocations exclude textures, pipelines,
allocator overhead and driver memory. The recorded first BVH upload queues
8,128/4,032 node bytes respectively; primitive records were already uploaded
in the immediately preceding Direct draw. Queue enqueue time is CPU time, not
the GPU transfer completion time. Fixed analytic WGSL input is 11,016 bytes
for every case. Pipeline creation varied across processes (about 17–943 ms in
observed runs, including cache/driver effects); no reliable complexity trend
can be inferred. Dynamic scale leaves this fixed pipeline untouched and
rebuilds derived primitives/BVH; stable leaf ordinals hold only with unchanged
structure. Source edits can renumber them and have no WorldState revision.

## Correctness, limitations and architecture decision

Seven GR-5 tests cover two families, shared definitions with independent
transforms/scales, nested rotation, empty groups, malformed references,
cycles, invalid numeric scale, depth/expansion budgets, the unused final repeat
transform, definition replacement,
stable IDs for parameter-only changes, bounds presence and CPU Direct/BVH ID
parity on 41 rays. Runtime GPU verification checks 81 CPU Direct/BVH identity
rays per captured state, actual readback equality between GPU Direct/BVH, and
pixel change after growth. These checks do not independently qualify all GPU
hit distances, normals, tangent behavior, near-coincident surfaces or hidden
exhaustion. There is no GR-5 certified Miss or uncertainty-aware GPU hierarchy.

**Recommendation: retain Field Graph for local implicit shapes and permit a
research-only hierarchical authoring layer above existing analytic execution.**
The layer demonstrates useful source compression and reuse for two unlike
structures, but it does not establish a production geometry architecture.
The existing analytic BVH handles the tested materialized scene efficiently;
a new lazy GPU hierarchy would need a matched correctness and speed result
against it. The current grammar cannot express smooth multi-part fields or
arbitrary 3D orientation (rotation is around Z); it has no authoritative
identity, world updates, persistence, replay or collision integration.
No new optimization was accepted after measurement: the measured large-scene
bottleneck already has the existing BVH path, while adding another traversal
would duplicate machinery without a proven gain. Incremental refitting,
runtime leaf-parameter buffers and shader generation were deliberately skipped.

Prior art: bounded generative grammars and hierarchical instancing are
established ideas (Prusinkiewicz/Lindenmayer); function representation and
compiled implicit evaluation exist in [libfive](https://libfive.com/) and
[Fidget](https://github.com/mkeeter/fidget); interval spatial reasoning is
studied by Snyder and sphere tracing by Hart. This campaign ran no matched
competitor, license evaluation or formal novelty study. The engineering
integration, not a claimed new mathematical method, is the result.

**GR-6 GO** only for a bounded experiment that (1) combines shared implicit
leaf programs with an explicit instance hierarchy, (2) compares actual GPU
hierarchy traversal and numerical outcomes with the same materialized geometry,
and (3) demonstrates a dynamic edit without stale bounds. **NO-GO** for a
production hierarchy, changed save format, expanded world caps, collision
authority, or general scripting based on GR-5. A measured GPU win and a
qualified no-false-Miss policy must precede any such integration.

## Qualification and reproduction

The final local qualification records 204 ordinary workspace tests and 92
separately executed opt-in tests (zero failures), plus format, Clippy, build,
rustdoc and `cargo deny check advisories licenses`. WSL/Linux ran the full
workspace (204 passed, 92 opt-in filtered) with one build job and a separate
target; native GPU was qualified only on Windows/Vulkan. Site build (222 pages)
and 24,302 local-link checks are recorded in
`target/gr5-review/README.md`. Hosted CI, DX12/FXC, a second GPU and Linux GPU
are not claimed. The existing DX12/FXC `X3511` limitation remains.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture
cargo doc --workspace --no-deps --locked
cargo deny check advisories licenses
cargo run --release -p geometry-research --locked -- examples/geometry-research/definitions/branching.json target/gr5-review/branching
cargo run --release -p geometry-research --locked -- examples/geometry-research/definitions/arcade.json target/gr5-review/arcade
```
