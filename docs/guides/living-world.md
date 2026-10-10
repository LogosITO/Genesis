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
