# Genesis Architecture Correction — AC-2

**Status:** bounded local acceptance of dynamic `Auto` on the tested Vulkan
adapter; broader backend and long-run reliability remain conditional.
**Starting state:** clean `living-world` at
`1cc4017629fd7ab82ed33bac61dd6f452ea96d49`.
**Dynamic Auto implementation commit:** `2ff0943b65d18412b6f1b9fd33c7b7d67e42b172`.
**Bounded high-update measurement commit:**
`61bb7cd74768bf0e9152de9eea7916f39433fa08`.
The report commit cannot contain its own hash; use `git log -2` for the final
local history. `main` and `efficiency-research` were not changed. There was no
push, merge, rebase, tag, release, deployment or GitHub settings change.

## Evidence, method and scope

This report distinguishes **measured** frame and readback results, **tested**
invariants, **source-level** conclusions, and **unverified** claims. Prior
[QA-0](architecture-qualification-qa0.md),
[QA-1](architecture-qualification-qa1.md),
[QA-2](architecture-qualification-qa2.md),
[AC-1](architecture-correction-ac1.md), Performance Research 1–4 and Living
World 1–4 were used as historical evidence, not assumed to apply unchanged.
AC-1's prepared scene remains the native frame path; AC-2 did not add a scene
cache or repeat that correction.

The authoritative fixture is the existing ignored `target/qa2/dense-world.json`,
SHA-256 `2a354e779afa51936a483fc884327e7b34d68356bbf10996f1541559ca0fa9d4`.
It validates with 512 ordinary spheres, four 48-node organisms, four sources,
eight authored occurrences with 4,096 segments, four Field Graph definitions,
eight Field Graph occurrences, one body and 4,993 analytical primitives.
All six native runs used its exact bytes, one fixed tick per recorded dynamic
frame, the same deterministic source moves and camera path, 64 warmup frames,
then 1,000 measured frames. All ended at tick 1,064 and had identical serialized
final `WorldState` (summary SHA-256
`a36e0f634703b65361e029c63dd05d3e86eefde9f145f0ccc89004d68fbf3491`).
All had 36 total BVH rebuilds and 140 field-node uploads. The normal Fifo
surface was used. GPU timestamps were sparse compute samples, not frame
intervals. CPU enqueue timing is not transfer-completion timing.

Environment: Windows 11, Ryzen 5 7500F, 32 GiB RAM, NVIDIA RTX 4070 SUPER,
driver 591.86, wgpu 30.0.1/Vulkan, Rust 1.94.0. This is one physical GPU.
Raw baseline and candidate JSONL plus checks are ignored under `target/ac2/`;
the compact summary and selected actual GPU captures are in ignored
`target/ac2-review/`. Separate processes reuse whatever driver shader cache
already exists. No global cache was deleted; first process does **not** mean
cold driver cache. These are one-run observations, not a cross-hardware
performance guarantee.

## Architecture decision and implementation

Before AC-2, `GpuRenderer::compiled_pipeline` rejected every `Auto` request if
any occurrence had morphology or runtime parameters. This was a conservative
stale-constant guard, not a compiler inability. Exact shaders inline numeric
constants and are keyed by immutable definition revision. Structural and
Combined shaders instead load current `GpuNode.param` values from the resident
field buffer. `WorldState::field_graph` exposes an occurrence's current runtime
graph; the buffer packer uploads changed nodes and assigns deterministic
topology selectors. `structural_key` contains the root and ordered node links
and rejects nonfinite or nonpositive packed parameters. Combined keys contain
the sorted, distinct structural keys plus compiler semantics version.

The small policy change routes `Auto` to the **existing Combined selection and
worker** when an **enabled** occurrence has morphology or runtime parameters.
Otherwise it preserves exact-revision selection. A single dynamic topology
uses the existing storage-backed structural shader through the Combined key;
several supported topologies use Combined dispatch. There is no second
eligibility implementation. More than four distinct topologies, invalid
packed parameters, incompatible immutable definitions, absent/failed/pending
programs and no enabled fields use the complete Interpreter. The renderer now
reports the last fallback reason separately from compiler errors. A pending
request never activates a pipeline with a different key. Cache entries and the
one worker remain renderer/device-local; the four-entry limit is unchanged.

The shader emits no numeric graph constants on this storage-backed path:
sphere radius, box extents, translation, scale and smooth-union width are read
through `field_nodes`. Current instance transform, material and world ID are
packed separately. `WorldState` remains authoritative. A source edit on an
occurrence with active runtime parameters is rejected by the world contract;
it cannot silently repurpose those channels for a different authored revision.
An immutable occurrence can be reloaded independently, changing the Combined
topology key when necessary. Invalid source reloads remain atomic.

This policy is **accepted for the current bounded grammar on the tested Vulkan
GPU**, and remains research-stage for other backends and hardware. Static
heterogeneous worlds are still interpreted under `Auto` when their immutable
revisions differ; changing that policy was outside the demonstrated dynamic
gain. Explicit Structural and Combined remain available for experiments.

## Matched native performance

Values are milliseconds. Interval p50/p95/p99 uses all 1,000 presented frames;
GPU p50/p95 uses the stated number of sparse timestamp samples. The baseline
`dynamic` binary was built before the change and stayed Interpreter. The
candidate `dynamic` binary uses the new `Auto`. Explicit Combined is the same
compiled implementation used as a control. Each 720p/1080p row is one process,
run sequentially. Fifo presentation, compositor scheduling and driver cache
were not controlled between processes.

| Size | Mode | Active measured frames | Interval p50/p95/p99 | >16.7 / >33.3 ms | GPU compute p50/p95 (n) |
| --- | --- | ---: | ---: | ---: | ---: |
| 720p | prior Auto / Interpreter | 1,000 Interpreter | 9.079 / 13.735 / 34.558 | 39 / 20 | 8.342 / 11.079 (41) |
| 720p | explicit Combined | 1,000 Combined | 6.064 / 7.151 / 10.169 | 0 / 0 | 0.829 / 1.477 (36) |
| 720p | new Auto | 1,000 Combined | 6.058 / 7.630 / 10.674 | 4 / 0 | 0.754 / 1.475 (37) |
| 1080p | prior Auto / Interpreter | 1,000 Interpreter | 16.367 / 48.537 / 78.329 | 483 / 99 | 18.926 / 44.443 (49) |
| 1080p | explicit Combined | 1,000 Combined | 6.059 / 7.569 / 10.190 | 0 / 0 | 1.239 / 1.787 (37) |
| 1080p | new Auto | 1,000 Combined | 6.061 / 7.743 / 10.436 | 0 / 0 | 1.335 / 1.850 (37) |

Each compiled run had one fallback warmup frame, selected Combined from frame
1, made one worker request and reported zero compilation failures. The selected
warm-cache worker pipeline time was 3.37–4.19 ms and source generation
0.08–0.39 ms. This does not bound cold driver compilation or frame-thread
interference on another system. The worst measured new-Auto interval was
30.404 ms at 720p and 13.415 ms at 1080p. The prior Interpreter 1080p p95 is
substantially worse than AC-1's earlier matched run, so the exact p95 ratio
must not be treated as stable. The explicit Combined control and new Auto
showed similar medians and compute times in this run. Static `Auto` takes the
unchanged exact/Interpreter branch; the ordinary and opt-in suites passed, but
no new 1,000-frame static A/B was run.

Measured median 1080p CPU snapshot preparation in the candidate was 0.651 ms,
field binding 0.135 ms and simulation step 0.606 ms. These are small relative
to the old Interpreter GPU cost. A persistent scene or field cache would add
many invalidation paths without a measured need. In the candidate 1080p run,
33 measured source moves rebuilt the BVH. Their build/flatten times ranged
3.66–6.87 ms (mean 4.41 ms), but their presented intervals were 5.74–6.59 ms;
the 967 other frames ranged 1.90–13.42 ms. This is correlation only, not a
proof that BVH work cannot affect other workloads. Full rebuild remains.

To test a different update rate without changing normal gameplay, the QA-2
profile now accepts `GENESIS_QA2_SOURCE_MOVE_INTERVAL` from 1 to 30; default
30 is preserved and zero is rejected. With interval 1, two additional matched
1080p runs moved the same source every tick for 1,000 measured frames.
Auto and explicit Combined each used Combined on all measured frames, made one
compile request, rebuilt the BVH 1,064 times total, uploaded field nodes 137
times and ended with the same serialized world SHA-256
`499ec35b85adf53c48fcc320d047b9a9bb2fd5341625ebc22ed454000a18839c`.
This state differs from the 30-tick schedule, as expected. Auto interval
p50/p95/p99 was 5.965/7.851/8.855 ms (maximum 10.457); explicit Combined was
5.964/7.548/8.329 ms (maximum 10.917). No measured interval exceeded 16.7
ms. Auto's BVH build p50/p95 was 3.94/5.13 ms and analytical binding
p50/p95 was 4.04/5.27 ms. Sparse GPU compute p50/p95 was 1.335/2.072 ms
(36 samples). Thus every-tick rebuilding is a real ~4 ms CPU cost, but did
not dominate presented frame tails on this reference machine. That is not a
guarantee for a slower CPU, more expensive scene or different presentation
mode; a safe refit still needs a measured target beyond this fixture.

A separate current-code 600-frame World Lab showcase profile exposed a
**different, existing** exact-revision churn risk. `Auto` selected a compiled
program for 349 frames, interpreted/compiled for 251 and made five requests
during rapid/slow authored revision phases. Explicit Structural on the same
showcase selected Structural for 539 frames, interpreted/compiled for 61 and
made one request. `Auto` chose exact specialization there because the graphs
were immutable, so this is outside the new dynamic-parameter branch. All
these Fifo runs had interval p50 about 6.06 ms; surface pacing masks shader
differences. The heterogeneous showcase control kept immutable `Auto` on
Interpreter for 600/600 frames and explicit Combined on 578/600 after 22
fallback frames. These were independent processes and not a pixel or final
world-state comparison. Changing static `Auto` policy to Structural could
help revision-heavy editing, but needs its own matched correctness and GPU
cost study. The five requests did not justify a new worker scheduler or cache
limit change in AC-2.

## Correctness and compiler lifecycle

The real dense diagnostic test used 289 rays at ticks 0, 30 and 60. At each
checkpoint 87 rays hit Field Graphs. Interpreter, active explicit Combined and
active Auto agreed on hit/field/uncertain/exhausted/ambiguous flags, selected
instance identity, authored revision and material. Interpreter versus Combined
depth differed by less than 0.02 world units and each normal component by
less than 0.04, preserving existing tolerances. Auto and explicit Combined
matched exactly in this readback. Auto's initial pending call used Interpreter;
the diagnostic reason was `compilation pending`, and one request became active.

A separate actual readback checked two instances sharing one source with
different current sphere radii. Changing only the right instance caused an
additional node upload and changed only its selected depth; Auto used the
current values with the same authored revision. An authoritative box half-Z
extent changed from 0.3 to 0.4 through the resource parameter API; the
Interpreter and Auto agreed on identity, revision, depth and normal. A
three-topology A→B→C→A sequence kept Interpreter while an obsolete program was
pending, selected the current key after completion, and reused the cached A
program on return without another worker request. Existing exact-revision
eviction and invalid-reload tests passed in the full opt-in suite. These tests
observe uploaded effects and upload counts; they do **not** independently
read raw GPU storage bytes. Interpreter and compiled paths share the ray
tracer, so agreement is differential GPU evidence, not a proof of the ideal
field equation. Historical CPU/f64 oracles remain independent but limited to
their specified domains.

The cache still holds four success/error entries and one worker. An obsolete
finished program may be cached but is never selected for a different current
key. A failed key remains a cached failure until eviction, preventing
per-frame retries for that key; alternating more than four failed keys could
still cause recompilation. Shader validation failure injection and actual
device-loss recovery were not exercised in AC-2. The new fallback reason
distinguishes pending, unsupported graph, incompatible definitions, no fields,
cached compilation failure and unavailable worker. It is a last-draw
diagnostic, not an authoritative world result.

## Numerical, backend and architectural findings

All ordinary and opt-in numerical regressions passed, including the previous
guard counterexample, inside starts, tangencies, mixed depth and spatial
oracles. The visual `bounds_culled` flag remains an empirical early skip;
it cannot certify a mathematical miss or preserve the result of a tracer that
was not run. Diagnostic readback sets the mode that bypasses this culling.
The previously observed 496 hidden `Exhausted` outcomes remain evidence of
suppressed tracer status, not 496 proven missing surfaces. No epsilon was
widened and no field gained physical collision authority. AC-2 changed no
numerical guard or world law.

The Windows default DX12/FXC control was rerun separately and failed with
exit 101 and `X3511: forced to unroll loop, but unrolling failed` at translated
analytical shader positions 111 and 465. Vulkan passed. QA-2's installed-SDK
DXC control previously passed on this same physical NVIDIA adapter; AC-2 did
not rerun that control or change driver/compiler deployment. A narrow shader
fix was not established, so DX12 FXC remains blocked. Linux WSL ordinary
workspace tests passed with one build job and a separate target; Linux native
GPU was not run.

Shader generation still has text-fragment coupling in Combined. The current
test corpus validates generated WGSL and compares outputs, but a shared
operation edit could be missed in one mode. Rewriting the emitter during this
policy change would enlarge the regression surface without a demonstrated
defect. The cache count and source-size limit remain unchanged. No general
worker scheduler, persistent scene cache or BVH refit was added.

## Local qualification

All commands below were actually run on this change unless qualified:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo build --workspace --locked` | Pass |
| `cargo test --workspace --locked` | 176 passed, 0 failed, 83 ignored |
| `cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture` | 83 passed, 0 failed |
| `cargo doc --workspace --no-deps --locked` | Pass |
| `cargo deny check advisories licenses` | Pass |
| Release dense readback and focused A→B→C→A / parameter GPU readback | Pass |
| High-update 1080p native Auto/Combined, 1,000 measured frames each | Pass; equal final world, 1,064 rebuilds per run |
| Profile default/invalid interval control | Default 30 passed; interval 0 rejected before rendering |
| Living World 1–4 CLI experiments and LW4 replay | Exit 0 each |
| WSL `cargo test --workspace --locked` (single job) | 176 passed, 0 failed, 83 ignored |
| DX12 default FXC targeted diagnostic | **Fail**, expected `X3511`; preserved as blocker |
| `cd site; npm run build; npm run check-links` | Pass: 208 pages, 21,336 local links |

The normal and opt-in totals above were rerun after the A→B→C→A, box extent
and measurement changes. Historical v7–v10 save tests, replay, GPU BVH,
World Lab external reload and The Passage automated tests are in the executed
workspace suites. The four Living World CLI commands and the LW4 history
replay also exited 0. The site build reported the existing nonfatal sitemap
warning because `site` is not configured in Astro; it did not break the
static build or local links. Hosted GitHub CI, manual
playtesting, a second physical GPU and Linux native GPU were not run.

## Decisions and next work

| Item | Decision | Evidence / limit |
| --- | --- | --- |
| Dynamic `Auto` through existing Combined path | **ACCEPT, bounded** | Matched 720p/1080p gain, dense and mutable-parameter readback, current-key lifecycle. Limited to validated graphs on one Vulkan GPU. |
| Interpreter fallback and reason | **ACCEPT for tested transitions** | Pending and obsolete-key fallback passed GPU tests. No-field/unsupported reasons follow the source branches; failure injection is still open. |
| Persistent CPU scene/field cache | **REJECT for now** | Prepared scene already shared; measured remaining CPU costs do not justify invalidation complexity. |
| BVH refit | **REJECT for now** | Even 1,000 consecutive measured rebuild frames stayed below 16.7 ms here, despite ~4 ms CPU work each. Lower-tier CPU headroom is untested. |
| Static exact-revision Auto churn | **CONDITIONAL** | World Lab measured five requests and 251 fallback frames versus Structural's one request and 61 fallback frames; static policy unchanged pending qualified A/B. |
| Certified visual guard | **NO-GO** | No rigorous f32 enclosure; diagnostic mode bypasses culling. |
| FXC shader rewrite | **INCONCLUSIVE** | Reproduced `X3511`; no qualified source-level correction. |

Highest-value next tasks, in order: (1) qualify a structural policy for
revision-heavy immutable editing, including identity/readback and first-use
latency; (2) deterministic shader-failure and device-recreation tests plus raw
parameter-buffer readback; (3) repeat native profiles on a second hardware
tier, including high-frequency BVH updates and DX12 compiler qualification.
Do not expand world
limits, add operators, build a generic job system or grant visual fields
physical authority from the present evidence.

## Reproduction

```powershell
$env:GENESIS_WINDOW_SIZE = '1920x1080'
$env:GENESIS_QA2_FRAMES = '1000'
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json dynamic target/ac2/new-auto-1080.jsonl
$env:GENESIS_QA2_SOURCE_MOVE_INTERVAL = '1' # bounded every-tick BVH stress
cargo run --release -p first-light --locked -- --qa2-profile target/qa2/dense-world.json dynamic target/ac2/high-update-auto-1080.jsonl
cargo test --release -p first-light --test qa1_valid_world --locked dense_dynamic_combined_diagnostic_parity -- --ignored --nocapture
cargo test --release -p analytic-renderer --test mixed_gpu --locked native_mixed_diagnostic_selects_nearest_mathematical_object -- --ignored --nocapture
cargo test --workspace --locked -- --ignored --test-threads=1 --nocapture
```

Regenerate the ignored dense fixture from
`dense_authored_world_reaches_combined_limits` with `GENESIS_QA2_FIXTURE_OUT`
as described in [AC-1](architecture-correction-ac1.md). `target/ac2-review/README.md`
contains the exact local evidence inventory and capture hashes.
