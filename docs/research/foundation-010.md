# Foundation 0.10 local authoring experiment

## Established implementation

External JSON v1 definitions are parsed and expanded by `world-authoring` without GPU access. The generated immutable typed structure holds source bytes and analytic capsule segments. The native `first-light --authoring FILE` window converts it to a renderer snapshot, and R atomically reloads the file. The normal Passage and ecology paths remain separate. See the [format contract](../specifications/mathematical-authoring.md) and [architecture RFC](../architecture/authoring-rfc.md).

## Method and actual outputs

Local Windows, Rust 1.94.0 release build, NVIDIA GeForce RTX 4070 SUPER / Vulkan / driver 591.86 for GPU observations. The source base was `f193d022dd7025403cbb88f169109d9d4ec953a4` on a new local feature branch. `branch-a.json` produced **170 expanded symbols, 26 segments, stack depth 3**. `branch-b.json` produced **121 symbols, 15 segments, stack depth 4**. The definitions differ in rewrite and orientation parameters. `authoring-inspect` loaded both from disk and emitted machine-readable summaries. A native preview of A rendered without the earlier empty-world statistics panic after that regression was fixed. Editing a temporary copy from A to B and sending R to the test window emitted a new 15-segment summary without recompiling Rust. A unit test rejected malformed replacement bytes without changing the prior scene; its native error text was not captured from the Windows redirection session.

An opt-in GPU readback test rendered both definitions at 640×480. GPU direct and GPU BVH RGB bytes matched for each scene on this adapter; the two authored images differed. Actual BMPs are in ignored `target/authoring-captures-010/` and are not generated illustrations or published fixtures. The preview uses analytic capsules and contains no mesh/voxel geometry. Pixel equality on other adapters is not implied.

## Bounded CPU scaling

The same A rule was run at 1–6 iterations with explicit budgets raised within hard ceilings. Thirty release-profile repetitions per row separately timed in-memory compilation, analytic `Scene` construction and CPU BVH building. Values are p50/p95 milliseconds. `minimum_source_and_segment_bytes` counts source bytes plus `size_of::<Segment>() × count` only; it **does not** measure peak process RSS, expanded-string capacity, BVH allocation, renderer buffers or driver memory.

| Iterations | Symbols | Segments | BVH nodes/depth | Min source + segment bytes | Compile p50/p95 | Scene p50/p95 | CPU BVH p50/p95 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 14 | 2 | 1 / 1 | 485 | 0.0017 / 0.0031 | 0.0002 / 0.0007 | 0.0001 / 0.0003 |
| 2 | 53 | 8 | 3 / 2 | 917 | 0.0024 / 0.0038 | 0.0007 / 0.0009 | 0.0006 / 0.0008 |
| 3 | 170 | 26 | 15 / 4 | 2,213 | 0.0045 / 0.0053 | 0.0020 / 0.0024 | 0.0068 / 0.0117 |
| 4 | 521 | 80 | 63 / 6 | 6,101 | 0.0085 / 0.0175 | 0.0053 / 0.0103 | 0.0386 / 0.0849 |
| 5 | 1,574 | 242 | 127 / 7 | 17,765 | 0.0261 / 0.0663 | 0.0266 / 0.0337 | 0.1798 / 0.3114 |
| 6 | 4,733 | 728 | 511 / 9 | 52,757 | 0.1006 / 0.1467 | 0.1638 / 0.2170 | 1.1066 / 1.4160 |

The separate file-reading CPU inspector was run 30 times per size while it still emitted timing by default, and reported load+compile p50/p95 of 0.126/0.211, 0.122/0.192, 0.129/0.166, 0.141/0.158, 0.159/0.205, and 0.251/0.353 ms for 1–6 iterations. The same timing is now opt-in with `--measure`; default JSON omits timing and is repeatable byte for byte. The first three sizes are dominated by process/file/clock noise. The table above uses in-process phase timings; it is not a claim about end-to-end load latency on other machines. At 728 segments, **CPU BVH build dominated the measured CPU phases**. A zero-iteration `X` definition was rejected because it draws no `F`, as specified.

## Native GPU observations

At actual 1280×720, Fifo present, `GENESIS_MEASURE=1`, an initial 26-segment preview reported a 15-node, depth-4 GPU BVH build of 0.0241 ms, with 1,664 primitive bytes and 960 BVH node bytes queued. Its last sampled frame window had 249 frames, interval p50/p95 6.04/6.58 ms, and sparse GPU-compute p50/p95 0.034/0.036 ms (10 samples). The 728-segment preview reported a 511-node, depth-9 BVH build of 1.0013 ms, with 46,592 primitive bytes and 32,704 BVH node bytes queued. Its last window had 330 frames, interval p50/p95 6.05/6.54 ms, and GPU-compute p50/p95 1.148/1.387 ms (12 samples). CPU `queue.write_buffer` enqueue was 0.0233/0.0198 ms for those initial uploads; it is not GPU transfer completion time. Logs are ignored local artifacts under `target/authoring-native-scale-*.log`.

These two windows were not a calibrated GPU benchmark suite. Sparse GPU timestamp samples, Fifo pacing, changing scene coverage and desktop load limit conclusions. Full input-to-photon latency and actual peak memory were not measured. A 728-segment scene fit the configured renderer budgets on this adapter; this says nothing about arbitrary GPUs.

## Safety and limitations

The compiler rejects unsupported versions, unknown or unbalanced symbols, invalid dimensions, nonfinite JSON numbers, oversized source files, expansion, work, stack, segment and position budget breaches. Construction is all-or-error. Recompiling retained exact source bytes reproduces the same segment list locally. No cross-platform bitwise equivalence, collision union, living growth, world-save integration or general field language is claimed. Existing `WorldState` save files are unchanged. Source bytes must be archived for reproducible authored previews; author `revision` alone does not protect against changed content.

Run the CPU examples from `examples/authoring/README.md`. The release CPU scaling test is `cargo test -p first-light --release --locked authored_scaling_benchmark -- --ignored --nocapture`; the GPU image test is `cargo test -p first-light --release --locked authored_gpu_images_match_direct_and_bvh -- --ignored`. Other established GPU tests remain opt-in.
