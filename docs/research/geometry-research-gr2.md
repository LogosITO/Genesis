# Geometry Research GR-2 — numerically qualified hierarchical spatial queries

**Status:** CONDITIONAL, 11 October 2026. **Branch:** `living-world`.
**Starting revision:** `4de0e05e9a5ee97e990034a336dff9e47e669ce0`.
**Final revision:** the local GR-2 report commit (`git rev-parse HEAD`).
The prototype is test-only. It does not change `WorldState`, renderer,
simulation, persistence, GPU shaders or production limits. Raw measurements
and the local test inventory are in ignored `target/gr2-review/`.

## Question and result

Can a bounded hierarchy exclude a finite ray segment from mathematical
geometry without turning uncertain floating-point outcomes into a false
`Miss`? For the **ideal occupied set of a restricted hard Field Graph** and
ideal analytic capsule bounds, yes, under the explicit binary64 operation
model below. `crates/authoring/tests/gr2_spatial.rs` returns only
`OutsideCertified`, candidate IDs, or `Indeterminate`; it does not return a
surface hit, distance or normal. Full independent Field Graph ray hit parity
therefore **did not pass the prerequisite for GPU hierarchy work**. No GR-2
GPU traversal was attempted.

## Audit of current contracts

| Operation or path | Established ideal contract | Floating-point guarantee / unknown |
| --- | --- | --- |
| Sphere and axis-aligned box | Exact SDF primitive in ideal arithmetic; finite closed occupied set | `Graph::sample` returns computed binary64, with no certified sign enclosure; near-zero classification is uncertain. See `crates/field/src/lib.rs` and `crates/authoring/src/field_graph.rs::sample_node`. |
| Capsule | Analytic finite swept segment; AABB from endpoint component extrema ± radius | Capsule is an analytic renderer primitive, **not** a Field Graph node. Renderer intersection roots/normals are computed binary64, not formally enclosed. See `crates/field/src/lib.rs::Capsule` and `crates/renderer/src/lib.rs::intersect_capsule`. |
| Positive uniform scale and translation | Transform the ideal occupied set; hard-graph AABB transforms componentwise | `properties` rounds each Graph bound operation outward. GR-2 rounds each *additional* instance multiply and add separately outward; this depends on ordinary correctly-rounded binary64 operations. Floating sample evaluation follows a different rounding path and is not covered. |
| Hard union | Occupied set is union; componentwise min/max encloses both child AABBs | Signed unit-Lipschitz ideal field; generally not exact SDF. Internal zero artifacts and tie normals remain possible. Graph validator has node/depth/visit budgets. |
| Smooth union | Existing validator requires exact-SDF children and enlarges ideal bounds by `width/4` | GR-2 refuses to certify or cull it; its numerical sign, normal and exterior entry require separate qualification. A smooth node yields `Indeterminate` in the prototype. |
| CPU field ray tracing | Restricted exterior-entry path can return approximate entry or bounds miss; work/iteration failures are explicit | `SolidEntryOutcome::ApproximateEntry` is not a certified root. Starts inside, near zero, unsupported smooth operations, exhausted work and uncertified advances cannot be converted to `Miss`. See `field_graph/spatial.rs`. |
| CPU analytic `Scene` and BVH | Shared sphere/box/capsule intersection, stable primitive IDs and nearest-hit selection | GR-2 independently checks selected axis-aligned expected distances/normals, then Direct/BVH parity. Both paths still share the leaf intersection implementation; arbitrary orientation and all tangencies are not formally certified. |
| GPU Field Graph and visual guard | Derived f32 shader evaluation, bounded ray work, visual bounds skip | No backend-wide rounding proof. `1 + 1e-8` becomes `1` in f32; QA-2 found 496 hidden `Exhausted` outcomes behind visual culling. The guard is not an authoritative geometric `Miss`. No GR-2 GPU evidence is claimed. |

`Graph::bounds()` encloses the **ideal** occupied set under its documented
arithmetic assumptions. `Graph::sample(point) <= 0` is a computed sign, not a
proof of ideal occupancy. GR-1's one-ULP instance expansion was a useful
experiment, not by itself a proof for arbitrary field evaluation or GPU f32.
This distinction determines the narrow GR-2 contract.

## Research-only broadphase contract

The input scene has at most 128 private leaves with unique stable `u32` IDs.
Each leaf is an AABB of either a validated hard Graph under positive uniform
instance scale/translation, or an ideal capsule from finite endpoints and a
positive radius. Smooth Graphs and nonfinite/overflowed bounds retain no
certified AABB and force `Indeterminate`; they are never silently dropped.
The index has at most 255 nodes and deterministic center/ID sorting.

For one binary64 addition or multiplication of finite stored inputs, the
round-to-nearest result and its adjacent representable values enclose the
exact real result. The prototype applies a directed adjacent value at **each
operation** in the bound derivation, not one fixed margin at the end. For a
positive scale, interval endpoints remain ordered. Union takes the extrema
of child enclosures. A ray coordinate is `origin + direction × parameter`;
over a closed parameter interval the affine coordinate extrema occur at its
two endpoints. Both endpoint products and sums receive outward enclosure.
Strict separation on any world axis then proves the ideal ray segment cannot
intersect that leaf's ideal occupied set. Touching bounds is a candidate.

This argument assumes IEEE-754 binary64 correctly-rounded scalar `+` and `*`,
finite operands/results, no reassociation that changes those operations, the
existing hard-Graph bounds induction, and a ray interpreted using its stored
binary64 direction and parameter interval. A non-unit direction means the
parameter is **not** Euclidean distance. Overflow or unsupported vocabulary
produces `Indeterminate`; there is no numeric fallback to `Graph::sample` sign.
The result does not certify a CPU/GPU field sample, a zero crossing, a ray
normal, or a physical collision.

`Scene::replace` refreshes a leaf and increments a checked generation. A stale
hierarchy takes the fresh **flat** bounds path; if that path cannot establish
exclusion it returns candidates or `Indeterminate`. Work budget exhaustion
always returns `Indeterminate(WorkBudget)`. Node/leaf and actual field-evaluation
counts are reported. The broadphase evaluates the field **zero times**. Source
definitions remain outside the index; the test reuses validated Graph data but
does not create a second compiler or WorldState authority.

## Independent checks and counterexamples

`gr2_spatial.rs` exercises 3,072 deterministic centerline hit/miss ray pairs
(6,144 ray queries) from
seed `0x5eed_2026` across sphere, box and capsule bounds. The independent
axis geometry says each centerline ray reaches a solid at `center.x - radius`
or `center.x - half_extent`; none was excluded. Tests also cover exact boundary
and near/far endpoints, inside origins, tangents and near tangents, two nearly
coincident spheres, nested graph and instance transforms, coordinates near
9,000, fully overlapping instances, numeric overflow, zero budget, and both
transform and definition replacement. A new hit at the enlarged radius is
still a candidate through the stale-index flat fallback. Smooth union and
overflow yield `Indeterminate`. This is a finite deterministic corpus, **not**
a formal proof of every supported floating-point state.

`crates/renderer/tests/gr2_analytic_reference.rs` checks independent simple
axis distances and normals for sphere, box and capsule, including interior
origins, an exact tangent, a distant sphere, a miss and near-coincident hit
ordering. The existing `Scene::intersect` and `Bvh::intersect` agreed in these
cases. Since they call the same leaf math, their agreement alone would not be
independent evidence; the explicit axis expectations are the independent part.
These tests do **not** establish a Field Graph hit or normal from the new index.

Two counterexamples constrain any broader claim:

1. A ray at sphere `y = 1 + 1e-8` is outside in the tested binary64 ideal
   model, while converting that coordinate to f32 yields exactly `1`. An f32
   point classification cannot inherit the binary64 exclusion proof.
2. A zero-work query and an unsupported smooth Graph return `Indeterminate`.
   Treating either as `Miss` would discard possible geometry or uncertainty.

The existing QA-2 496-ray hidden-`Exhausted` visual-guard counterexample is a
separate observed GPU warning. GR-2 did not repeat it or alter its policy.

## CPU performance, construction and update

Local Windows 11 Pro, AMD Ryzen 5 7500F, Rust 1.94.0, release profile.
The same 1,024 finite ray segments were queried by flat and hierarchical
broadphases, seven repetitions each. The table gives median milliseconds per
**batch**; `target/gr2-review/benchmark.jsonl` has all 14 distributions,
candidate counts, node/leaf tests, memory and frequencies. Graph compilation
and leaf construction are outside `index_build_median_ms`; the seven update
measurements include replacement bound creation and full index rebuild.

| Distribution | Instances | Flat query | Hierarchy query | Index build | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| One motif | 1 | 0.0204 | 0.0392 | 0.0001 | Hierarchy slower |
| Separated, sparse rays | 32 | 0.3205 | 0.0830 | 0.0013 | Hierarchy faster |
| Separated, sparse rays | 128 | 1.2734 | 0.2948 | 0.0111 | Hierarchy faster |
| Partly overlapping, sparse rays | 128 | 1.2922 | 0.1382 | 0.0111 | Hierarchy faster |
| Fully overlapping, mostly missing rays | 128 | 1.2305 | 0.0644 | 0.0068 | Whole-tree rejection helps |
| Fully overlapping, all bounds visited | 32 | 0.9208 | 1.8327 | 0.0016 | Hierarchy slower |
| Fully overlapping, all bounds visited | 128 | 3.3836 | 6.7283 | 0.0039 | Hierarchy slower |
| After instance move, separated | 32 | 0.3553 | 0.1045 | 0.0013 | Update + rebuild median 0.0015 ms |

All matched query outcomes agreed in the benchmark. Stable rebuilt runs had
zero fallbacks and zero indeterminate outcomes; the explicit stale-index check
used one flat fallback. Actual field evaluations were **zero for both methods**
because this measured broadphase does no narrowphase. For 128 leaves the index
has 255 nodes (18,360 bytes of node structures) and 8,192 bytes of leaf
records, excluding vector capacity/allocator and source definition storage.
The data do not establish full ray-query speedup, presentation latency, or GPU
performance. Sparse rays can make even overlapping geometry cheap to reject;
dense overlap removes that benefit. The 128-instance scale exceeds the current
eight-Field-instance WorldState budget and is research-only.

## Decision and next milestone

**CONDITIONAL GO for the restricted CPU exclusion theorem and test-only
broadphase; NO-GO for production hit/pick/physics authority and GPU hierarchy
at GR-2.** The hierarchy has a useful spatial role, but independent Field Graph
ray-hit, distance, normal and numerical-uncertainty parity are missing.

The next milestone should qualify a **restricted narrowphase** for exterior
rays against hard sphere/box unions. It needs explicit numeric enclosures for
candidate roots and normals, an ambiguous/tangent/inside policy, deterministic
stable-ID tie handling, and tests against independent analytic references at
the same transforms. Only after that should a GPU hierarchy be proposed. A
small-scene/overlap bypass policy should be measured at full query level, not
inferred from broadphase timings. Do not raise budgets, introduce a new compiler,
or make smooth union/capsule Field Graph nodes as a side effect of GR-2.

## Qualification

Windows: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--locked -- -D warnings`, `cargo build --workspace --locked`, `cargo test
--workspace --locked`, `cargo doc --workspace --no-deps --locked`, and
`cargo deny check advisories licenses` passed. The ordinary workspace run had
**184 passed, zero failed, 90 ignored** across 45 test/doc-test suites. The
GR-2 release benchmark ran separately and passed. WSL Ubuntu 24.04 passed
fmt, Clippy and the same 184 ordinary tests with one build job. `cd site &&
npm run build && npm run check-links` passed: 216 pages and 23,006 local links;
the existing sitemap warning remains because no public `site` URL is set.
Exact logs and reproduction commands are in the local review README. The
90 ignored tests were **not** all run in GR-2; no new GPU hierarchy was built.
Hosted CI, DX12/Linux GPU parity and a formal backend-wide f32 rounding proof
were not performed. No remote operation is part of GR-2.
