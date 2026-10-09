# Foundation 0.10.1: local identity and provenance evaluation

## Established implementation

The static authoring compiler hashes exact source bytes with SHA-256, records compiler semantics version 1, gives each emitted segment a bounded derivation path, and writes self-contained versioned snapshots. A preview reload reads one bounded byte vector, skips identical bytes, otherwise compiles a complete candidate, constructs its analytic scene and CPU BVH, then replaces the active preview and clears selection. `WorldState` saves and living organisms are unchanged. See the [canonical contract](../specifications/mathematical-authoring.md) and [ADR 0008](../adr/0008-content-addressed-authoring.md).

## Deterministic and adversarial checks

The SHA-256 implementation matched the published `abc` test vector. Equal bytes reproduced equal segments and identities; equivalent JSON with extra whitespace produced a different digest. Repeated capsules with identical endpoints got distinct derivation paths. A valid eight-step path, 65,281-symbol expansion, and 1,024-segment input compiled within configured budgets. Wrong expected digest, mutated snapshot bytes, unknown snapshot/compiler versions, invalid JSON, malformed grammar and exceeded budgets were rejected. A preview test confirmed unchanged-source no-op, rollback after invalid JSON and budget failure, stale selection rejection after changed source, and a different CPU BVH node count after A→B replacement.

The opt-in native GPU test on Windows/NVIDIA GeForce RTX 4070 SUPER/Vulkan rendered A and B at 640×480. Direct and BVH RGB readbacks matched per scene; two renders of an unchanged scene reused the GPU BVH, while switching A→B increased its rebuild count by one. These are local adapter observations, not cross-adapter proof. Existing The Passage tests run separately with the workspace and opt-in suites.

## Release-profile timing method

Rust 1.94.0, local Windows machine, 30 warm in-process samples per case. Values below are p50/p95 milliseconds. Reading used an ignored file under `target/`, so Windows file cache and filesystem scheduling affect results. Parsing includes schema validation. Rewriting includes derivation-path identity generation; geometry includes segment records and parent identities. Those two operations are not individually timed because they occur in the same bounded pass. Timing resolution makes sub-microsecond values approximate.

| Valid case | Input bytes | Symbols | Segments | Read | Parse + validate | SHA-256 | Rewrite + identity | Geometry |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Branch A | 452 | 170 | 26 | 0.0436 / 0.0786 | 0.0029 / 0.0053 | 0.0004 / 0.0005 | 0.0035 / 0.0155 | 0.0032 / 0.0134 |
| A at 6 iterations | 341 | 4,733 | 728 | 0.0290 / 0.0640 | 0.0017 / 0.0036 | 0.0002 / 0.0004 | 0.0941 / 0.1135 | 0.0597 / 0.0758 |
| Near symbol ceiling | 545 | 65,281 | 1 | 0.0657 / 0.0753 | 0.0043 / 0.0048 | 0.0004 / 0.0005 | 1.4098 / 1.5369 | 0.0912 / 0.1023 |
| Segment ceiling | 1,310 | 1,024 | 1,024 | 0.0289 / 0.1127 | 0.0022 / 0.0055 | 0.0006 / 0.0010 | 0.0196 / 0.0273 | 0.0200 / 0.0579 |

The existing separate scene/BVH benchmark at 728 segments measured p50/p95 **0.1579/0.2111 ms** for renderer snapshot construction and **0.9732/1.3691 ms** for CPU BVH build. The new bounded derivation path increased minimum source-plus-segment storage to 81,877 bytes for that case; this is not total process memory.

The A↔B preview benchmark measured identical-byte reload separately from changed-byte preparation and replacement. It excludes file reading and GPU execution. Across 30 samples, unchanged-source p50/p95 was **0.0001/0.0002 ms**; changed compile+scene+CPU-BVH+swap was **0.0133/0.0357 ms**; replacing and dropping two already prepared preview objects was **0.0002/0.0002 ms**. These tiny durations approach timer resolution and are local observations, not general latency guarantees.

## Memory and interpretation

`DerivedSymbol` is 22 bytes on this local build; at the 65,536-symbol cap one vector's element storage is 1,441,792 bytes. Eager rewriting may temporarily hold both old and new vectors, so element storage alone can approach 2,883,584 bytes, plus capacities, source, segments, scene, BVH and allocator overhead. The source cap is 65,536 bytes, segment cap is 1,024, and snapshot JSON cap is 512 KiB. Peak RSS and GPU memory were not measured. At the symbol ceiling, rewrite/path generation dominated the measured CPU phases; at 728 segments, CPU BVH build dominated scene preparation. No unlimited-detail claim follows from these bounded tests.

## Release governance audit

`world-authoring` is an unpublished library at 0.1.0; `authoring-inspect` is an unpublished CLI example at 0.1.0. Neither appears in the Release Please package map or manifest, so the current configuration does not create an independent release for either. The registered `first-light` example is also `publish = false`, with its path excluded from its Release Please package and `skip-github-release: true`. All existing manifest versions matched their Cargo package versions in the local checkout. The new dependency from `first-light` to `world-authoring` is path-only, so no publishable version constraint needs propagation. No release configuration edit was justified. GitHub-hosted Release Please behavior after integration was not observed.

## Open questions

Cross-revision topology correspondence remains undefined. A modified definition always invalidates current preview selection, even if geometry happens to match. The self-contained snapshot digest detects corruption relative to its stored value; it is not a signature. A future world save needs an explicit instance schema and migration policy. GPU OOM/device-loss after the CPU transaction remains governed by the renderer and was not made atomically reversible.
