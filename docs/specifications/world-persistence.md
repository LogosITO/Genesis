# World persistence version 1

## Decision

`world-runtime` saves a JSON envelope with `format_version: 1` and the runtime state. The state includes the fixed step, completed tick, deterministic seed, world ID allocator, sphere entities, organism parameters and every node/parent/energy value, resource sources, pending typed events, and the next event order. The format is intentionally inspectable. It is versioned but not promised as a stable interchange format.

Input is limited to 65,536 bytes before parsing. Unknown fields, malformed JSON, unsupported versions, nonfinite or invalid numeric state, duplicate or missing world IDs, invalid node relationships, exhausted capacities, and invalid pending events are rejected. Loading constructs a complete candidate, validates it, then returns or replaces state; a failed `load_into` leaves the current runtime untouched. Exact `f64` JSON round trips use Serde JSON's `float_roundtrip` parser feature.

`save_new` writes and syncs a temporary file in the destination directory, creates the final path by hard link, then removes the temporary name. It never overwrites an existing save. A failed write cannot corrupt a previous valid save at the same path. The filesystem must support hard links. A crash can leave a temporary file; directory entries are not synced and durable recovery after power loss is not guaranteed. To keep multiple checkpoints, choose distinct paths. The byte API lets an embedder use its own durable storage protocol.

## Limits

The file size cap bounds parser input, not process memory to the byte. The format contains no integrity signature or authenticated provenance. Checksums used by examples are diagnostics only. Loading untrusted saves should be subject to normal application-level file and resource controls. No migration from future versions is implemented.
