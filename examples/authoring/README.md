# Genesis mathematical authoring example

Experimental / Research Stage. These JSON files define static structural grammars, not living organisms or scalar fields.

From the repository root, inspect a definition without a GPU:

```sh
cargo run --release -p authoring-inspect --locked -- examples/authoring/branch-a.json
cargo run --release -p authoring-inspect --locked -- examples/authoring/branch-b.json
```

Display either definition using the existing analytic GPU renderer:

```sh
cargo run --release -p first-light --locked -- --authoring examples/authoring/branch-a.json
```

The preview prints a deterministic JSON topology summary including the exact-byte SHA-256 revision and compiler semantics version, opens a native window, and draws one analytic capsule per `F` segment. The CPU inspector prints the same stable fields; add `--measure` to include one wall-clock file-read and compilation duration for local diagnostics. Use A/D or left/right arrows to orbit, W/S or up/down arrows to tilt, left-click to select a static segment, **R** to reload the file, and Escape to exit. Editing `iterations`, a rule, step length, yaw, pitch, radius, or origin needs no Rust recompilation. An unchanged file is a no-op; a failed reload retains the last valid structure; a changed valid file clears selection.

To exercise an authoritative world occurrence without a GPU, run:

```sh
cargo run -p authoring-inspect --locked -- examples/authoring/world-single.json --world
```

This creates two occurrences sharing one exact definition, picks a segment, confirms analytic contact, applies tick-indexed move and disable events, saves and reloads version-6 world state, then checks replay. Output is deterministic JSON. To view an occurrence in the native renderer, run:

```sh
cargo run -p first-light --locked -- --authored-world examples/authoring/world-single.json
```

Click a segment to select it, press **E** to enable/disable, or **M** to move it one world unit along +X through a typed tick event. The native path requires a compatible GPU and is separate from the headless save/replay check. The `--authoring` command above remains a static reloadable preview.

Keep a copy of the exact JSON bytes or use the `world-authoring` API's self-contained `snapshot_json()`/`load_snapshot_json()` pair for experiments you need to reproduce. `revision` is an author label; `content_sha256` identifies exact bytes. Version-6 world saves embed those bytes once per revision. See [the format](../../docs/specifications/mathematical-authoring.md) and [world instance contract](../../docs/specifications/authored-world-instances.md).
