# World persistence version 4

## Decision

`world-runtime` writes a JSON envelope with `format_version: 4` and the runtime state. It reads valid version-1, version-2, and version-3 saves. For version 1, missing `max_children` defaults to 2 and missing shared `budget` defaults to zero. For versions 1 and 2, the former dense node allocation count becomes `next_node_id`; their nodes, parents, and energy remain intact. Version-3 and version-4 saves require a valid explicit `next_node_id` and may contain gaps after pruning. Older saves without a kinematic body load with `body: None`. New saves are version 4, so older runtimes are not expected to read them. State includes fixed step, completed tick, seed, world ID allocator, sphere entities, organisms and sources, optional kinematic body state and last contact, pending typed events including `PruneBranch` and `SetBodyVelocity`, and next event order. The format is inspectable but not a stable public interchange API.

Input is limited to 65,536 bytes before parsing. Unknown fields, malformed JSON, unsupported versions, nonfinite or invalid numeric state, duplicate or missing world IDs, missing or forward node parents, cycles, excess children, invalid budget or lifetime allocator, exhausted capacities, and invalid or conflicting pending cuts are rejected. Loading constructs a complete candidate, migrates only older supported versions, validates it, then returns or replaces state; a failed `load_into` leaves the current runtime untouched. Exact `f64` JSON round trips use Serde JSON's `float_roundtrip` parser feature.

`save_new` writes and syncs a temporary file in the destination directory, creates the final path by hard link, then removes the temporary name. It never overwrites an existing save. A failed write cannot corrupt a previous valid save at the same path. The filesystem must support hard links. A crash can leave a temporary file; directory entries are not synced and durable recovery after power loss is not guaranteed. To keep multiple checkpoints, choose distinct paths. The byte API lets an embedder use its own durable storage protocol.

## Limits

The file size cap bounds parser input, not process memory to the byte. The format contains no integrity signature or authenticated provenance. Checksums used by examples are diagnostics only. Loading untrusted saves should be subject to normal application-level file and resource controls. No migration from future versions is implemented.
