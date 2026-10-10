# Mathematical World Lab scene format

World Lab is an experimental standalone GPU viewer, not an authoritative
editor. Run `cargo run -p world-lab --locked` for the included scene, or pass
a JSON path. The scene file is versioned with `format_version: 1` and limited
to 64 KiB. The full example is at `examples/world-lab/scenes/showcase.json`.

`definitions` contains unique names, paths to existing Field Graph JSON files
inside the scene directory, and optional `revision` SHA-256 strings. The
revision covers the **exact graph file bytes**. The Lab computes it through
`DefinitionRevision`; if provided, a mismatch rejects the scene. Graphs use
the canonical [Field Graph format](field-graph.md), including its node, depth,
source-size, and numerical restrictions. Maximum active scene contents are
four definitions, eight field instances, 64 ordinary spheres, and 72 material
definitions. The renderer and adapter may impose tighter limits.

`materials` entries have unique `name`, linear RGB `base` and `alternate`
triples, an `emission` triple, and a `pattern` object. Pattern kinds are
`constant`, `gradient`, and `bands`. The latter two require unit `axis`,
`frequency`, `phase`, and `speed`; the [material contract](materials.md)
defines their ranges and world-space evaluation. `spheres` and `fields`
reference a material by name. Spheres have radius, translation, and positive
uniform scale. Fields reference a graph definition, translation, positive
uniform scale, and optional `enabled` (default true). These create a fresh
`WorldState` with stable IDs for that scene generation; IDs need not remain
stable across reordering or reloads. JSON unknown fields are rejected.

`camera` gives target, orbit distance, yaw, elevation, and vertical
`fov_radians`. `background` is linear RGB in `[0,1]`; `light_direction` is a
finite nonzero world-space vector. Camera, background, light, visibility, and
materials are presentation settings; geometry and instance state live in
`WorldState`. The Lab evaluates materials at simulation time zero. It does
not run simulation or author dynamic time in this slice.
Optional `render` flags are `show_analytic`, `show_fields` (both default true),
`diagnostic`, and `normals` (both default false). The same flags can be toggled
in the window; successful reload resets them to the scene values.

Press **R** to reload. Parsing, graph compilation, world construction, and
CPU snapshot validation finish before replacing the active scene. On an open
GPU, the candidate also receives an offscreen preparation draw. Synchronous
preparation failure retains the old scene. GPU execution errors can still
arrive asynchronously; this is not a cross-device hardware transaction.
The HUD and stderr report rejected reloads. Camera orbit resets to the
scene's settings on successful reload. **C** writes a BMP of the actual mixed
GPU image to the current directory. For reproducible one-frame capture on an
available desktop GPU, set `GENESIS_WORLD_LAB_CAPTURE_ONCE` to an output BMP
path and run the same command; the Lab exits after the captured frame.

Numerical diagnostic colors expose uncertain field tracing. Normal display
is an approximate GPU `f32` visualization, not certified occupancy or
implicit collision. No mesh or voxel scene representation is introduced.
