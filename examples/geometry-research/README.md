# GR-5 geometry research preview

This is a research-owned, data-driven hierarchy. It expands validated sphere
and capsule definitions into the existing analytic renderer. The GPU receives a
materialized primitive list; the optional GPU BVH is derived acceleration. No
hierarchy is traversed lazily in WGSL, and no geometry here enters `WorldState`,
persistence, collision, or replay.

From the repository root:

```sh
cargo test -p geometry-research --locked
cargo run --release -p geometry-research --locked -- examples/geometry-research/definitions/branching.json target/gr5-review/branching
cargo run --release -p geometry-research --locked -- examples/geometry-research/definitions/arcade.json target/gr5-review/arcade
```

Add `--detail` for a closer camera. Each run writes actual 640 × 480 GPU PPM
readbacks for Direct/BVH and base/grown states, a JSONL measurement file, and
diagnostic flat JSON dumps. The `grown` state uses a deterministic uniform scale
of 1.3. The program rejects direct/BVH pixel mismatch, missing CPU ray identity
parity, or an unchanged GPU image after growth. GPU timestamps require adapter
support; absent timestamps are recorded as an empty sample list.

The source format is version 1 JSON. `definitions` hold named analytic spheres
or capsules. `nodes` contain `leaf`, `group`, or `repeat` operations. Child
placements apply translation, XY-plane rotation (`angle_deg` about +Z), then
positive uniform scale in the parent coordinate system. The root is a node ID.
Definitions can be reused by many leaves, and nodes can be shared by a DAG.
Cycles, dangling references, nonfinite inputs, and excess work are rejected.
The limits are 16 KiB source, 64 nodes, 64 definitions, depth 12, 16 group
children, 32 repetitions per node, 2,048 expanded visits and 512 leaves.
The derived primitive buffer is therefore at most 32 KiB before renderer
allocation overhead. Actual device limits are checked by the renderer.

Derived feature IDs are depth-first ordinals. They remain stable when only the
global scale changes; structural edits can renumber them. Bounds are finite
axis extrema of the materialized analytic primitives in computed `f64`, not a
rounding-certified spatial exclusion. A capsule or sphere has an exact ideal
primitive SDF, but the hard union of all leaves is not generally an exact SDF.
The prototype does not return qualified field misses or physics contacts.

The architecture and measured limitations are in
[`docs/research/geometry-research-gr5.md`](../../docs/research/geometry-research-gr5.md).
