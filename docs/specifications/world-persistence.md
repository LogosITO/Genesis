# World persistence version 2

## Decision

`world-runtime` writes a JSON envelope with `format_version: 2` and the runtime state. It also reads valid version-1 saves: missing `max_children` defaults to 2, and the new shared organism `budget` defaults to zero. Version-1 chain nodes and their maturity energy remain intact. New saves are version 2, so an older runtime is not expected to read them. The state includes fixed step, completed tick, seed, world ID allocator, sphere entities, organism parameters, every node/parent/energy value, shared budget, resource sources, pending typed events, and next event order. The format is inspectable but not a stable public interchange API.

Input is limited to 65,536 bytes before parsing. Unknown fields, malformed JSON, unsupported versions, nonfinite or invalid numeric state, duplicate or missing world IDs, missing or forward node parents, cycles, excess children, invalid budget, exhausted capacities, and invalid pending events are rejected. Loading constructs a complete candidate, validates it, then returns or replaces state; a failed `load_into` leaves the current runtime untouched. Exact `f64` JSON round trips use Serde JSON's `float_roundtrip` parser feature.

`save_new` writes and syncs a temporary file in the destination directory, creates the final path by hard link, then removes the temporary name. It never overwrites an existing save. A failed write cannot corrupt a previous valid save at the same path. The filesystem must support hard links. A crash can leave a temporary file; directory entries are not synced and durable recovery after power loss is not guaranteed. To keep multiple checkpoints, choose distinct paths. The byte API lets an embedder use its own durable storage protocol.

## Limits

The file size cap bounds parser input, not process memory to the byte. The format contains no integrity signature or authenticated provenance. Checksums used by examples are diagnostics only. Loading untrusted saves should be subject to normal application-level file and resource controls. No migration from future versions is implemented.
