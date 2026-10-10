# Living World 3: bounded mathematical morphogenesis

**Status:** experimental local research on `living-world`, starting at
`02aabdc`. No remote integration or manual playtest is claimed.

## Baseline and decision

Living World 2 owns one growing Field Graph scale in `WorldState`. The same
finite resource allocator serves analytical organisms and this visual field.
Its spatial shape changes, but resource demand samples the field instance's
fixed position, so the new surface does not affect future uptake. The source
bytes and definition revision are immutable; GPU programs are derived.

This experiment adds a deliberately narrow second form: the hard union of two
independent spheres from `morph-lobes.json`. Stable lobe indices 0 and 1 are
the runtime parameter identities. Each lobe has a current and initial radius,
uptake coefficient, radius gained per allocated unit, maximum radius, and
allocation counters. The world validates each value and derives a graph with
the same topology and exact source revision. This is **not** a general mutable
Field Graph parameter system. Unsupported topologies and parameter edits are
rejected; the source bytes are never rewritten by growth. The other implicit
organism keeps Living World 2's uniform-scale strategy.

For lobe *i* and source *s*, the request each fixed tick is
`source.sample(outer_site_i) × uptake_i × step_seconds`. The outer site is
computed from the current sphere center, radius, and world transform. The
ablation uses the initial radius for sampling while allowing the same growth
law to change the visible radius. Each row is capped by radius headroom;
analytical organisms, the old growing field, and the two lobe rows enter the
same deterministic proportional allocator. Each source is debited once from
its available stock. Allocated units update the world-owned radii and derived
bounds. `source.sample` is a bounded proximity signal, not physical contact.
The implicit field still has no collision authority.

This choice used existing source and allocation contracts. A general scalar
proximity evaluator, arbitrary parameter dictionary, incremental bounds
refitter, and new resource ledger were rejected for this bounded question.

## A/B and causal evidence

The two worlds start with the same definitions, IDs, seed, parameters, sources,
and fixed step. The intervention moves one source at tick 61. They are equal
through tick 60. At tick 240, control radii are `[0.98689854, 1.10647765]`
and intervention radii are `[0.71365153, 0.85559140]`. Analytical node
counts are `[19, 16]` and `[19, 9]`; the second implicit form ends at scales
`0.35873478` and `0.65`. The source move directly changes local samples;
subsequent allocation changes both radii, which moves future sample sites.
The shared ledger then changes allocations available to neighbors. These are
observations for one scene, not evidence of general biological emergence.

With geometry following disabled, the intervention's radii are
`[0.72606183, 0.84669957]` and analytical nodes `[19, 8]` at tick 240.
Thus feedback adds a smaller but measurable effect beyond the direct source
move. A no-intervention control and mid-run save/load replay matched. Earlier
Living World 1 and 2 A/B digests remained unchanged in local reruns.

## Persistence, event history, and native use

Morphology requires v9; old v7 and v8 worlds remain loadable and retain their
previous save versions. Exact authored graph bytes and SHA-256 revision remain
in each completed snapshot. Derived graph and bounds are rebuilt on load.
Invalid values, incompatible topology, and mislabeled older versions fail
validation. The headless experiment includes its initial state and event.

Native `--living-morph` supports pause, fixed stepping, fast time, camera,
source intervention, reset, F5 completed save, F9 load, and F6 history export.
F6 creates `target/living-world-bundles/tick-N.json` without overwriting an
existing file. `--living-morph-replay FILE` validates and replays the initial
snapshot plus applied events, then compares the canonical completed state.
The bundle is capped at 512 KiB, 4,096 events and 10,000 ticks. F9 loads a
completed snapshot; it does not reapply old events or resume their history.
The on-screen HUD includes radii, stocks, time, and compiler path. A native
1080p automated window run passed beyond tick 1,600; key use was tested in
code, not manually playtested.

## Independent numerical qualification

The oracle independently intersects the two spheres analytically, then
compares selected hits and distances with CPU Field Graph tracing, direct GPU
field query, analytical CPU/GPU geometry, and mixed GPU source selection.
It uses actual world states at ticks 0, 60, 61, 120, 240, and 2,000. On the
tested Vulkan adapter, 69,061 unambiguous rays were checked, including 1,305
field and 794 analytical hits. Fifty-nine threshold cases were classified as
ambiguous. One near-miss with discriminant `0.0001965752` is classified as a
GPU hit within its `0.001` threshold: the oracle excludes it from certified
agreement instead of widening the general distance tolerance. No GPU
uncertain/exhausted result occurred in this selected corpus. This is an
independent reference only for the supported union-of-two-spheres shape;
shared GPU ray stepping is not independent evidence. RGB comparison does not
prove per-pixel material, depth, normal, or identity equivalence of compiled
paths for arbitrary graphs.

Eight actual 640×360 GPU BMPs compare interpreter and Combined rendering at
ticks 0, 60, 120, and 240 for both worlds. Maximum RGB drift was one channel
unit; the pre-intervention pairs were equal and later pairs differed. One
compilation request was observed despite changing radii and repeated node
uploads. Dynamic bounds were recomputed. Exact `Auto` retains interpreter
fallback for this runtime parameter layout; Structural/Combined use current
node data. No stale program may be keyed only by a shared definition revision
when occurrences have different runtime values.

## Local performance

NVIDIA RTX 4070 SUPER, Vulkan, driver 591.86, release profile. Offscreen GPU
timestamps are seven samples after two warmups, matched tick-240 world and
camera. Values below are compute milliseconds, **not** full frame times.

| Resolution | Interpreter p50 / p95 | Combined p50 / p95 |
| --- | ---: | ---: |
| 1280×720 | 9.4405 / 9.9224 | 0.1550 / 0.1552 |
| 1920×1080 | 20.6786 / 23.0707 | 0.3349 / 0.3354 |
| 2560×1440 | 34.9007 / 36.4348 | 0.5847 / 0.8031 |

The intervention CPU tick was 0.0052 / 0.0085 ms p50 / p95 with feedback,
and 0.0049 / 0.0085 ms in the ablation. These small differences are below a
credible isolated cost attribution. The full native render and Fifo waiting
are separate from these GPU timestamps. No second GPU or DX12 measurement was
made. The ignored `target/living-world-3-review/` directory holds the local
logs and captures; it is not a published artifact.

## Limits and next questions

- The dynamic schema supports exactly one hard union of two spheres; general
  graph parameters, smooth union and moving centers remain research work.
- Surface-following sites are point samples, not integrated surface uptake or
  physical collision. Feedback in this scene is measurable but modest.
- The numerical oracle does not establish a complete proof for arbitrary
  implicit fields; near-threshold classifications remain conditional.
- Renderer device loss, a second adapter, direct compiled identity/depth
  readback, and human assessment of controls remain unverified.
- Further work should first generalize a validated parameter schema and
  produce independent per-pixel source/depth readback before claiming broader
  mathematical correctness.

Reproduce the CPU experiment with
`cargo run --release -p first-light --locked -- --living-morph-experiment`;
run the native view with `--living-morph`. GPU qualification tests are opt-in
because they require a compatible adapter.
