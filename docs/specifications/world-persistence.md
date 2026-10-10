# World persistence versions 7–9

Living World 3 writes `format_version: 9` when a field occurrence owns bounded
two-lobe `FieldMorphology`. The snapshot stores the current and initial radii,
limits, uptake and radius gain coefficients, allocation counters, and the
geometry-feedback flag. Loading validates these values and the exact authored
source revision, then rebuilds the derived graph and spatial bounds. A v7 or
v8 file claiming morphology is rejected. Worlds without morphology continue
to write v7 or v8.

The optional native F6 history bundle is distinct from a completed F5 save.
It contains an initial v9 snapshot, ordered applied events, final tick, final
completed snapshot, and a diagnostic FNV-1a digest. Replay validates the
bounded event stream and compares canonical completed world, tick and step.
The replay scheduler's internal event-order counter is discarded at the
comparison boundary because native events are applied directly. Neither
format is authenticated or promises bitwise replay across arbitrary platforms.

Living World 2 writes `format_version: 8` when a visual Field Graph occurrence
has resource-coupled `FieldGrowth`. The growth law, initial and maximum scale,
cumulative and last allocation, and current world-owned transform are in the
completed snapshot. The immutable graph source bytes and exact SHA-256 revision
remain in the field definition. The loader validates the growth state against
the scale and rejects a v7 file that contains growth. Worlds without dynamic
fields still write v7, preserving the original Living World 1 save bytes and
digests. Versions 1–7 remain readable under their existing gates.

A save is a **completed state snapshot**, optionally with events scheduled for
future ticks. It is not a history of previously applied inputs. Loading and
continuing must not reapply past events. The headless Living World 2 experiment
records a separate initial v8 snapshot and the typed tick-61 event; that pair
is self-contained because the snapshot embeds the exact graph source. Native
F5/F9 saves do not contain a complete keyboard event log; Living World 3
exports a separate applied-event bundle with F6.

Foundation 0.13 adds visual field definitions and occurrences. Static-field
saves use `format_version: 7`; versions 1–6 remain readable with empty field collections.
Files that claim an older version but contain field state are rejected. On load,
each distinct exact-byte graph is recompiled and checked against its recorded
SHA-256 revision and graph semantics version before the world is accepted. A
failed load leaves an existing runtime unchanged. The field source limit is
16 KiB, with at most four retained revisions and eight occurrences. Visual
fields have no contact or gameplay authority.

## Decision

`world-runtime` writes a JSON envelope with the runtime state. It reads valid versions 1–9. For version 1, missing `max_children` defaults to 2 and missing shared `budget` defaults to zero. For versions 1 and 2, the former dense node allocation count becomes `next_node_id`; their nodes, parents, and energy remain intact. Versions 3–9 require a valid explicit `next_node_id` and may contain gaps after pruning. Older saves without a kinematic body load with `body: None`. Versions 1–4 have unlimited sources without a reservoir; a finite reservoir mislabeled as an older version is rejected. Versions 1–5 have no authored world instances; authored definitions, occurrences or events mislabeled as an older version are rejected. Older runtimes are not expected to read new format versions. State includes fixed step, completed tick, seed, monotonic world ID allocator, sphere entities, organisms and sources, finite source ledgers where present, optional kinematic body state and last contact, shared exact-byte authored definitions and occurrence records, shared visual field definitions and occurrence records, pending typed events including authored updates, and next event order. The format is inspectable but not a stable public interchange API.

Input is limited to 2 MiB before parsing; each authored definition is separately bounded to 65,536 source bytes and each field graph to 16,384 bytes. Unknown fields, malformed JSON, unsupported versions, nonfinite or invalid numeric state, duplicate world IDs, invalid authored or field hashes or semantics, dangling definition references, missing or forward node parents, cycles, excess children, invalid budget or lifetime allocator, exhausted capacities, and invalid or conflicting pending events are rejected. Gaps in world IDs are allowed after occurrence removal, but IDs are never reused. Loading constructs a complete candidate, recompiles each distinct authored and field revision, migrates only older supported versions, validates it, then returns or replaces state; a failed `load_into` leaves the current runtime untouched. Exact `f64` JSON round trips use Serde JSON's `float_roundtrip` parser feature.

`save_new` writes and syncs a temporary file in the destination directory, creates the final path by hard link, then removes the temporary name. It never overwrites an existing save. A failed write cannot corrupt a previous valid save at the same path. The filesystem must support hard links. A crash can leave a temporary file; directory entries are not synced and durable recovery after power loss is not guaranteed. To keep multiple checkpoints, choose distinct paths. The byte API lets an embedder use its own durable storage protocol.

## Limits

The file size cap bounds parser input, not process memory to the byte. The format contains no integrity signature or authenticated provenance. Checksums used by examples are diagnostics only. Loading untrusted saves should be subject to normal application-level file and resource controls. No migration from future versions is implemented.
