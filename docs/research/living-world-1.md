# Living World 1: bounded persistent ecosystem

**Status:** experimental local demonstration. Baseline `bd1ffd0` from
`efficiency-research`; work proceeds on local `living-world`. This report
records implemented rules and local measurements, not biological realism or
production hardware support.

## Question, inventory, and chosen architecture

**Hypothesis:** moving one finite resource source at tick 61 changes later
resource uptake and branch topology of two organisms through the same growth
and allocation laws used by the control world. A failure would be no later
organism difference, a difference only in the directly moved source, replay
divergence, or invalid resource accounting.

| Already implemented | Used here |
| --- | --- |
| `WorldState`, stable organism/node/source IDs, analytic growth nodes and capsules | The single authoritative mutable world; renderer scenes are derived. |
| Finite source stock, bounded replenishment, proportional contested allocation | Shared environmental feedback. |
| Typed tick events, fixed-step `Runtime`, atomic failure, version-7 save/load | The intervention, snapshots, and replay. |
| CPU analytic contact and GPU direct/BVH display | Existing physical and visual contracts remain separate. |

The needed extension was a scene-sized experiment, a native view, a bounded
initial-condition JSON parser, a completed-snapshot adapter for the existing
save format, and a one-line numerical correction to replenishment accounting.
No new world-state field, simulation rule, persistence version, ECS, renderer,
or compiler mode was added. General implicit collision, population dynamics,
and arbitrary authored laws remain outside this experiment.

The checked-in version-1 initial scene at `examples/first-light/scenes/living-world.json`
has one narrow, single-child strategy and one wider, two-child strategy. Both
consume two finite sources. The separate [scene contract](../specifications/living-world-scene.md)
defines validation and reload. The file specifies initial parameters, while
evolving nodes and stock live in `WorldState`. The GPU receives an analytic
snapshot of that state; it never determines growth or resource allocation.

## Implemented laws and accounting

Each 16,666,667 ns tick first applies ordered typed events, replenishes source
stock up to capacity, allocates available units once across organisms, samples
the resulting local field at eligible growth nodes, and grows nodes when their
maturity signal and shared organism budget reach the configured threshold.
Allocation order and node IDs are stable. A source's cumulative ledger obeys
`initial stored + cumulative actual replenishment - cumulative allocation =
current stored` within the documented floating-point tolerance. No source has
negative stock. A source moved out of reach keeps replenishing until its finite
capacity clips additional offers.

There are two linked feedbacks. Withdrawal by one organism lowers shared stock
available to the other now and on later ticks. Growth creates new sampling
positions; those tips then alter later uptake and growth direction. The source
move changes the field and thus both feedback paths without selecting a
winner. The models have no birth, mutation, death, or general physical contact
with visual Field Graphs. `max_children=1` and `max_children=2` are genuinely
different developmental strategies under the same simulation implementation.

The initial experiment exposed a pre-existing `f64` edge case: subtracting
`0.2` from `0.215` yielded `0.015000000000000013`, slightly above a declared
`0.015` refill. The recorded generated amount is now capped by the declared
per-tick offer. The existing balance tolerance handles the representational
difference. A focused regression covers this decimal case.

## Controlled two-world experiment

Both worlds start with seed 41 and identical version-7 save bytes. World A has
no event. World B schedules exactly one validated `MoveSource` for source ID 2,
to `(4.0, 1.5, 0.0)` at tick 61. Before the tick, the worlds still have equal
`WorldState`; only B's pending event differs. Both then run the same `Runtime`
tick code through tick 240. Both are saved and loaded at tick 120. A separate
fresh replay of each event sequence equals the resulting runtime exactly on
the tested Windows build.

| Tick | A nodes (IDs 0,1) | B nodes | A/B total allocated units | A/B branches on ID 1 |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 1, 1 | 1, 1 | 0 / 0 | 0 / 0 |
| 60, before event | 16, 6 | 16, 6 | 1.92 / 1.92 | 2 / 2 |
| 61, event tick | 16, 6 | 16, 6 | 1.947 / 1.932 | 2 / 2 |
| 90 | 18, 11 | 18, 8 | 2.73 / 2.28 | 4 / 3 |
| 120 | 19, 17 | 18, 10 | 3.54 / 2.64 | 8 / 4 |
| 240 | 19, 42 | 19, 21 | 6.78 / 4.08 | 20 / 10 |

The initial digest is `1d61cd530ed0f9e0` in both worlds. At tick 60 both
digests are `930b7ce72ab1f9ce`. Final digests are
`3a46d14b9bdef83f` (A) and `296f4ab12a43c0c3` (B). These are FNV-1a hashes
of stable JSON save bytes, not hashes of memory layout or a promise of
cross-platform bitwise equality. Stable organism IDs remain 0 and 1. At tick
240, B's moved source has 1.0 stored unit at its cap, while A consumes its
corresponding source completely. That unconsumed supply and the later
organism topology are consequences of the shared laws; moving the source
itself is the direct event effect.

An invalid out-of-range move is rejected without a partial state change.
Malformed external scenes fail before replacing the running world. The
checked-in parser rejects unsupported versions, extra keys, wrong counts,
equal strategies, invalid numeric parameters, and files above 64 KiB.

## Native execution and actual GPU images

`cargo run --release -p first-light --locked -- --living` starts a native
interactive window. Its HUD shows tick, two node counts, two source stocks,
pause/speed state, and recent action. Camera, pause, fixed step, fourfold time,
source move/toggle, analytical pruning, reset, F5 snapshot save, and F9 reload
use the existing app paths. An external scene can be loaded with
`--living-scene FILE` and revalidated on reset. The [player guide](../guides/living-world.md)
contains the controls. The native app was launched and advanced beyond tick
800 on a Vulkan RTX 4070 SUPER; the single automated window intervention was
logged as source ID 2 at tick 61. Human usability and every key path were not
manually playtested.

An opt-in GPU readback rendered eight actual 640×360 scenes: A and B at ticks
0, 60, 120, and 240. GPU direct traversal and GPU BVH output matched exactly
per RGB byte in each scene. A/B images matched before the event and differed
at 120 and 240. The t240 captures were visually inspected: the single stem,
branching body, and moved source are present. The BMPs are in the ignored
`target/living-world-1-review/` package. This comparison shares the existing
analytic shader and is not an independent mathematical intersection proof.

This demonstration intentionally uses analytical spheres and capsules derived
from organism and source state. It does not use Field Graph compilation;
therefore it neither proves nor refutes the exact, structural, or Combined
compiler's benefit for a future implicit organism. Those paths and their
interpreter fallback remain unchanged and covered by existing regressions.

## Local performance

Reference: Windows, Rust 1.94, wgpu 30.0.1, NVIDIA RTX 4070 SUPER, driver
591.86, Vulkan, release profile. CPU and GPU tests are single local runs.
The control world's 240 changing ticks had CPU tick p50/p95 **0.0048/0.0078
ms** and `Scene::from_world` p50/p95 **0.0041/0.0073 ms**. The intervention
world measured **0.0039/0.0049 ms** per tick and **0.0029/0.0041 ms** per
snapshot. One save-and-load at tick 120 took 0.140 ms in A and 0.030 ms in B;
these single observations do not establish a distribution. Final snapshots
held 122 and 80 analytic primitives respectively.

Offscreen GPU BVH compute at intervention tick 240 (80 primitives) used seven
samples after two warmups per resolution. These timestamps exclude CPU world
updates, readback, shader creation, and window presentation:

| Resolution | GPU p50/p95 | Draw-and-timestamp p50/p95 |
| --- | ---: | ---: |
| 1280×720 | 0.0860/0.0864 ms | 0.242/0.277 ms |
| 1920×1080 | 0.1115/0.1124 ms | 0.277/0.296 ms |
| 2560×1440 | 0.1733/0.1739 ms | 0.340/0.377 ms |

The actual 1080p window used `Fifo`. Thirteen two-second windows with the
automated tick-61 intervention recorded median frame intervals about
6.06–6.07 ms; the observed per-window p95 ranged 6.25–7.73 ms and per-window
p99 reached 8.80 ms. The early window ended at tick 84; later windows reached
tick 1525. Source acquisition alone commonly took about 5.4–5.7 ms. Only
9–12 GPU timestamps were sampled per window, and the mature window scene had
134 primitives with GPU compute around 1 ms p50. These intervals are real
presentation measurements on one machine, not a general 60 FPS guarantee.
The earlier no-intervention run is retained as a separate control log. A
pre-existing ecology fixture also measured one, two, and four organisms, but
its 1–6-node scenes are a scale reference, not a substitute for the living
world's changing workload. Source-count, update-frequency, second-GPU, and
weak-GPU scale sweeps remain unmeasured.

## Persistence, limitations, and architectural judgment

F5 uses `Runtime::from_snapshot` to validate and save the completed `WorldState`,
time, and fixed step without changing the version-7 file format. The native
app rejects saving an intervention queued for the next tick and never
overwrites an existing save. F9 loads only the last save from the current app
session; `--living-load FILE` works after restart. Applied key-event history is
not stored with the snapshot; the headless A/B fixture persists its typed
event sequence in its result and verifies a fresh replay. The authored scene's
source bytes and revision are likewise not embedded in a save. The scene
format supports numeric choices within fixed laws, not arbitrary developer
defined ecology.

The current `WorldState` naturally holds changing analytical organisms and
finite sources, and ordinary growth changes GPU primitive buffers/BVH data
without shader recompilation. A new Field Graph topology would still require
validated compilation; its parameters can reuse structural/Combined programs
when eligible, but that was measured in Performance Research 3–4 rather than
this workload. Existing limits (four organisms, 48 nodes each, four sources)
are adequate for this first small scene. A 10× ecosystem would require measured
budget changes, growth-allocation profiling, spatial work reduction, renderer
capacity review, and larger save qualification. A weaker-hardware tier is a
hypothesis until tested. Physical interaction with arbitrary implicit fields
remains unavailable; keep the analytical contact model specialized.

**Decision:** the central A/B question is answered **yes within the tested
bounds**: one player-equivalent intervention changed later allocation and
branch topology under shared laws, and authoritative state survived save/load
and deterministic replay. Product readiness is **partial**: native key-flow
playtesting, provenance of external scene files, broader hardware, longer
ecological dynamics, and input-event-history persistence remain open.

The smallest next milestone is a versioned, world-owned parameter/event
contract for a genuinely mutable implicit structure, with one bounded field
organism responding to the same finite resource ledger. Qualify CPU/GPU
identity and compiler fallback before extending ecosystem size or physics.
