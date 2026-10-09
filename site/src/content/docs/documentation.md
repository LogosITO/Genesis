---
title: Documentation
template: splash
---

Run the vertical slice from the repository root:

```sh
cargo run -p hello-world --locked
```

The output is computed after four steps and a signed-distance query. Start with the [field contracts](../reference/specifications/mathematical-fields/), [distance bounds](../reference/specifications/distance-bounds/), [gradients](../reference/specifications/gradients/), [spatial queries](../reference/specifications/spatial-queries/), [world state](../reference/specifications/world-state/), and [simulation time](../reference/specifications/simulation-time/). API docs are generated from Rust source with `cargo doc --workspace --no-deps --locked`.

For the experimental GPU viewport, use `cargo run -p first-light --locked`; see the [First Light guide](../reference/architecture/first-light/) and [CPU/GPU contract](../reference/specifications/cpu-gpu-contract/). GPU tests are opt-in and require a compatible native adapter.

For external mathematical structure authoring, inspect the [bounded grammar format](../reference/specifications/mathematical-authoring/) and [two-file example](../reference/research/foundation-010/). The CPU inspector and native analytic preview read JSON without recompiling Rust.

For the first authoritative authored world instance, run `cargo run -p authoring-inspect --locked -- examples/authoring/world-single.json --world`. It verifies two world IDs sharing one definition, analytic picking and contact, typed updates, version-6 save/load, and replay. A native view is available with `cargo run -p first-light --locked -- --authored-world examples/authoring/world-single.json`; click a segment, press **E** to toggle it or **M** to move it. Read the [instance contract](../reference/specifications/authored-world-instances/).

For the local playable prototype, run `cargo run --release -p first-light --locked -- --passage`. The in-window HUD explains movement, camera, selection, source movement, pruning, and restart. The Windows packaging script creates a local portable ZIP with an offline playtest card and license files. Read the [player guide](../reference/guides/the-passage/), [manual QA checklist](../reference/guides/playtest-qa/), and [local research record](../reference/research/first-playable/). This remains an experimental prototype, not a public release.

For the experimental growth system, run `cargo run -p first-life --locked -- baseline`, `pruning`, or `pruning-replay`. Each prints a machine-readable summary. Use `cargo run -p first-light --locked -- --life` for the native viewport: left-click a branch, press **P** to prune it at the next tick, and use **Space** to pause or resume. See the [interaction contract](../reference/specifications/first-interaction/), [growth model](../reference/specifications/growth-model/), [persistence](../reference/specifications/world-persistence/), and [replay](../reference/specifications/replay-determinism/) specifications.

For first kinematic contact, run `cargo run -p first-life --locked -- contact`, `contact-pruned`, or `contact-replay`. In the native First Life viewport, hold **I/J/K/L** to move the orange body toward a branch, prune it, then move through the cleared space. The [contact contract](../reference/specifications/kinematic-contact/) defines supported solids, sweep mathematics, and limits.

For finite shared resources, run `cargo run -p first-life --locked -- ecology competition`, `isolated`, `separated`, `environment-change`, `pruning`, or `replay`. The existing `--life` viewport now shows two differently colored organisms, one finite source, and their node counts and source balance in the title. Read the [allocation contract](../reference/specifications/resource-allocation/) and [ecosystem state](../reference/specifications/ecosystem-state/). The stage remains an experimental toy model.

Foundation 0.5 adds bounded branching and analytic capsule connections. Run `cargo run -p first-life --locked -- scale --measure` for a local CPU scaling sample. The [capsule contract](../reference/specifications/capsule/) and [First Structure measurements](../reference/research/first-structure-measurements/) record limits and evidence. The world can exceed the GPU snapshot budget; overflow is reported explicitly.
