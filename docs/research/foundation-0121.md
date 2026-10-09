# Foundation 0.12.1: field solids and spatial reliability

Historical record. Foundation 0.12.2 supersedes the `IdealModelMiss` outcome with
explicit uncertainty and adds a narrow outward-rounded bounds exclusion; see the
current [field graph contract](../specifications/field-graph.md).

## Status, hypothesis, environment

**Experimental finding on one machine, 2026-10-10.** A bounded union-only graph can
support a useful exterior entry query without calling every zero point a surface.
The local run used Windows x64, an NVIDIA GeForce RTX 4070 SUPER, Vulkan, and a Rust
debug build for field GPU measurements. CPU timing experiments used release builds.
These tests establish neither a certified `f64`/`f32` error bound nor portability.

## Method and independently checked cases

The occupied solid is the ideal `F≤0` set. Two closed unit-half-extent boxes at
`x=-1` and `x=1` occupy `[-2,2]×[-1,1]²`; their shared face gives `F(0)=0` while
an open radius-0.5 ball around the origin remains occupied. The CPU test checks the
exact zero sample, negative samples on either side, and first entry at `x=-2`.
Actual GPU readback sampled seven points from `x=-2.01` to `2.01`, including the
internal zero; its centre sample was `Some(0.0)`. Away from zero by over 0.001,
CPU/GPU signs agreed; measured values agreed within 0.0001 world units in this set.
Six further samples covered a translated, uniformly scaled sphere on both sides of
its boundary and at its centre, also within 0.0001. An interior shared-face ray
returned GPU uncertainty rather than a physical hit. There were zero
sample-classification mismatches in the seven box points. This is a
finite regression, not a universal bound.

Independent quadratic roots or box slabs checked exterior entry for one sphere,
one box, translated/scaled spheres, disjoint/overlapping/coincident/nested sphere
unions, an adjacent-box union, a miss, and an interior start. A near-tangent case
allows an explicit `Exhausted` rather than accepting a false `Miss`. Existing graph
validation covers graph depth, repeated references, work count, invalid scales and
nonfinite input. The new spatial test checks region estimates and a deliberately
insufficient work budget. Region estimates remain ideal-model calculations only.

### Restricted moving sphere investigation

For a union of exact sphere SDFs `F=min_i(d_i)`, an exterior sphere of radius `r`
intersects the solid when `F(p)-r≤0`; algebraically this is the union of spheres with
radii `R_i+r`. A CPU research test compares 0.8-safe field steps against the existing
authoritative analytic `ContactScene::sweep` for one sphere, two separated spheres,
and two overlapping spheres. The earliest fractions agreed within `1e-4` in all
three non-tangent exterior cases. The field test returns no normal, identity,
overlap classification, or certified contact time. It has no interior-start or
tangent guarantee. **Decision:** keep field-graph collision disabled in `WorldState`;
the existing sphere/capsule contact remains authoritative.

## GPU execution and timing

The fixed WGSL interpreter now has a diagnostic sample entry point. An optional
compute-pass timestamp is read separately from CPU preparation and the blocking
readback wait. The latter includes GPU completion, so its value must not be added
to compute time. Each controlled graph was sampled 12 times at 320×240, with 512
query rays, a balanced union, and the indicated iteration budget. Below are median
and p95 **milliseconds** from one debug-profile run; no warmup or confidence interval
was claimed. GPU timestamp was supported on this adapter. Node depth and shared
subexpressions are encoded in the generated scenes; 32 nodes reach 47 expanded
visits per field sample. The old whole-query figures are retained in test output,
but they include transfer and synchronization.

| Nodes / visits | Budget | GPU compute p50 / p95 | CPU prepare p50 / p95 | Readback wait p50 / p95 | Full frame p50 / p95 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 2 / 2 | 32 | 0.434 / 1.026 | 0.173 / 11.100 | 0.634 / 1.296 | 1.160 / 19.763 |
| 2 / 2 | 128 | 0.436 / 0.933 | 0.178 / 0.213 | 0.618 / 1.108 | 1.061 / 1.627 |
| 8 / 11 | 32 | 0.852 / 1.313 | 0.200 / 0.327 | 1.043 / 1.507 | 1.624 / 2.022 |
| 8 / 11 | 128 | 1.045 / 1.537 | 0.180 / 0.191 | 1.300 / 1.762 | 1.731 / 2.231 |
| 16 / 23 | 32 | 1.880 / 2.187 | 0.192 / 0.385 | 2.250 / 2.892 | 2.712 / 3.447 |
| 16 / 23 | 128 | 2.597 / 2.858 | 0.219 / 0.786 | 2.798 / 3.249 | 3.472 / 4.170 |
| 32 / 47 | 32 | 3.297 / 3.480 | 0.186 / 0.214 | 3.519 / 3.745 | 4.042 / 4.244 |
| 32 / 47 | 128 | 3.952 / 4.337 | 0.181 / 0.261 | 4.177 / 4.546 | 4.671 / 4.980 |

Larger expanded graphs cost more on this workload; the first row's p95 preparation
spike demonstrates runtime noise. CPU preparation includes graph buffer creation and
upload, not an isolated upload measurement. This run does not separate GPU query
compute from query readback, vary resolution or spatial complexity independently, or
cover other adapters. The timestamp measures the render compute pass only.

## Opt-in audit and limits

The seven CPU timing tests previously left unrun were identified and executed
individually in release mode: two `first-light` authored reload/scale tests, one
`world-authoring` phase test, `authoring-inspect` world measurements, `first-life`
interaction timing, and two `world-simulation` allocation/contact timings. **7/7
passed.** They are performance experiments with assertions, not a substitute for
the ordinary correctness suite. The prior report's 24 other opt-in passes were from
an earlier run. This milestone also ran the entire current ignored suite separately:
**32/32 passed**, including the seven CPU experiments and four field-graph GPU tests.
The seven CPU timing tests were additionally run individually in release mode; the
whole-suite pass used the default debug profile.

The crucial numerical debt is an error bound for sampling and advancement across
validated graph transforms in both `f64` and `f32`. Fixed padding is not a proof.
Before authoritative use, evaluate outward-rounded intervals or validated error
tracking, near/far endpoint rules, grazing contact, normal/identity selection, and
multi-adapter behaviour. Do not treat an `IdealModelMiss` or GPU `Miss` as a
certified real-world collision miss.
