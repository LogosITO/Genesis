# Performance Research 3 baseline: final Research 2 renderer

**Status:** reproduced locally before changing renderer code. Branch
`efficiency-research`, HEAD `86658504afc6c5afac5e968c9c4c3a0ca68479a3`, clean working tree.
The diff from measured Research 2 implementation `67f4956` to this HEAD
contains only the Research 2 report and two documentation links. No shader,
renderer, World Lab, or benchmark source changed in that interval.

## Method

`cargo test --release --target-dir target/performance-research-3-build -p analytic-renderer --test mixed_benchmark native_compiler_matched_matrix --locked -- --ignored --nocapture --test-threads=1`

The initial default Cargo target unexpectedly selected an older benchmark
binary and reported **zero** matching tests. A separate target directory
recompiled the current source and ran **one** benchmark test. Only that clean
rebuild is used below. Its 68 machine-readable rows are retained locally in
`target/performance-research-3-baseline.txt` and the ignored review package.

All A/B/C/D measurements within a row use the same world, camera, near/far
interval, 128-step policy, normals, materials, depth selection, and wgpu
timestamp query. A is the interpreter without the visual bounds guard; B adds
the guard; C is constant-specialized WGSL without the guard; D adds the guard.
Each result is GPU compute p50/p95 in milliseconds from seven samples after
three warmups. It excludes CPU preparation, presentation, and readback.

Environment: Windows, Rust 1.94.0, wgpu 30.0.1, NVIDIA GeForce RTX 4070 SUPER,
Vulkan, driver 591.86, release profile. This is one physical GPU.

| Nodes | Pixels | A | B | C | D |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1280×720 | 4.583/4.655 | 1.185/1.759 | 0.046/0.047 | 0.038/0.039 |
| 1 | 1920×1080 | 8.630/9.489 | 2.080/2.564 | 0.088/0.088 | 0.066/0.067 |
| 1 | 2560×1440 | 15.044/15.623 | 3.541/3.996 | 0.151/0.151 | 0.113/0.113 |
| 8 | 1280×720 | 6.492/7.301 | 1.404/1.684 | 0.059/0.059 | 0.038/0.039 |
| 8 | 1920×1080 | 14.689/15.144 | 2.598/3.381 | 0.125/0.125 | 0.068/0.068 |
| 8 | 2560×1440 | 25.586/26.174 | 3.939/4.708 | 0.216/0.216 | 0.115/0.116 |
| 16 | 1280×720 | 11.313/13.150 | 3.626/5.017 | 0.090/0.091 | 0.055/0.056 |
| 16 | 1920×1080 | 24.409/26.122 | 6.739/8.548 | 0.190/0.191 | 0.093/0.094 |
| 16 | 2560×1440 | 43.946/45.567 | 14.622/17.723 | 0.329/0.330 | 0.158/0.158 |
| 32 | 1280×720 | 25.817/27.177 | 12.191/14.005 | 0.172/0.173 | 0.110/0.117 |
| 32 | 1920×1080 | 52.069/55.695 | 23.620/26.290 | 0.361/0.361 | 0.197/0.197 |
| 32 | 2560×1440 | 89.859/91.245 | 40.710/41.687 | 0.619/0.839 | 0.328/0.329 |

The exact source-definition SHA-256 revisions for the complexity rows are:

| Nodes | Revision |
| ---: | --- |
| 1 | `6d82a73e86fb1e0e6fd735dded64684b01b48cc4c0f3a6fa60438b506648bd0b` |
| 8 | `20ce78f3818cae8f2acf93498ed7848f1477ea12fb4fc6c283e545c57d619344` |
| 16 | `81555aeca3e39db539e98a98537a1e8c71f9d2c24caa5d2af6764c3570a3caf7` |
| 32 | `ed303a9259b68c0c42d412b5b14e98a743897f886dc9eba9ceb8f8900e971af3` |

These rows confirm the prior trend on the final Research 2 HEAD. The values
vary between runs; they are not a performance promise for another adapter,
driver, field, material workload, or changing world.
