# Foundation 0.11 local evaluation

## Status and method

This is a local experiment, not a production benchmark. Windows x64, pinned Rust 1.94, debug profile, one process. The CPU measurement test ran 30 serial samples per case and reports the median wall-clock time for each operation. The small fixture has one capsule; `branch-a.json` has 26. Instances of the same exact source share one definition record and one compiled `Structure` in memory. Counts of 1, 2 and 8 fit the explicit world budgets. Reproduce with:

```sh
cargo test -p authoring-inspect authored_world_cpu_measurements --locked -- --ignored --nocapture
cargo test -p first-light authored_world_gpu_matches_cpu_and_direct_bvh --locked -- --ignored --nocapture
```

## CPU measurements

Times are milliseconds at p50. `reuse` is the total for the second through last occurrence, so the one-instance row has no reuse work. `spawn` includes compilation for the first occurrence. Snapshot and BVH columns are separate; save and load include JSON work, and load recompiles the source.

| Definition | Instances / capsules | Compile | First spawn | Reuse total | Transform | Snapshot | Render BVH | Contact BVH | Save | Load | Save bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| single | 1 / 1 | 0.016 | 0.042 | 0 | 0.0008 | 0.013 | 0.0016 | 0.013 | 0.136 | 0.156 | 7,133 |
| single | 2 / 2 | 0.019 | 0.048 | 0.010 | 0.0009 | 0.017 | 0.0021 | 0.015 | 0.225 | 0.198 | 7,995 |
| single | 8 / 8 | 0.022 | 0.057 | 0.083 | 0.0011 | 0.031 | 0.0072 | 0.030 | 0.332 | 0.303 | 13,171 |
| branch-a | 1 / 26 | 0.045 | 0.081 | 0 | 0.0051 | 0.039 | 0.057 | 0.050 | 0.174 | 0.223 | 9,289 |
| branch-a | 2 / 52 | 0.045 | 0.081 | 0.016 | 0.0050 | 0.061 | 0.175 | 0.102 | 0.192 | 0.236 | 10,146 |
| branch-a | 8 / 208 | 0.046 | 0.081 | 0.109 | 0.0050 | 0.238 | 0.905 | 0.452 | 0.316 | 0.341 | 15,292 |

The source is stored once per revision: 318 or 452 source bytes across all occurrences. The save grows with occurrence records; the serialized byte count also includes the pre-existing runtime fields and pretty-printed JSON. Exact process heap use was not measured. The eight-occurrence `branch-a` case makes CPU render BVH construction materially larger than grammar compilation on this machine; this is a reason to measure real scene update frequency before attempting incremental refits.

## GPU parity and offscreen images

Adapter: NVIDIA GeForce RTX 4070 SUPER, Vulkan, driver 591.86. Resolution 640×480, debug profile, 20 timestamp samples per state/path. The test used CPU semantic picking and GPU direct/BVH readback on three explicit rays. It checked hit/miss, primitive identity, hit distance within 0.002 world units and outward normal components within 0.005. The three direct and BVH offscreen RGB images matched byte for byte. The captured images show two capsules initially, the second shifted after movement, and one remaining after disabling the first. The `target/authored-world-captures/` BMPs are local ignored test artifacts, not committed screenshots or evidence of native window presentation.

| State | Primitives | Direct GPU p50 / p95 ms | BVH GPU p50 / p95 ms | CPU BVH build ms | Primitive upload queue ms / bytes | BVH upload queue ms / bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| initial | 2 | 0.01331 / 0.01411 | 0.01197 / 0.01229 | 0.0105 | 0.0254 / 128 | 0.0878 / 64 |
| moved | 2 | 0.01347 / 0.01434 | 0.01232 / 0.01325 | 0.0053 | 0.0282 / 128 | 0.0217 / 64 |
| disabled | 1 | 0.00947 / 0.01062 | 0.00938 / 0.00982 | 0.0075 | 0.0347 / 64 | 0.0556 / 64 |

GPU values are timestamped compute work; upload values are CPU time in `queue.write_buffer`, not measured device transfer or full-frame presentation. First-use compilation, driver clocks, tiny workloads and ordering can dominate differences. The existing automatic policy still chooses direct traversal for at most four primitives. No claim is made for other adapters, resolutions, complex authored scenes or window presentation.

## Correctness evidence and limits

Workspace tests, legacy save tests, the headless two-occurrence scenario, contact direct/BVH comparison, old renderer GPU tests and the new authored GPU test passed locally. A renderer restricted to one 64-byte primitive record rejected the valid two-capsule scene without changing the world. Version-6 saves retain exact source bytes and pending events; corrupt hashes, unsupported semantics, dangling references, duplicate IDs, invalid scale and v5 mislabeling were rejected without modifying an active runtime. Segment selections expire on remove, replacement, transform or participation change. The native `--authored-world` process started on Vulkan and rendered several 1280×720 frames near 60 FPS before manual termination. An initial telemetry branch panicked on an absent ordinary sphere; it was fixed and the repeated run stayed active without that failure. Human visual inspection of the window and keyboard/mouse interaction remain unverified. Rotation, dynamic authored growth, cross-revision state migration, watertight CSG union and cross-platform bitwise identity are outside the contract.
