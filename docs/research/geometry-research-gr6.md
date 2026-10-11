# Geometry Research GR-6 — native GPU hierarchy traversal

**Decision: retain materialized GPU BVH; keep compact traversal research-only.**
11 October 2026. Branch `living-world`; starting HEAD `00471d32ba492e339df788a04f0b80e0a313ede8`.
Implementation commit `1dd4ed4` (`feat(research): traverse compact geometry hierarchy on GPU`).
The final local HEAD is the commit containing this report. No remote operation was
performed. Raw JSONL, fixtures, logs and actual GPU captures are ignored under
`target/gr6-review/`. This is a finite experiment on one RTX 4070 SUPER/Vulkan
adapter, not a cross-backend or certified numerical result.

## Architecture and representation

GR-5's path is validated JSON → CPU depth-first expansion → `Scene` → GPU
primitive records → optional derived BVH. Its fixed analytic WGSL source is
11,016 bytes. GR-6 additionally packs the same
validated source into shared analytic definitions (48 bytes each), hierarchy
nodes (48 bytes each, including a local subtree AABB), and child/repeat edges
(48 bytes each). No GPU record contains a transform for each derived leaf.
The CPU computes subtree leaf counts and local bounds once during packing; it
may revisit shared DAG subtrees. The shader source is fixed at 10,604 bytes
for all tested structures. It does not generate specimen-specific WGSL.
`WorldState`, collision, save formats, replay and production rendering paths
remain untouched.

The shader uses an explicit 12-frame DFS stack. Group children and repeat
iterations compose translation, +Z rotation and positive uniform scale in
depth-first order. A repeat frame advances its transform after each child.
Skipped subtrees advance the ordinal by their precomputed leaf count; the
first leaf has ID 0, as in GR-5 CPU materialization. Structural edits can
renumber ordinals. A parameter-only global scale keeps them stable. A world
ray retains its parameter `t`: the shader transforms leaf endpoints and radius
to world space before sphere/capsule intersection. Normals are computed from
that transformed geometry. An equal-depth tie keeps the first DFS leaf, like
Direct's strict `<` selection. Inside starts use the exit intersection.

Limits: 12 stack frames, 2,048 node visits, 512 leaf evaluations and 4,096
loop iterations per ray. The shader checks packed counts, child/definition
indices, stack and work capacity. Any failure returns explicit `exhausted`
diagnostics and magenta in the research image; the CLI rejects such a frame.
Host validation retains GR-5's 16 KiB source, 64 nodes/definitions, depth 12,
16 group children, 32 repeat count, 2,048 expanded visits and 512 leaves.
An empty storage binding allocates one full record. Layout sizes are asserted
in a Rust test and exercised by actual GPU shader creation/readback.

Subtree AABBs are computed in CPU `f64`, rounded outward by a 64-`f32`-epsilon
coordinate pad, then tested in shader local coordinates with a small ray-`t`
slack. These are **empirical visual bounds**, not certified exclusion. The
unpruned traversal is the research reference. The pruned path is accepted only
for observed frames with zero unpruned/pruned hit-ID, depth and color mismatches.
No GR-2/3/4 CPU numerical guarantee transfers to WGSL.

## Inputs and actual GPU observations

The unchanged branching source (SHA-256
`a6eecc397ab5b740d774b25dec4a350dfd7ca10a1f4e48e3d263b68a55b43e19`)
has 7 nodes, 2 definitions and 242 leaves. The unchanged arcade source
(SHA-256
`20feb822ada307a7a66da2ac2b092530be87cc7c9ba4b004f874cfc9ef4744ec`)
has 6 nodes, 3 definitions and 84 leaves. Both use the same GPU pipeline and
layout. Real 640×480 images show the full branching tips and all three arcade
tiers; no geometry disappeared in the pruned captures. See ignored
`branching-pruned.png`, `arcade-pruned.png` and PPMs in the review directory.

Release-profile GPU timestamp medians below are seven post-warmup samples per
path, one adapter, one camera per specimen. The `base` rows are the clearest
matched comparison; `grown` changes geometry and framing. Later `returned`
timings showed large clock/load variation and are retained in JSONL for
correctness only, not used to infer speedups. CPU times are individual nested
operations, not additive end-to-end latency.

| Workload, base | Leaves | Direct ms | BVH ms | Hierarchy raw ms | Hierarchy pruned ms | Packed geometry bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| One sphere | 1 | 0.0067 | 0.0115 | 0.1147 | 0.1147 | 96 |
| Sparse repeat | 26 | 0.0393 | 0.0149 | 0.4239 | 0.1219 | 192 |
| Sparse pairs | 36 | 0.0522 | 0.0171 | 0.7557 | 0.1208 | 336 |
| Fully overlapping spheres | 32 | 0.0479 | 0.0346 | 0.5048 | 0.1659 | 192 |
| Arcade | 84 | 0.2776 | 0.0383 | 1.4561 | 0.1382 | 816 |
| Branching | 242 | 0.6280 | 0.0567 | 5.0258 | 0.2212 | 1,488 |
| Sparse nested repeat | 512 | 0.6669 | 0.0367 | 8.3589 | 0.3584 | 288 |

All timings came from actual offscreen compute with readback on NVIDIA RTX
4070 SUPER, Vulkan, Windows, 640×480, release build. These scenes have
different spatial distributions and camera coverage; rows are not a fitted
scaling law. No tested workload beat the existing materialized BVH. Direct
beat BVH for the one-leaf case. The pruned path substantially improved on
unpruned hierarchy, but remained roughly 3.6× slower than BVH for arcade and
3.9× for branching base. Dense overlap also lost. No break-even was observed
through the existing 512-leaf research limit.

## Memory, preparation and updates

| Base specimen | Authored JSON | Definitions | Nodes | Edges | Compact geometry | Flat primitives | BVH records | Flat + BVH |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Branching | 1,701 | 96 | 336 | 1,056 | 1,488 | 15,488 | 8,128 | 23,616 |
| Arcade | 1,093 | 144 | 288 | 384 | 816 | 5,376 | 4,032 | 9,408 |

These are source/record byte counts, not measured driver allocations. The
research renderer additionally allocates a **14,745,600-byte per-pixel
diagnostic buffer and equally sized readback buffer**, plus the 1,228,800-byte
RGBA target and small uniform/timestamp resources. Its *total* allocation is
therefore not smaller than the existing renderer. Compact geometry savings
would matter only after a production design removes or reduces diagnostics.

The base run measured branching parse 0.185 ms, compact pack/bounds 0.031 ms,
CPU materialization 0.048 ms and BVH build 0.322 ms; arcade measured 0.111,
0.005, 0.031 and 0.071 ms respectively. These single measurements are noisy
and include different nested work. Hierarchy pipeline creation was 2.04 ms
and 1.45 ms in separate processes. `update_program` replaces validated GPU
buffers while retaining the same pipeline. An opt-in GPU test changed a shared
sphere radius, returned to its old value, changed repeat count 3→2, and
observed correct changed/returned frame diagnostics. Its buffer creation
measurements were 0.089 and 0.079 ms. The CLI also replaced the canonical
branching stem radius (0.09→0.12) and arcade repeat count (7→6) on the same
device/pipeline: 7,481 and 1,628 pixels changed, and the updated images were
byte-identical to fresh-renderer images. Enqueue-side replacement was 0.049
and 0.040 ms respectively; none of these values measures GPU upload
completion. Global scale 1→1.3→1 changes only the 112-byte
uniform for the compact path; Direct/BVH rematerialize and rebuild. Bounds in
node-local coordinates remain valid under the global scale. Other edits require
repacking bounds and replacing buffers; selective subtree refit was not built.

## Correctness qualification and limitations

The actual GPU test covers sphere, capsule, nested rotation, repetition,
shared definition/overlap, empty group, parameter return and buffer replacement.
It passed. Both canonical base/grown/returned scenes produced zero exhaustion;
pruned and unpruned outputs were byte-identical, with zero hit-ID/depth
mismatches over all 307,200 pixels. Maximum unpruned visits/leaves were
363/242 for branching and 109/84 for arcade; the 512-leaf case remained within
the existing work budget. Branching base's pruned shader recorded 940,859
subtree skips summed across pixels. This is observed GPU instrumentation, not
a count of mathematically certified misses.

**Materialized Direct versus compact hierarchy is not exact at every pixel.**
For branching base, 135 colors differed; GPU Direct queries at those pixels
found 3 hit/miss and 10 identity differences. Grown had 203 color differences,
2 hit/miss and 4 identity differences. Arcade base had 69 changed colors and
2 identity differences. The 512-leaf variant had 10 identity differences.
The 289 fixed sampled rays per canonical state had zero identity mismatches,
but edge pixels expose the gap. CPU materialization composes transforms in
`f64` before conversion to GPU `f32`; hierarchy composes in WGSL `f32`.
Near-tangent and near-coincident results can consequently differ. No tolerance
was widened to classify these as equal. This blocks production visual parity
and any certified spatial-query claim. Pruning matched the *unpruned hierarchy*
in the tested frames; this does not establish safety for all valid inputs.

Normal differences above 0.02 occurred on 16 of the branching-base differing
color pixels; one depth difference exceeded 0.002 world units. GPU f32
subtree bounds and intersection arithmetic remain heuristic. Exhaustion is
explicit, but malformed-device-input defense is secondary to host validation.
There is no fallback to materialized rendering in the research shader; its
magenta/exhausted output must not be treated as a complete frame.

## Decision and next questions

| Question | Result |
| --- | --- |
| Genuine compact GPU execution; one shader for two structures | PASS |
| Supported ideal sphere/capsule geometry and DFS IDs | PARTIAL: GPU f32 edge disagreements |
| Correct first hit/depth/normal on all tested pixels | FAIL versus materialized Direct; PASS only on the sampled stable corpus |
| Explicit bounded work/exhaustion | PASS in valid non-exhausted frames and a deliberately cyclic packed-input GPU diagnostic test; the test bypasses host validation only to exercise the guard |
| Compact geometry memory | PASS for record bytes; FAIL for whole instrumented prototype allocation |
| GPU performance versus BVH | FAIL on every measured workload |
| Dynamic scale and buffer replacement | PASS for tested parameter/count edits; partial for general topology updates |
| Compiler/program reuse | PASS for fixed analytic shader and `update_program` test |
| Shared implicit Field Graph leaves | NOT VERIFIED; separate generated mixed-field evaluator lacks a runtime-indexed program interface here |
| Production integration | NO-GO |

This is nested analytical instancing with lazy repeat traversal and optional
hierarchical visual culling, not a new general implicit evaluator or a
mesh/voxel representation. GPU hierarchy traversal and BVH are established
families of techniques; compare the [*pbrt* BVH chapter](https://pbr-book.org/4ed/Primitives_and_Intersection_Acceleration/Bounding_Volume_Hierarchies).
No novelty claim is made.

**Primary outcome: keep CPU materialization plus GPU BVH.** Compact hierarchy
is useful as authoring data; this shader remains a qualified research
counterexample to assuming compact execution is faster. GR-7 should focus on
one numerical strategy for transform/near-tangent parity and a lower-overhead
diagnostic mode before considering selected GPU hierarchy workloads. Do not
make this path authoritative, raise production limits, add implicit leaves or
replace the production BVH on these measurements.

## Reproduction and scope of verification

From repository root, run `cargo test -p geometry-research --locked`, then
`cargo test -p geometry-research --locked -- --ignored --test-threads=1
--nocapture`. For each canonical JSON, run `cargo run --release -p
geometry-research --locked -- <source.json> target/gr6-review/<name>`.
The executable records per-path GPU timestamp samples, CPU preparation,
identity/depth/normal diagnostics and PPMs in `measurements.jsonl`. Run GPU
benchmarks sequentially and avoid concurrent GPU tests. Windows workspace
format, Clippy, build, rustdoc and ordinary tests passed (206 passed, 0
failed, 93 opt-in tests skipped in that run). The separate opt-in suite passed
93/93, including real GPU tests. `cargo deny check advisories licenses`
passed. WSL Ubuntu 24.04 passed focused GR-6 tests, workspace Clippy, build,
ordinary tests and rustdoc; it was not treated as a second GPU/backend test.
The site built 224 pages and verified 24,740 local links. Windows/site logs
and a WSL command/result summary are retained in the ignored review package.
Hosted CI, DX12 FXC, another physical GPU and
numerical certification were not verified.
