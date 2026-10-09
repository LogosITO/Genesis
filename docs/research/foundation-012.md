# Foundation 0.12 local field-graph experiment

## Status and method

**Experimental finding, one machine.** Windows x64, Rust 1.94, debug profile, NVIDIA GeForce RTX 4070 SUPER via Vulkan. The fixed shader interpreted validated nodes directly; no mesh, voxel or analytic-primitive-list lowering occurred. The source files were `examples/field-graph/definitions/twin.json` and `plinth.json`. Both were loaded by the same executable without recompilation. Commands:

```sh
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/twin.json
cargo run -p field-graph-preview --locked -- examples/field-graph/definitions/plinth.json
cargo test -p analytic-renderer --test field_graph_gpu --locked -- --ignored --nocapture --test-threads=1
```

The program wrote actual 640×480 GPU readback PPMs under ignored `target/field-graph-captures/`. Local PNG conversions were used only to inspect them. Twin lobes showed two joined spheres; plinth showed a box with a top sphere. There was no missing central geometry. Full-image inspection found 0 red uncertain pixels, 100 yellow exhausted pixels for twin lobes, and 10 yellow pixels for plinth, concentrated near grazing silhouettes. Those pixels are deliberately diagnostic, not silently converted to background. The captures are local artifacts, not simulated screenshots or evidence of native window presentation.

A live `--watch` process was exercised with a temporary copy of twin lobes, invalid JSON, then plinth. It printed the original capture, rejected the invalid reload while retaining that preview, and then printed the new plinth capture after the valid file arrived. The process was stopped after this scripted check; no interactive window was involved.

## CPU/GPU correctness

The opt-in readback test compared center hit, exterior miss, interior uncertainty, and an offset hit on both files. Hit distances agreed within 0.005 world units in these selected rays. Additional GPU queries exercised spheres of radii 0.01, 1 and 100, a nested scale/translation under a coincident union, tangent and near-tangent rays, and a one-step budget. The tangent returned `Exhausted`; the slightly inside ray returned a hit and the slightly outside ray returned `Miss`. One-step evaluation returned `Exhausted`. CPU tests covered analytic primitive samples, transforms, union ties, bounds, invalid JSON, IDs, references, cycles, depth, expanded work, nonfinite values, rays starting inside, and arithmetic overflow. The existing `Field::difference(A,A)` example demonstrated a zero set with no negative interior; this graph format deliberately excludes difference.

Selected-ray agreement is not a universal finite-precision proof. CPU uses `f64`, GPU uses `f32` and finite-difference normals. Near tangencies, branch ties, very small features far from the origin, and other adapters remain open.

## Local measurements

The scaling test generated balanced unions of translated instances of one sphere with shared child references. Each row used 40 parser/compiler/CPU samples and 12 GPU query/frame samples. Values below are p50 milliseconds; GPU columns include host preparation, submission and blocking readback, **not isolated GPU compute time**. Query batches had 512 rays. Frames were 320×240 RGBA8. The same shader and scene were used at each iteration budget. `compile` includes JSON parse and validation. Node payload is 32 bytes per node, excluding allocation overhead, uniform, ray and image buffers.

| Nodes / expanded visits | Budget | JSON parse | Compile incl. parse | CPU sample | Query 512 + readback | Frame + readback | Node bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 / 2 | 32 | 0.0119 | 0.0180 | 0.000121 | 0.864 | 2.054 | 64 |
| 2 / 2 | 128 | 0.0119 | 0.0180 | 0.000121 | 0.908 | 2.046 | 64 |
| 8 / 11 | 32 | 0.0319 | 0.0443 | 0.000390 | 1.491 | 2.660 | 256 |
| 8 / 11 | 128 | 0.0319 | 0.0443 | 0.000390 | 1.509 | 2.860 | 256 |
| 16 / 23 | 32 | 0.0580 | 0.0812 | 0.000767 | 2.329 | 3.895 | 512 |
| 16 / 23 | 128 | 0.0580 | 0.0812 | 0.000767 | 2.648 | 5.035 | 512 |
| 32 / 47 | 32 | 0.1074 | 0.1499 | 0.001522 | 1.515 | 7.332 | 1024 |
| 32 / 47 | 128 | 0.1074 | 0.1499 | 0.001522 | 1.704 | 11.061 | 1024 |

The 32-node query time being lower than the 16-node time is likely affected by workload and scheduling; these are not normalized GPU compute timings. The largest graph's 128-step frame was slower than the 32-step frame in this workload. First-use shader/driver behavior and debug builds affect p95; no confidence interval was estimated. A repeat of the scaling test after other GPU runs gave materially lower frame medians (for example, 32 nodes / 128 steps fell from 11.061 to 4.221 ms), demonstrating sensitivity to GPU state and scheduling. The two 640×480 one-shot captures reported 12.186 ms and 11.508 ms full frame including readback. They are not directly comparable to the 320×240 series. Matched-scene comparison against the existing analytic renderer was not performed because its direct primitive intersections and this field zero-set tracer differ in hit, inside and tangent semantics.

## Limits and next questions

The current path allocates and uploads node data every draw; a cache may help repeated static frames, but needs measured evidence and reload invalidation. No isolated GPU timestamp, upload-only cost, full memory residency, other GPU adapter, second machine or native presentation was measured. No implicit collision, topology guarantee, world instance, save migration, or real organism growth is claimed. A future milestone should first establish numerical occupancy/contact semantics, then test GPU and CPU agreement over a broader scale and adapter matrix before attaching field graphs to authoritative world state.
