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

The preview prints a deterministic JSON topology summary, opens a native window, and draws one analytic capsule per `F` segment. The CPU inspector prints the same stable fields; add `--measure` to include one wall-clock file-read and compilation duration for local diagnostics. Use A/D or left/right arrows to orbit, W/S or up/down arrows to tilt, **R** to reload the file, and Escape to exit. Editing `iterations`, a rule, step length, yaw, pitch, radius, or origin needs no Rust recompilation. A failed reload prints an error and retains the last valid structure.

Keep a copy of the exact JSON bytes for any experiment you need to reproduce. `revision` is an author label, not a content hash; the preview does not add these static structures to `WorldState` or to its save format. See [the format and grammar contract](../../docs/specifications/mathematical-authoring.md).
