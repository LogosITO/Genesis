# Field graph research preview

Two version-one JSON definitions produce different GPU images without changing Rust code:

```sh
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/twin.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/plinth.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/twin.json --watch
cargo run -p field-graph-preview --locked -- --world-fixture
cargo run -p field-graph-preview --locked -- --mixed-world-fixture
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/shared-face.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/balanced-16.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/smooth-bridge.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/hard-bridge.json
cargo run -p field-graph-preview --locked -- --inspect examples/field-graph/definitions/balanced-16.json
```

The preview is headless and requires a Vulkan or DirectX 12 GPU. It prints a JSON CPU/GPU center-ray summary and writes actual GPU readback as PPM under `target/field-graph-captures/`. In `--watch` mode, edit the same JSON file. A valid revision gets a new capture; invalid input leaves the last valid graph and capture intact. Stop with Ctrl-C. The PPM is RGB extracted from GPU RGBA8 pixels, not a generated illustration.

`--world-fixture` constructs two world-owned visual field instances and writes
`target/field-graph-captures/world-instances.ppm`. GPU ray queries produce the
hit, miss and uncertainty results; CPU code combines those results into RGB
pixels. The JSON summary gives stable IDs, outcome counts, adapter and full
query/readback time. This fixture does not change The Passage. The canonical
[field graph contract](../../docs/specifications/field-graph.md) states budgets
and numerical limits.

`--mixed-world-fixture` builds a `WorldState` with one analytic sphere, one
world-authored capsule and two instances sharing one field definition. It uses
the same camera rays for both GPU query paths, compares approximate world-unit
hit distances and writes `target/field-graph-captures/mixed-world.ppm`. Red
pixels mean an unresolved field query, yellow means exhausted tracing, and
magenta means the display-only depth tie band cannot order two hits. The JSON
summary reports source counts and full blocking time. This is a 256×256
offscreen reference, not the interactive Passage viewport.

`shared-face.json` joins adjacent boxes. Its origin is a zero sample on an
internal face, so it must not be treated as an external surface. `balanced-16.json`
uses all 32 permitted nodes and 47 expanded visits per sample; it provides a
repeatable 16-sphere display and cost fixture. The older `twin.json` exercises
overlap, while `plinth.json` mixes box and sphere branches. These fixtures are
validated by ordinary CPU tests. On the local RTX 4070 SUPER/Vulkan adapter,
their GPU captures displayed the expected joined box and 4×4 sphere grid.
The balanced scene's centre ray lies between spheres: the older preview reports
an ideal-model `Miss`, while the restricted solid query correctly reports
`Indeterminate(UncertifiedAdvance)` because its bounds overlap the ray.
`smooth-bridge.json` uses `smooth_union` to join two translated spheres with
an editable `width`; `hard-bridge.json` uses the same inputs and hard union.
Their GPU captures show the wider continuous smooth neck. The restricted physical query explicitly reports
`Indeterminate(UnsupportedOperation)` for this visual field.
`--inspect` compiles and validates a JSON definition without requesting a GPU;
it prints the revision, node/work counts, distance semantics and outward bounds
as JSON. Invalid input exits with an error before any preview is created.
