# Field graph research preview

Two version-one JSON definitions produce different GPU images without changing Rust code:

```sh
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/twin.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/plinth.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/twin.json --watch
cargo run -p field-graph-preview --locked -- --world-fixture
```

The preview is headless and requires a Vulkan or DirectX 12 GPU. It prints a JSON CPU/GPU center-ray summary and writes actual GPU readback as PPM under `target/field-graph-captures/`. In `--watch` mode, edit the same JSON file. A valid revision gets a new capture; invalid input leaves the last valid graph and capture intact. Stop with Ctrl-C. The PPM is RGB extracted from GPU RGBA8 pixels, not a generated illustration.

`--world-fixture` constructs two world-owned visual field instances and writes
`target/field-graph-captures/world-instances.ppm`. GPU ray queries produce the
hit, miss and uncertainty results; CPU code combines those results into RGB
pixels. The JSON summary gives stable IDs, outcome counts, adapter and full
query/readback time. This fixture does not change The Passage. The canonical
[field graph contract](../../docs/specifications/field-graph.md) states budgets
and numerical limits.
