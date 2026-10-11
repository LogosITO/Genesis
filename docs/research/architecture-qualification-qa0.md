# Genesis Architecture Qualification — QA-0

**Status:** local qualification, 2026-10-11. **Source baseline:**
`living-world` at `2cd2913effb178832d7a6f9a0b82a7dd319af1c0`.
The audit received on this date was treated as hypotheses. No production
architecture, world limit, renderer policy or save format was changed.

## Evidence rules and environment

- **Measured** means a timed run recorded in `target/qa0/` or the retained
  [LW4 evidence](evidence/living-world-4/README.md). Timings apply only to
  that workload and machine.
- **Tested** means a named executable assertion passed. Passing differential
  GPU tests does not prove an independent mathematical oracle.
- **Code fact** means a property established by the cited source lines.
  **Hypothesis/risk** means its effect has not been demonstrated.
- The starting working tree had a 19-line uncommitted replacement of the
  206-line committed LW4 report. It was copied to ignored
  `target/qa0/living-world-4-short-working-copy.md`; the canonical report was
  restored from `2cd2913`, checked against retained raw data and annotated.

Windows 11 Pro 10.0.26200, AMD Ryzen 5 7500F (6 cores/12 threads), 32 GiB
RAM, RTX 4070 SUPER driver 591.86, Vulkan; Rust/Cargo 1.94.0, cargo-deny
0.20.2, Clippy 0.1.94, rustfmt 1.8.0, Node 24.14.0, npm 11.9.0,
Astro 7.3.8 and Starlight 0.42.6. Linux build and ordinary tests ran in
WSL Ubuntu 24.04 with Rust 1.94.0. WSL initially lacked the four native
packages already installed by CI (`pkg-config`, `libwayland-dev`,
`libxkbcommon-dev`, `libx11-dev`); these were installed locally. The first
parallel Linux test compile failed with `Cannot allocate memory`; a serial,
non-incremental run with `CARGO_TARGET_DIR=/tmp/genesis-qa0-target` passed.
That first failure is an environment result, not a failed test assertion.

## Qualification results

| Platform / command | Result | Scope |
| --- | --- | --- |
| Windows `cargo fmt --all -- --check` | PASS | Workspace formatting. |
| Windows `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS | All targets. |
| Windows `cargo build --workspace --locked` | PASS | Debug build. |
| Windows `cargo test --workspace --locked` | PASS | Final run: 174 passed, 0 failed, 67 ignored including two new QA-0 probes. |
| Windows `cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture` | PASS | All 65 existing opt-in tests passed, including native GPU tests. |
| Windows release `qa0_cpu_scaling` opt-in test | PASS | 2 new measured probes passed; timings below. |
| Windows `cargo doc --workspace --no-deps --locked` | PASS | Rustdoc. |
| Windows `cargo deny check advisories licenses` | PASS | Advisories and licenses. |
| `cd site && npm run build && npm run check-links` | PASS | Final run: 200 pages and 19,718 local links; existing nonfatal 404/i18n/sitemap warnings. |
| Linux `cargo build --workspace --locked` | PASS | Native WSL build after installing CI packages. |
| Linux `cargo test --workspace --locked` with one build job and incremental off | PASS | 174 passed, 0 failed, 67 ignored including two new QA-0 probes. |
| Linux `cargo fmt --all -- --check`, strict Clippy and rustdoc | PASS | Serial WSL checks, all targets for Clippy. |

The complete Windows opt-in set included GPU BVH edge and real-scene parity,
field boundary diagnostics, native mixed identity/depth, compiler revision
and structural reuse, evolving-world readbacks, World Lab external edit, and
the LW4 40,960-pair comparison. This is broad regression coverage within the
supported workloads, not a proof for arbitrary implicit fields. Linux opt-in
GPU tests were not run: WSL is not an independent physical GPU/backend test.

Separate release CLI runs of `--living-experiment`,
`--living-implicit-experiment`, `--living-morph-experiment` and
`--living-rules-experiment` all exited successfully and reported
`replay_equal: true`. The LW4 rerun's normalized complete JSON matched the
retained `experiment.json` exactly. The retained v10 history bundle replayed
to tick 240 with digest `9103ee2194ffe042` and `equal: true`. Separate
`world-runtime --test field_save` ran 4/4 tests; `world-lab` ran 4/4 normal
tests, with its GPU test already passing in the opt-in run. Runtime tests
exercise v7–v10 feature gates, round trips, rejected downgrades, tampering
and continuation. **They do not use frozen historical save fixtures generated
by each old release**, so cross-version compatibility evidence is narrower
than the phrase “migration tested” suggests.

The source manifest's 10/10 hashes matched the baseline checkout. Ten
original 640×360 BMPs were present in ignored local evidence. Control and
intervention hashes matched at ticks 0/60, differed at 61/120/240; the
tick-240 pair was visually inspected. A fresh GPU run regenerated ten BMPs
with 10/10 hashes identical to the originals on the same adapter/backend.
The BMPs are not tracked, so a separate machine must rerun the capture test
to inspect their pixels.

## Actual dependency map

`cargo metadata --no-deps --locked --format-version 1` gives these direct
local dependencies:

| Crate | Depends on |
| --- | --- |
| `spatial-math` | none |
| `analytic-field` | math |
| `world-authoring` | field, math |
| `world-state` | authoring, field, math |
| `world-simulation` | world, field, math |
| `world-runtime` | simulation, world, math |
| `analytic-renderer` | simulation, world, authoring, field, math |

The `first-light` application depends on all seven library crates;
`world-lab` depends on renderer, world, authoring, field and math.
`world-state` and
`world-simulation` do not depend on `wgpu` or renderer. `WorldState` owns
definitions, revisions, mutable parameters and rules; renderer scenes, BVH,
buffers and compiled WGSL are derived. `world-runtime` owns persistence and
scheduled events. The site imports Markdown from `docs/`; it is not a second
hand-maintained technical source.

## Performance baseline, not a gate

The new ignored release-profile probe is
`crates/renderer/tests/qa0_cpu_scaling.rs`. It uses 3 warmups and 11 samples
for `Scene::push`/`Bvh::build`, 21 samples for world validation/snapshot.
Primitives are equal-radius spheres placed on one line; the 2,048/4,096
rows are standalone renderer `Scene`s, **not valid `WorldState`s** under the
current 512-sphere cap. No background-load or allocator-noise control was
attempted. All values below are milliseconds, median/p95 on the Windows CPU.

| Primitives | `Scene::push` total | `Bvh::build` |
| ---: | ---: | ---: |
| 128 | 0.0047 / 0.0049 | 0.0234 / 0.0387 |
| 512 | 0.0584 / 0.1177 | 0.1363 / 0.1459 |
| 2,048 | 1.1135 / 1.6366 | 0.9183 / 1.0786 |
| 4,096 | 7.3221 / 9.1636 | 1.9272 / 2.4858 |

| World spheres | `WorldState::validate` | `Scene::from_world` including validation/push |
| ---: | ---: | ---: |
| 128 | 0.0035 / 0.0046 | 0.0098 / 0.0129 |
| 512 | 0.0117 / 0.0248 | 0.0718 / 0.0817 |

The exact loop in `Scene::push` gives quadratic comparisons for unique IDs.
These measurements confirm a real scaling cost but **do not show a present
frame-rate defect at the 512-sphere world cap**. The current benchmark does
not isolate primitive construction, dynamic organism snapshot generation,
GPU upload or cache lookup in a maximum combined world. Those remain QA-1
measurements. The existing release `finite_allocation_and_growth_commit_benchmark`
reported allocation-demand/solver median/p95 of 0.0003/0.0004,
0.0004/0.0005 and 0.0006/0.0007 ms for 1/2/4 organisms. It uses single-root
worlds and does **not** measure 48-node parent lookup.

The fresh release LW4 GPU run used one dynamically changing two-definition
world, seven samples after two warmups. GPU compute timestamps in ms:

| Resolution | Interpreter p50/p95 | Explicit Combined p50/p95 | Combined offscreen frame p50/p95 |
| --- | ---: | ---: | ---: |
| 1280×720 | 8.722 / 9.080 | 0.158 / 0.159 | 0.376 / 0.473 |
| 1920×1080 | 19.303 / 19.447 | 0.311 / 0.312 | 0.551 / 0.647 |
| 2560×1440 | 31.020 / 33.570 | 0.544 / 0.545 | 0.840 / 1.118 |

One Combined compilation request and changing node uploads were recorded.
These are explicit-mode offscreen measurements; default `Auto` may use the
interpreter for dynamic instances. They exclude window presentation, VSync,
cold pipeline cost and other adapters. The original LW4 table and the fresh
table differ slightly; no performance regression or improvement is claimed
from seven unpaired samples. Raw six-record JSONL is retained with LW4
evidence.

## Audit finding matrix

Priority means the next qualification priority, not a claim of production
severity. “Confirmed” refers only to the stated code fact. Each row gives
an observable completion criterion rather than a proposed rewrite.

| ID / priority | Source and verdict | Class, consequence and correction criterion |
| --- | --- | --- |
| Q01 / P1 | `crates/world/src/life.rs:8-20`, `crates/world/src/field.rs:12-19`, `crates/authoring/src/field_graph.rs:19-28`, `crates/runtime/src/persistence.rs:12-14`. Caps are confirmed. | **Capability limit**, not a defect. Current world cannot demonstrate large-scale claims. Before increasing caps, profile 1×/4×/16× CPU/GPU/memory/save/replay with explicit rejection behavior. |
| Q02 / P2 | `crates/renderer/src/gpu.rs:1754-1765` calls `Scene::from_world` each mixed draw; `crates/renderer/src/lib.rs:423-483` validates/builds it; `gpu.rs:1351-1389` validates again and repacks fields. Confirmed. | **Repeated work**. Current pure-sphere 512 snapshot p50 0.0718 ms; full mixed-world cost not isolated. Instrument validate/snapshot/pack/upload at maximum valid world before any revision cache. Cached writes at `gpu.rs:1579-1636` do not remove CPU preparation. |
| Q03 / P2 | `crates/renderer/src/lib.rs:535-546` scans all IDs on every `Scene::push`. Confirmed O(N²) comparisons. | **Measured algorithmic inefficiency**: 4,096 standalone pushes p50 7.3221 ms. Current world cap limits exposure. Replace only with equivalent duplicate-ID and allocation errors; accept when scaling is near-linear and direct/BVH parity still passes. |
| Q04 / P2 | `crates/simulation/src/life.rs:39-65,384-393,416-425` searches prior nodes for each parent. Confirmed nested scans. | **Algorithmic risk**, not measured at 48 nodes. Run matched deterministic worlds at 1/24/48 nodes and profile `allocate_finite` plus growth commit; accept an index or single-pass child count only with identical IDs, allocations and replay. |
| Q05 / P2 | `examples/first-light/src/living_rules.rs:63-92` requires two fields and a base scene; `crates/authoring/src/field_graph.rs:33-42,65-95` limits mutable components and graph ops. Confirmed. | **Scope limit** of the example/DSL, not broken general authoring. Independent developer must author two genuinely different supported forms without Rust edits; record blockers before adding syntax. |
| Q06 / P2 | `crates/world/src/field.rs:444-458` stores three optional growth variants; `crates/simulation/src/life.rs:68-150,163-217` allocates them separately. Confirmed. | **Architecture duplication** with future maintenance cost, no demonstrated incorrect allocation. Specify one resource/parameter contract and add frozen v7–v10 compatibility fixtures before consolidation. |
| Q07 / P2 | `crates/renderer/src/gpu.rs:284-311,713-775` makes `Auto` default but declines exact compilation for dynamic morphology/parameters. Confirmed, intentional stale-value guard. | **Policy/performance risk**, not a current correctness bug. Tests must prove fallback identity/depth and no stale revision for every mode; report selected path and cold/warm frame tails before enabling dynamic compilation in `Auto`. |
| Q08 / P2 | `crates/renderer/src/gpu.rs:510-585,681-816` uses revision/topology keys, four resident pipelines and one pending worker. Cache tests and native revision tests passed. | **Churn hypothesis**: no stale cache defect reproduced. Run rapid alternating revisions beyond four entries, record misses/compile latency/fallback duration and require correct output for every frame. |
| Q09 / P1 | `crates/renderer/src/gpu.rs:1456-1486` adds an empirical f32 bounds margin; `crates/renderer/tests/field_graph_gpu.rs:89-215,645-708` and mixed oracle cover selected cases. Confirmed. | **Numerical correctness risk**: finite test coverage is not a conservative proof for all rays. Build independent f64 oracles over supported extreme scale/translation/tangency and require hit or explicit uncertainty, never a silent false miss. |
| Q10 / P1 | `crates/world/src/field.rs:444-446` defines visual-only FieldInstances; `crates/simulation/src/contact.rs:94-165` builds contact from supported analytic world objects. Confirmed. | **Functional gap**, not a regression: implicit fields cannot be gameplay collision authority. Define separate visual-ray and physical-contact capability contracts before implementing one provably supported implicit class. |
| Q11 / P2 | `examples/first-light/src/living_rules.rs:357-544` compares 40,960 GPU pixel pairs; `crates/renderer/src/gpu.rs:199-241` exposes identity/status. Confirmed; two paths share tracer. | **Evidence limit**. Retained one-pixel sphere/box oracle and GPU comparison passed, but broad mixed-field correctness remains unproven. Record hit/ambiguous distributions and independent expected distance/normal for difficult rays. |
| Q12 / P3 | Workspace manifests and `crates/renderer/src/gpu.rs:2062-2089` show Rust/wgpu viewport integration, no stable C ABI or Unity/Unreal plugin. Confirmed. `crates/renderer/src/material.rs:5-80` is basic color/Lambert shading. | **Future product gap**, not QA-0 failure. Defer SDK, plugin and PBR work until core semantics and portability are qualified. |
| Q13 / P2 | `crates/renderer/src/field_compiler.rs:60-91` composes WGSL through markers and string replacement; large mixed-mode files remain. Confirmed. | **Maintenance risk**, no failing compiled test. Add generator-marker and shader validation regressions before extracting responsibilities; no size-only rewrite. |
| Q14 / P1 | `docs/research/living-world-4.md` working copy was 19 lines vs 206 in HEAD; `.github/workflows/ci.yml:1-6` triggers PR/main, not local branch. Confirmed. | **Proven evidence/documentation defect**: canonical report restored and raw text retained; hosted CI remains unrun on this branch. Accept external CI only after an authorized push/PR, not as a local PASS. |
| Q15 / P2 | `crates/runtime/src/persistence.rs:50-177` selects/validates v7–v10; `crates/runtime/tests/field_save.rs:13-185` passes round trip, tamper and downgrade tests. No frozen historical save files are present. | **Compatibility evidence gap**, not a known load defect. Preserve representative bytes from each released schema and prove load/continue/save without semantic drift. |

The external audit's qualitative maturity warnings are consistent with these
limits. Its proposed performance impact and broad correctness conclusions
cannot be promoted to facts from static inspection alone. No P0 defect was
reproduced in QA-0. P1 items are next-stage correctness/evidence gates or
major capability limits, not current test failures.

## Risk register and next decision

1. **Numerical false-negative risk (P1):** independent field-ray oracle is
   narrow; f32 padding is heuristic. Gate future renderer claims on no silent
   false miss over a defined supported domain.
2. **Evidence portability (P1):** original BMPs and full test logs are local;
   DX12, second physical GPU and hosted CI remain unverified. Regenerate
   captures from documented commands and run an external CI/adapter matrix
   before a public correctness claim.
3. **Scale ceiling (P1):** world/graph/save caps are explicit. A larger-world
   claim requires controlled 1×/4×/16× resource and failure measurements.
4. **Behavioral model duplication (P2):** growth variants and versioned saves
   need compatibility fixtures before any unification.
5. **Frame CPU work (P2):** repeat validation/packing and quadratic push are
   real; prioritize by whole-frame measurements at current and proposed caps.

**GO:** continue a bounded QA-1 qualification of numerical ray contracts,
maximum-valid-world CPU phases, compiler selection under churn, and historical
save fixtures. Small measured fixes are acceptable only after their baseline
and invariants are recorded.

**NO-GO:** claims of a scalable arbitrary mathematical world engine, universal
implicit physics, cross-platform bit-identical replay, production renderer,
60 FPS guarantee, Unity/Unreal graphics plugin or broad GPU correctness.

Do not start a large refactor, increase caps blindly, add new organisms or
field operators, begin a showcase, enable compiled `Auto` for dynamic fields,
or publish a release on the strength of this QA-0 report. These would obscure
the remaining correctness and compatibility questions.

## Reproduction and outstanding checks

Run from the repository root with Rust 1.94.0; native GPU tests require a
compatible adapter. Logs from this qualification are in ignored
`target/qa0/` on the qualification machine.

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture
cargo doc --workspace --no-deps --locked
cargo deny check advisories licenses
cargo test --release -p analytic-renderer --test qa0_cpu_scaling --locked -- --ignored --nocapture --test-threads=1
cargo test --release -p first-light --locked two_topologies_native_gpu_resolution_measurement -- --ignored --nocapture --test-threads=1
cargo run --release -p first-light --locked -- --living-rules-experiment
cargo run --release -p first-light --locked -- --living-morph-replay docs/research/evidence/living-world-4/history-bundle.json
cd site && npm run build && npm run check-links
```

Linux `cargo test` used `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0` and
`CARGO_TARGET_DIR=/tmp/genesis-qa0-target` after the first WSL memory error.
No native Linux GPU, DX12, second physical adapter, hosted GitHub Actions,
manual playthrough or long-duration stability run was claimed.
