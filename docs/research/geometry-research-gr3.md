# Geometry Research GR-3 — numerically qualified Field Graph narrowphase

**Status:** CONDITIONAL, 11 October 2026. **Branch:** `living-world`.
**Starting revision:** `fc503ad2f12a8fe79a5288de1eb150d054dc47c9`.
**Final revision:** the local GR-3 report commit (`git rev-parse HEAD`).
This is a test-only CPU experiment. No production renderer, simulation, physics,
`WorldState`, GPU shader or budget was changed. Raw local data and commands are
in ignored `target/gr3-review/`.

## Question and result

Can a finite ray be given a justified first exterior hit, miss, identity and
normal for a restricted hard Field Graph? **Conditionally yes** for ideal
spheres, axis-aligned boxes, positive uniform scales, translations and hard
unions, if the binary64 operations below meet the stated rounding model and
the computed intervals separate sufficiently. Otherwise the research query
returns `Indeterminate`. It never promotes sampled sign, a tangent estimate or
an exhausted budget into a certified `Miss`.

The implementation is `crates/authoring/tests/support/gr3_narrowphase.rs`,
included by the GR-2 spatial test target. It reads the existing validated
`Graph` nodes and GR-2 bounds hierarchy. It does not create a second graph
compiler or a new source of world state. An instance ID and graph occurrence
ordinal identify a unique qualified primitive. A world integration would need
to derive this snapshot from `WorldState` and preserve revision invalidation;
that integration is outside GR-3.

## Existing implementation and contract audit

| Path | Ideal statement | Actual numeric status and risk |
| --- | --- | --- |
| `Graph::sample_node` in `crates/authoring/src/field_graph.rs` | Sphere/box sign and hard-union sign describe the ideal occupied set; hard union is not generally an exact SDF | Recursive binary64 sampling has no outward value enclosure. `sample <= 0` alone cannot certify ideal occupancy or a boundary. |
| `Graph::gradient_node` | A smooth primitive has an analytic normal; union branch ties and box features can have no unique normal | Branch selection uses computed samples; no certified branch ordering near ties. `Gradient::Undefined` is explicit for known singularities, but a returned vector is not a numeric enclosure. |
| `trace_exterior_entry_report` in `field_graph/spatial.rs` | Exterior sphere tracing may approach a first zero in ideal arithmetic | It normalizes direction, stops at a user tolerance, and returns `ApproximateEntry`, not a root enclosure. Starts inside/near zero, unsupported smooth unions, invalid arithmetic, budget and uncertified far advances remain explicit uncertainty. `BoundsMiss` uses conservative graph/ray bounds only. |
| Analytic renderer `intersect_primitive` | Sphere quadratic and box slabs describe ideal ray intersections | Direct/BVH share leaf arithmetic and return point `f64` distances/normals. Discriminant, slab ordering and tie decisions have no interval certificate. Their parity is not independent proof. |
| GR-2 hierarchy | Strictly separated outward bounds exclude an ideal ray segment | Candidate means only possible overlap. Stale generation uses a fresh flat fallback; unsupported or overflowing bounds and budget exhaustion remain `Indeterminate`. |
| GR-3 research narrowphase | Analytic root/entry intervals can enclose an ideal first exterior hit | Valid only under the explicit scalar rounding assumptions below. No claim about `Graph::sample`, GPU f32, arbitrary platform bitwise parity, collision authority or a production `Miss`. |

## Scope and numerical assumptions

The test scene accepts up to 128 instances; each validated Graph has at most
the existing 32 nodes, 12 depth and 64 expanded visits. Only reachable sphere,
box, translate, positive scale and hard-union nodes are flattened. A smooth
union, invalid/overflowing transformed parameter or excessive expansion has
no qualified primitive set and yields uncertainty. The 128-instance research
scale exceeds the production eight-Field-instance limit.

All stored finite `f64` input numbers are treated as exact reals. Each scalar
`+`, `-`, `*`, `/` and square root is assumed correctly rounded in binary64,
without reassociation or flush-to-zero that violates this model. The adjacent
representable values around each rounded lower/upper operation enclose the
ideal result. Interval multiplication/division considers all endpoint pairs;
division by an interval containing zero, overflow and nonfinite expansion
produce `Indeterminate`. This is a conditional arithmetic argument, not a
formal proof of every CPU backend. The direction is finite and nonzero but may
be unnormalized: ray parameter `t` is not necessarily Euclidean distance.
The queried interval is closed `[near, far]`; this prototype qualifies a hit
only when its entire parameter enclosure lies strictly inside it. Boundary
contact at `near`/`far` is conservatively indeterminate.

Transforms are composed as interval center and positive uniform scale, in
the same graph order as `Graph::sample_node`. No float sample is used to decide
inside/outside. Each interval ray point is `origin + direction × t`, with an
outward operation at each step. A qualified position encloses the ideal point;
it is not a single exact floating-point coordinate.

## Algorithm and decision rules

1. GR-2 broadphase returns certified outside, sorted candidate instance IDs,
   or indeterminate. A stale index queries fresh flat bounds. Any broadphase
   uncertainty stops the full query with `Indeterminate`.
2. For each candidate, the graph is represented by bounded analytic sphere or
   box occurrences. The research scene derives and refreshes those terms when
   an instance transform or definition changes. Each candidate term costs one
   narrowphase work unit; a full budget returns `Indeterminate(WorkBudget)`.
3. A sphere uses interval coefficients for
   `a = d·d`, `b = 2(o-c)·d`, `c = (o-c)·(o-c)-r²`, and `D = b²-4ac`.
   `D.upper < 0` proves no real sphere crossing. If `D.lower <= 0`, tangent
   and nearly tangent cases are indeterminate. With `D.lower > 0`, interval
   square root and division enclose entry/exit parameters. A hit needs a
   certified exterior start, an entry wholly within `(near, far)`, and entry
   upper strictly before exit lower. Clearly out-of-range root intervals are
   misses; uncertain boundary overlap remains indeterminate.
4. A box uses outward lower/upper face coordinates and interval slab times.
   Strict separation, disjoint slabs or a wholly out-of-range interval proves
   miss. A certified exterior start, interior entry parameter and strict
   entry-before-exit yield a hit. A ray parallel to a face at an uncertain
   boundary is indeterminate.
5. For an exterior-start hard union, the first **provably earlier** primitive
   entry is the first union entry. The winner's parameter upper must be below
   every other hit's lower. Overlapping intervals, any unknown primitive query,
   or a start inside any primitive make the union result indeterminate. This
   rule does not expose later internal surfaces. Candidate and result ordering
   are deterministic; numeric ambiguity is not resolved by arbitrary ID tie
   breaking.

For a sphere, the normal is returned as three intervals enclosing
`(ray_point - center) / radius` at the ideal surface; the exact ideal normal
has unit length. For a box, a strict unique entry face yields an exact axis
normal. Edge and vertex hits can be qualified while normal is **absent**.
Coincident boundaries or nearly equal first hits yield `Indeterminate` for
identity/normal. A caller must not replace an absent normal with a fabricated
one. This query's `Hit` carries parameter, position, instance identity,
occurrence ordinal and optional componentwise normal enclosure.
The prototype also stops if **any** candidate term is indeterminate, even
when an earlier known hit might be proved first with additional time-order
bounds. This conservative choice increases uncertainty; it is not a false
miss and is a concrete target for a later refinement.

## Independent checks and counterexamples

The GR-3 tests use exact axis oracles: a unit sphere at the origin is entered
from `(-3,0,0)` at `t=2` with normal `(-1,0,0)`; a translated unit box centered
at `(4,0,0)` is entered at `t=3`; direction `(2,0,0)` gives sphere parameter
`t=1`. Tests require those values to lie in the returned intervals, with
small measured interval width. A 1,024-case fixed-seed (`0x5eed_2027`)
integer-center corpus checks 1,024 hits and 1,024 misses against independent
centerline geometry. GR-2's 3,072 hit/miss pairs remain active. These finite
tests complement the interval argument; they are not exhaustive proof.

Other directed cases cover a sphere AABB candidate that is an analytic miss,
exact tangent and `1+1e-8` near-tangent, interior origin, box face/edge/vertex,
nonunit and tiny directions, a small scaled sphere, position and normal
enclosures, a sphere around coordinate 9000, separated and overlapping hard
unions, coincident and nearly coincident entries, nested transforms, near/far
boundary, numeric overflow, broadphase and narrowphase budgets, changing
definition and transform, stable instance/occurrence identity, stale flat
fallback and rebuild. Flat/hierarchical
full-query outcomes agreed for every measured ray, but their shared
narrowphase is **not** independent evidence by itself.

Counterexamples to broader claims:

- At sphere `y = 1 + 1e-8`, the binary64 ideal ray is outside, while an f32
  conversion yields exactly `1`. No GR-3 CPU exclusion transfers to WGSL.
- A tangent sphere discriminant encloses zero; calling it a proven hit or miss
  would overstate the floating-point evidence.
- Two coincident union entries, and two instances separated by `1e-15`, have
  overlapping first-hit intervals. Stable source IDs do not justify picking
  either surface/normal; the query returns `Indeterminate`.
- A box edge/vertex has a qualified entry parameter but no single geometric
  normal. An arbitrary axis normal would be a policy choice, not a theorem.

## Full CPU query performance

Windows 11 Pro, Ryzen 5 7500F, Rust 1.94.0 release profile; one validated
sphere Graph per instance, 1,024 identical rays per method, seven repetitions
within each of five independent process runs. All times below are the median
of those five per-run median **milliseconds per full batch**. Each full query
includes broadphase, analytic interval narrowphase and per-query timing
instrumentation. Independent medians of the timed broad/narrow components do
not necessarily sum to the full median. Graph compilation, initial instance
term construction and index build are outside query time. Full raw rows,
component times, counts and bytes are in `benchmark-series.jsonl`; each run's
original stdout is retained separately. A preliminary run flipped the relative
result for the densest case, motivating the five-run series.

| Scene | Flat full | Hierarchy full | Index build | Indeterminate / 1024 | Observation |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 sphere, sparse | 0.1443 | 0.1525 | 0.0001 | 0 | difference small relative to run variation |
| 8 separated | 0.1886 | 0.1709 | 0.0003 | 0 | modest gain |
| 32 separated | 0.4409 | 0.2312 | 0.0013 | 7 | hierarchy faster |
| 128 separated | 1.4698 | 0.5706 | 0.0109 | 40 | hierarchy faster |
| 32 partly overlapping | 0.4395 | 0.2086 | 0.0013 | 8 | hierarchy faster |
| 128 partly overlapping | 1.4618 | 0.3807 | 0.0110 | 54 | hierarchy faster |
| 32 coincident, mostly sparse rays | 0.4301 | 0.1934 | 0.0009 | 4 | whole-tree rejection helps |
| 128 coincident, mostly sparse rays | 1.4222 | 0.2932 | 0.0039 | 4 | whole-tree rejection helps |
| 32 coincident, dense rays | 8.2422 | 9.2264 | 0.0009 | 1024 | hierarchy slower in all five runs |
| 128 coincident, dense rays | 32.8421 | 36.8944 | 0.0039 | 1024 | hierarchy median slower; one flat run was noisy |
| 32 separated after move/rebuild | 0.4305 | 0.2255 | 0.0013 | 7 | stale hierarchy fallback median: 0.4451 ms |

The moved instance's replacement plus full rebuild had a five-run median of
0.0030 ms (range 0.0024–0.0034 ms); each run contained one such update. For 128
leaves the index stores 255 nodes (18,360 bytes), 8,192 bytes of leaf records,
and 14,336 bytes of precomputed analytic terms. These are lower bounds that
exclude vector capacity, allocator and `BTreeMap` overhead and source Graphs.
The broadphase and narrowphase both evaluate `Graph::sample` **zero** times;
the benchmark reports actual analytic term tests and uncertainty/fallback
counts. The dense coincident scene has 1,024 indeterminate first-hit identities,
not 1,024 qualified hits. This data does not establish production picking,
physics or GPU performance. A one-instance bypass looks sensible, but a
selection policy needs representative full application workloads.
For dense 128-instance overlap, the five-run flat range was 32.3701–45.6059
ms and hierarchy range 35.9035–38.9794 ms; this machine's workload noise
precludes a universal crossover claim.

## Acceptance and architectural decision

| Criterion | Result |
| --- | --- |
| AC-1 numerical contract | Met conditionally for finite stored numbers, the restricted grammar and correctly-rounded binary64 operations; not a platform-independent floating-point proof. |
| AC-2 ray hit | Met for certified exterior sphere/box cases with interval parameter/position; tangent, boundary and unstable cases remain explicit uncertainty. |
| AC-3 composition and normals | Met for uniquely earliest hard-union entries and smooth primitive/unique box face normals; ties and nonsmooth normals are not manufactured. |
| AC-4 broadphase integration | Met in the research scene: GR-2 candidate IDs, stale fallback, budgets and unsupported bounds cannot silently become a miss. |
| AC-5 reproducibility | Fixed fixtures, seed, tests, JSONL and commands are retained in source and local review package. |
| AC-6 full-query performance | Measured on one machine and one graph family, including negative dense-overlap result; no broader speed claim. |

**CONDITIONAL** for this test-only CPU narrowphase. **NO-GO** for a production
CPU query API, renderer integration, physics/collision authority and GPU
hierarchy from GR-3 alone. The next milestone should validate this restricted
interval contract against additional independent transformed-shape oracles
and actual `WorldState` revision snapshots, then test an API that preserves
hit/miss/uncertainty and optional normals. A separate GPU study must qualify
f32 operations and backend parity. Do not raise production budgets, hide
uncertainty with epsilon, or port this hierarchy to GPU as a side effect.

## Qualification limits

Windows fmt, strict Clippy, workspace build/tests, rustdoc and cargo-deny
advisories/licenses passed; 189 ordinary tests passed, zero failed and 91 were
ignored across 45 suites. The GR-3 release benchmark and affected GR-2
broadphase benchmark passed separately. WSL
Ubuntu fmt, strict Clippy and workspace tests passed. Site build and link
check are recorded in the review package. The remaining ignored tests, hosted
CI, formal verification of all floating-point states and any GR-3 GPU parity
were not run. No remote operation was performed.
