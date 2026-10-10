# Living World 1 — local research demonstration

Living World 1 is a small deterministic ecosystem, not a biological model. A
single-stem organism and a branching organism grow from finite resource sources.
They consume shared stock, so each affects the other's later supply. Moving one
source changes future uptake through the same rules used in the control world.

## Launch

From the repository root, with Rust 1.94 and a Vulkan or DirectX 12 adapter:

```text
cargo run --release -p first-light --locked -- --living
```

The checked-in scene at `examples/first-light/scenes/living-world.json` is
the initial configuration. To edit its bounded strategy and source parameters,
copy it and run `cargo run --release -p first-light --locked -- --living-scene
PATH`. Press **R** to validate and reload that file atomically. An invalid edit
leaves the current world running. The [scene contract](../specifications/living-world-scene.md)
lists its limits.

## Controls

| Key | Action |
| --- | --- |
| Arrows or WASD | Look around the world. |
| Space | Pause or resume fixed-step simulation. |
| Period (`.`) | Advance one fixed step while paused. |
| F | Toggle fourfold simulated time, with bounded per-frame catch-up. |
| M | Queue a move of the first finite source on the next tick. |
| O | Queue activation/deactivation of the first source. |
| Left click, then P | Select and prune a supported analytical branch. |
| R | Reset to the same initial state, or validate and reload the external scene. |
| F5 | Save the completed world snapshot to `target/living-world-saves/tick-N.json`. |
| F9 | Reload the last save made in this app session. |
| Esc | Exit. |

F5 never overwrites a file. Advance a pending intervention before saving. To
load a save after restarting the app, run:

```text
cargo run --release -p first-light --locked -- --living-load target/living-world-saves/tick-N.json
```

Saves use the existing bounded version-7 `Runtime` format and retain stable
organism, source, and node identities. They capture completed state; the native
app does not save an independent history of already applied keyboard events.
The headless A/B experiment below replays its typed event from the same start.

## Reproduce the two-world result

```text
cargo run --release -p first-light --locked -- --living-experiment
```

The JSON contains equal initial states, checkpoints through tick 240, a single
source move at tick 61, resource allocation, growth topology, stable IDs,
save-byte digests, and a fresh replay equality result. No GPU is needed for
this experiment. `--living-measure` measures CPU tick, renderer snapshot, and
one save/load operation locally; timing is hardware specific.

The GPU displays analytical spheres and capsules derived from `WorldState`.
The resource influence field guides CPU growth; it is not an implicit physical
collider. The present demonstration does not exercise the experimental Field
Graph compiler. See the [research report](../research/living-world-1.md) for
actual captures, performance measurements, and limitations.

## Living World 2: implicit organism

Run `cargo run --release -p first-light --locked -- --living-implicit` to see
the two analytical organisms and a two-lobed implicit field competing for the
same finite sources. The implicit field's uniform scale grows from acquired
resources. Its HUD shows tick, analytical node counts, source stocks, scale,
total acquired units, and compiler status. The controls above apply, including
M to move a source, R to reset, F5 to save and F9 to reload. After restart,
`--living-load PATH` recognizes the v8 dynamic-field snapshot and restores
the mixed view. Wait for the HUD's Structural path; interpreter fallback keeps
the scene visible while compilation is pending.

For a deterministic, GPU-free A/B experiment run
`cargo run --release -p first-light --locked -- --living-implicit-experiment`.
The JSON contains the exact initial v8 snapshot, tick-61 event, checkpoints,
and replay result. `--living-implicit-measure` measures local CPU stages.
See the [Living World 2 research record](../research/living-world-2.md) for
GPU captures and limits. The field is visual and is not a physical collider.

## Living World 3: surface-coupled morphology

Run `cargo run --release -p first-light --locked -- --living-morph` from the
repository root. Two independent radii change with local resource allocation;
their outer sampling sites move with the mathematical surface. A second
implicit field retains the earlier scale-growth strategy. The common controls
above apply. Press **F6** after a completed tick to export a bounded initial
snapshot plus applied-event history to
`target/living-world-bundles/tick-N.json`. Replay it with:

```text
cargo run --release -p first-light --locked -- --living-morph-replay target/living-world-bundles/tick-N.json
```

F5 still writes a completed v9 world snapshot; F9 loads that state without
replaying already applied inputs. Loading a completed save ends native history
collection until reset. F6 never overwrites an existing file. To inspect the
same deterministic A/B rule without a GPU, use `--living-morph-experiment`;
`--living-morph-measure` reports local CPU timing. See the
[Living World 3 research record](../research/living-world-3.md) for results,
numerical limits, and the tested GPU path.

## Living World 4: authoring two evolving mathematical forms

Run `cargo run --release -p first-light --locked -- --living-rules` for the
checked-in scene or `--living-rules-experiment` for the GPU-free fixed A/B and
ablation suite. The scene file is
`examples/first-light/scenes/living-world-4.json`. Its two sibling Field Graphs
are `morph-lobes.json` and `canopy-box.json`. Copy all three files plus
`living-world.json` into one directory before editing, then run:

```text
cargo run --release -p first-light --locked -- --living-rules-scene PATH
```

The bounded scene schema is `format_version: 1`, one `base_scene` file name,
and exactly two `fields`. Each field has a sibling `definition` file name,
world `position`, positive uniform `scale`, and one to four `rules`. A rule
names an authored graph node and component, then sets `uptake`,
`gain_per_unit`, `maximum`, `positive_site`, and `feedback`. For example:

```json
{
  "binding": { "node": "body", "component": "box_half_x" },
  "uptake": 1.1,
  "gain_per_unit": 0.26,
  "maximum": 1.1,
  "positive_site": true,
  "feedback": true
}
```

Supported components are `sphere_radius` and `box_half_x`, `box_half_y`,
`box_half_z`. They bind to a sphere radius or one box half extent in the
validated graph. The authored starting component must be within `[0.1, 2]`;
`maximum` must be at least that value and no greater than 2. `uptake` is
`[0, 100]`, `gain_per_unit` is `[0.001, 10]`. Each source requests
`source.sample(site) × uptake × fixed_step_seconds`; headroom limits demand
before the existing shared finite-stock allocator distributes it. Allocated
units increase the authoritative parameter up to `maximum`. The sample site
is the chosen positive or negative primitive-axis feature point, transformed
through a unique graph path and then the occurrence transform. `feedback:
false` holds that site at its authored initial location while growth proceeds.
An internal union feature is not guaranteed to lie on the final exposed
surface; this is a proximity sampling rule, not a physical contact model.

Only plain sibling file names are accepted, with no directory escape. File
limits are 64 KiB for the scene and 16 KiB per graph. Invalid JSON, missing
files, unsupported node/component pairs, duplicate channels and invalid
numbers reject the entire reload. Press **R** to reload an external scene;
the running world remains intact if validation fails. **M** moves the first
finite resource source on the next tick. **F5** writes a completed v10 state,
**F9** reloads the last save in this session, and **F6** exports the initial
state plus applied events for replay with `--living-morph-replay PATH`. A
completed save after restart loads with `--living-load PATH`. These operations
use the same `WorldState` and simulation as the earlier Living World stages.
The history command and bundle `kind` retain their Living World 3 names for
compatibility; their loader also validates v10 parameter worlds.
See the [Living World 4 research record](../research/living-world-4.md) for
measured results and limits.
