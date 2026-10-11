# Geometry Research GR-4 — qualified queries over WorldState snapshots

**Decision: CONDITIONAL research result; NO-GO for production collision or a public query API.**
11 October 2026, branch `living-world`. Starting HEAD:
`b31e0fc496e2dcae1c0b5c32f5909f83074dadb9`. Final implementation
revision: `27d193b` (`test(research): qualify WorldState field snapshots`).
The report was first committed as `bbdb1d1`; use
`git log --oneline b31e0fc..HEAD` for subsequent report corrections. Raw local
runs are in ignored `target/gr4-review/`.

## Scope and architecture

GR-2 (`crates/authoring/tests/gr2_spatial.rs`) supplies conservative candidate
exclusion from outward AABBs and a deterministic, bounded binary hierarchy.
GR-3 (`crates/authoring/tests/support/gr3_narrowphase.rs`) supplies interval
analytic entry tests. Both remain **test-only research code**. GR-4 adds
`gr4_world.rs` beside them and a dev-only dependency on `world-state`. No
renderer, GPU, physics, simulation, persistence format, graph grammar or world
budget changed.

The adapter calls `WorldState::validate`, reads enabled Field Graph occurrences
in stable world order, takes the *current* graph from `WorldState::field_graph`
(including runtime parameter or morphology edits), and reads the occurrence's
world transform and stable ID. It derives GR-3 terms, GR-2 bounds and a fresh
hierarchy. The snapshot captures `FieldInstance` and `FieldDefinition` values.
A query against a supplied current state first validates that world, then
compares the captured values with Rust `PartialEq` semantics;
a mismatch returns `Stale`, never an old `Hit` or `Miss`. The immutable snapshot
can still be queried as its captured historical state. Empty enabled-field
sets yield an empty field-domain `Miss`. Unsupported enabled graphs remain in
the candidate set and yield uncertainty; they are not silently dropped.

**Query domain:** enabled visual Field Graph occurrences only. Analytic world
spheres, organisms, authored capsules, resources and kinematic bodies are not
part of this query. It must never be described as a whole-WorldState collision
query. Visual material is owned outside `WorldState` and no material is
returned. The WorldState budget is four field definitions and eight field
instances; the 32/128-instance GR-3 scenes are research-only.

The prototype reuses GR-2's `u32` ID field. A valid world ID above `u32::MAX`
is **rejected** during snapshot derivation; it cannot be truncated. This is an
API limitation rather than a claim that world IDs fit in 32 bits.

## Result semantics

| Status | Meaning in the restricted exterior-start contract |
| --- | --- |
| `Miss` | Every enabled field occurrence was excluded or its supported terms proved no entry, subject to the arithmetic assumptions below. |
| `Hit` | An exterior first-entry parameter and position are enclosed; exactly one primitive occurrence is provably earliest. Stable instance ID and graph occurrence ordinal are supplied. A normal is optional. |
| `HitAmbiguous` | A first exterior entry exists and its parameter/position are enclosed, but the unique owner is not certified. There is no ID, material or normal. |
| `Indeterminate` | Unsupported operation/overflow, ambiguous boundary or tangent, inside start, unsafe intermediate, uncertain candidate, exhausted budget or an unqualified root. It is never converted to `Miss`. |
| `Stale` | The supplied field-domain state differs from the captured state. This is a separate snapshot/current-state result, not a geometric miss. |

For `HitAmbiguous`, **every** relevant analytic term must have returned a
qualified hit or miss, with the start proved exterior to every term. Then the
first entry of the hard union is the minimum of the term entry parameters.
The interval minimum of all hit intervals encloses that first entry. If these
intervals cannot establish a unique winner, no owner is selected. A known
earlier uncertain candidate still forces `Indeterminate`. This applies to
exactly coincident spheres and sufficiently close entries; it does not infer a
hit from overlap of unsupported or unresolved candidates. Internal component
surfaces after the first exterior entry are not reported. A ray starting
inside any component remains unresolved.

The returned source provenance is recoverable from the captured world
occurrence and exact definition revision, but `QualifiedHit` itself has no
revision field. This is one reason the result is not yet a public API.

## Independent finite references

GR-4 tests use analytical line/sphere and slab entry formulas outside the
interval implementation, plus exact axis witnesses. Tested world instances
include positive and negative translations, scales 0.5 and 2, graph scales
0.5, 2 and 3, different transform orders, nested box scaling, nonunit rays,
off-axis rays, misses, coincident instances, a hard union and supported
runtime sphere-radius changes. Numerical intervals are checked for inclusion
of the independent expected parameter and world-space point. Existing GR-3
tests additionally cover axis-aligned boxes, box feature normals, tangent and
near-tangent rays, inside starts, clipping, overflow and budgets; GR-2's
deterministic bounds corpus remains active. These are finite tests, not a
formal proof for all transformed inputs. Exact rational or arbitrary-precision
oracles, broad random transformed corpora and adversarial cancellation are
**not verified**.

The new explicit examples are: two coincident world spheres from `x=-1` have
the same exterior entry at parameter 3 with ambiguous owner; a nested box
under graph/instance transforms has center `x=14`, half extent `x=2`, and a
direction-two ray from `x=0` enters at parameter 6. A parallel ray exactly on
a box face remains `Indeterminate`, as specified by the GR-3 boundary policy.

## Arithmetic audit

GR-3 stores closed binary64 intervals. `add` and `sub` use the appropriate
endpoint pairs; `mul` and `div` evaluate all four endpoint products/quotients.
`div` rejects a denominator interval containing either signed zero. `sqrt`
rejects a negative lower bound. Every accepted scalar result is expanded by
one adjacent representable value in each direction; an infinite/NaN result or
expansion is rejected. Interval `min`/`max` take componentwise endpoint
extrema. Sphere discriminants, roots, box slabs, composed centers/scales,
ray-point reconstruction and sphere normals use these operations. A box
normal is present only for a uniquely separated entry axis. The tests include
NaN, infinity, signed zero, zero-crossing division, overflow and the smallest
subnormal multiplication. No concrete arithmetic counterexample was found in
the tested cases, so no interval primitive was changed.

The argument **assumes correctly rounded binary64 scalar operations** for
the actual operations evaluated, including square root, with no unsafe
reassociation or flush-to-zero. Source inspection and Windows/WSL finite
tests do not establish this contract for arbitrary compiler flags, CPU modes
or GPUs. The guard around exact tangent and near/far boundaries deliberately
returns uncertainty. `t` is a ray parameter, not distance unless direction is
unit length. The query is for the ideal geometry of stored finite binary64
constants, not for the sign of `Graph::sample` or approximate renderer pixels.

## Snapshot invalidation findings

The current-state comparison covers exact definition bytes/revision, instance
IDs/order, transform, enable state, generation, growth, morphology and runtime
parameter channels. It rejects transform, source replacement, enable/disable,
parameter growth and removal/recreation changes in tests; invalid edits leave
the old snapshot current. An unrebuilt deserialized world is rejected as
invalid before a current query. A reset to an empty world is stale. A JSON
WorldState round-trip followed by `rebuild_fields` and validation yields an
equivalent new snapshot; the existing v7–v10 runtime persistence tests pass.
No production save schema changed.

**Limitation:** there is no world-lifetime or reset epoch in the current
`WorldState` API. A separately reconstructed world with *exactly identical*
field definitions, instance IDs and state is observationally equivalent to
the captured field domain. GR-4 cannot prove that it is the same world
lineage. Its “current” check certifies field-state value equivalence under
normal validated-world use, not historical object identity across such a
reset. A public API needs an explicit world/session identity or a borrow-bound
query context before claiming lifetime-stable owner identity. Non-field world
changes and external material changes do not invalidate this field-only
snapshot; that is deliberate for its stated domain.
Finite `f64` values use ordinary equality, so `+0.0` and `-0.0` compare equal;
the query's ideal stored-real geometry also treats them as the same value.

## Measured performance

Windows 11 Pro, Ryzen 5 7500F, Rust 1.94.0 release profile, default target
CPU, fixed seed `0x5eed_2028`. Three independent processes; each reports the
median of seven 1,024-ray batches. The table reports the median of those
three process medians, in milliseconds. Query batches include GR-2 candidate
search, GR-3 interval narrowphase and instrumentation; snapshot construction
is measured separately and includes WorldState validation, current graph
cloning, term derivation, bounds and hierarchy build. “Current hierarchy”
includes the field-state comparison for each ray. Measurements at this
scale are sensitive to timer and cache noise; small differences are not
reliable speedups.

| Valid world | Flat batch | Hierarchy batch | Current hierarchy batch | Snapshot build | Indeterminate | Ambiguous hit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 sphere, mixed rays | 0.3265 | 0.3538 | 0.5137 | 0.0010 | 128 | 0 |
| 4 separated, mixed rays | 0.5740 | 0.6086 | 0.8673 | 0.0026 | 128 | 0 |
| 8 separated, mixed rays | 0.9561 | 0.9930 | 1.4672 | 0.0033 | 128 | 0 |
| 8 coincident, mixed rays | 0.9410 | 1.0675 | 1.4598 | 0.0023 | 128 | 256 |
| 8 coincident, all crossing | 2.9015 | 3.0991 | 3.4715 | 0.0023 | 0 | 1024 |

The current-state path validates the world for every query, so its cost is
material even at this eight-instance scale; batching against an already
validated borrowed state would require a separate API contract. The 128
uncertain mixed rays are exactly tangential to a sphere. In the dense
case all 8,192 analytic term tests yield 1,024 qualified geometric hits with
ambiguous owner. Hierarchy traversal adds work because all bounds overlap;
there is no reason to deploy it automatically for these valid-world scenes.
At eight leaves its recorded index has 15 nodes and 2,488 bytes of measured
leaf/node/term storage lower bound. Raw per-ray p50/p95,
candidate counts, shape tests, index build, edit and refresh times are in the
JSONL review package. Memory there is a **lower bound** on leaf/node/term
storage, excluding allocator, vectors, cloned graph and definition bytes.

The pre-change GR-3 report and raw data have separate 32/128 research scenes.
Its 128 separated-sphere 1,024-ray batch was 1.4698 ms flat versus 0.5706 ms
hierarchy across five processes; its 128 dense coincident batch was 32.8421
versus 36.8944 ms. These are **not production-valid worlds** and the GR-4
ambiguous-hit classification changes the narrowphase result contract, so they
are not a matched before/after performance claim. Static many-query,
dynamic rebuild and dense-overlap costs are represented by the separated
construction/query measures above; no long interactive frame workload was
measured.

## Regression and portability

The changed test target passed on Windows and Ubuntu 24.04/WSL. The complete
workspace on each OS passed **197 tests, 0 failures, 92 normally ignored**
across 45 reported suites. The separate sequential Windows `--ignored` run
executed **all 92 opt-in tests with 0 failures** (including existing GPU
diagnostics and research benchmarks). It used the default test profile; the
GR-4 timing table comes from separate release-profile runs. Windows workspace
formatting, strict Clippy, build, rustdoc
and cargo-deny passed. The existing v7–v10 historical save continuation, deterministic
replay, World Lab, renderer CPU and Living World tests are part of that
workspace run. Site build and local-link check pass under `/Genesis/`.
Two existing native GPU diagnostics passed separately:
`field_graph_gpu_numerical_boundaries` (tangent `Exhausted`, near-inside `Hit`,
near-outside `Miss` on the tested adapter) and
`world_field_query_preserves_gpu_boundary_diagnostics`. These readbacks do
**not** certify GR-4: WGSL f32 and visual guard results have different numeric
contracts. The existing `y = 1 + 1e-8`, tangent and visual guard exhaustion
cases remain warnings against transferring CPU `Miss` or hit enclosures to GPU.
The Windows opt-in run reported an NVIDIA GeForce RTX 4070 SUPER, Vulkan,
driver 591.86 for existing renderer diagnostics; this is one adapter only.

## Decision matrix and next research gate

| Question | Status | Evidence / gap |
| --- | --- | --- |
| Transformed finite references | PARTIAL | Independent sphere/box formulas and directed tests; no exhaustive or high-precision corpus. |
| Binary64 interval contract | CONDITIONAL | Source audit and two-OS finite tests; backend rounding/FTZ not formally qualified. |
| Geometry separate from owner | PASS for restricted exterior starts | Coincident and near-coincident supported hits retain an enclosed union entry without fabricated ID/normal. |
| Valid WorldState derivation | PASS for enabled visual fields | World validator, current graph, stable IDs, source revisions, actual 8-instance cap. |
| Stale geometry refusal | PARTIAL | Exact field-state changes rejected; identical-state world reset cannot be distinguished as a new lifetime. |
| Hierarchy value at current world scale | INCONCLUSIVE | No consistent gain in measured 1–8-instance cases; dense coincidence is slower, while 32/128 research scenes can benefit when sparse. |
| Dynamic updates | PARTIAL | Transform, definition, visibility, parameter, invalid update and round-trip tested; not every simulation event/replay path in one integrated experiment. |
| GPU numeric parity | NOT VERIFIED | No GR-4 binary64 interval port or equivalent GPU contract. |
| Public API / physics integration | NO-GO | World-lifetime provenance, broader oracles and GPU boundary remain unresolved. |

**Primary next step:** define a world/session identity and extend independent
numeric witnesses before considering an exported query API. Then test (1)
reset/reload lineage, (2) exact or higher-precision transformed cases near
the boundary, and (3) representative dynamic workloads at the valid eight
field-instance cap. The gate is zero stale definite answers, zero invented
owner/normal, and a stated rounding model on each supported target. Do not
increase world limits, port the interval code to WGSL, or grant collision
authority on the basis of these finite tests.

## Reproduction

```sh
cargo test -p world-authoring --test gr2_spatial --locked
cargo test -p world-authoring --test gr2_spatial --release --locked benchmark_valid_world_lifecycle -- --ignored --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
cargo deny check advisories licenses
cd site && BASE_PATH=/Genesis/ npm run build && BASE_PATH=/Genesis/ npm run check-links
```

Windows PowerShell users should set `$env:BASE_PATH='/Genesis/'` before the
site commands. The benchmark should run in release mode, repeatedly in
independent processes, and on the actual target CPU. Timing and finite
differential agreement are measurements, not a universal numeric proof.
