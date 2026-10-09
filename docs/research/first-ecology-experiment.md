# First Ecology local experiment

**Status:** local experimental observation, not biological validation. Reproduce each JSON result with `cargo run -p first-life --locked -- ecology <scenario>`; supported scenarios are `isolated`, `competition`, `separated`, `environment-change`, `pruning`, and `replay`. Each uses seed 31, 40 fixed 0.1-second steps, threshold 0.5, and a finite source capacity/refill of 0.2 units per tick. Results below were measured on the local Windows Rust 1.94.0 development build. Snapshot counts include analytic render primitives, not physical resource cells.

| Scenario | Final node counts | Cumulative source allocation | Consequence |
| --- | ---: | ---: | --- |
| Isolated | 16 | ≈8.0 | One organism uses the shared supply alone. |
| Competition | 8, 8 | ≈8.0 | Both draw from the same 0.2-unit tick supply. |
| Separated | 17, 17 | ≈16.0 across two sources | Spatially separated sources fund both independently. |
| Source disabled at tick 20 | 4, 4 | ≈3.8 | Growth slows after allocation ends; stock refills to 0.2. |
| Child 1 pruned at tick 20 | 6, 9 | ≈8.0 | Surviving tips alter subsequent proportional shares; IDs are not reused. |
| Save/load at tick 20 | 8, 8 | ≈8.0 | Final runtime equals uninterrupted competition exactly. |

These figures are outcomes of the current mathematical rules, not targets hardcoded by the implementation. `total_allocated` is source withdrawal; node maturity signals are not conserved units. Floating-point ledgers show small residuals near zero. The JSON also reports IDs, branch and tip counts, budgets, remaining stock, per-tick allocation and replenishment, save size, snapshot size, and a reproducible FNV-1a fingerprint of the saved bytes.

The focused contact regression creates two finite-source organisms, grows a branch across a body path, verifies direct/BVH contact, prunes that branch, verifies both queries miss, and moves the body through the cleared path. A separate offscreen GPU test renders the two-organism scene before and after growth; direct and GPU BVH images agree. With `GENESIS_ECOLOGY_CAPTURE` set to an output `.ppm` path, that test saves the real 512 × 512 direct readback. The local capture was visually inspected: two separate green/blue organisms and the gold shared source were visible, with no missing primitives apparent at this view. This does not inspect every camera angle or the native input path. Neither test substitutes for the uncompleted manual click/select/P/move viewport workflow.

## Performance method and limitations

Run `cargo run --release -p first-life --locked -- ecology measure`. The local Windows / Rust 1.94.0 release-profile result below uses 50 serial repetitions per row. Each repetition clones a fixed baseline outside the timed region, executes one full tick, then separately builds a renderer snapshot and CPU contact BVH. The growing rows start immediately before a known node-allocation tick; static rows have a disabled source and unchanged geometry. Values are median / p95 milliseconds, rounded to four decimal places. The contact BVH build is measured separately even though no body exists in these scenes and the full tick therefore does not build it.

| Organisms | Tick | Result nodes | Full CPU tick | Renderer snapshot | Contact BVH build | Node storage lower bound |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | Static | 1 | 0.0007 / 0.0026 | 0.0002 / 0.0007 | 0.0003 / 0.0008 | 48 B |
| 1 | Growing | 2 | 0.0008 / 0.0015 | 0.0003 / 0.0005 | 0.0003 / 0.0004 | 96 B |
| 2 | Static | 2 | 0.0011 / 0.0012 | 0.0003 / 0.0003 | 0.0003 / 0.0003 | 96 B |
| 2 | Growing | 4 | 0.0014 / 0.0022 | 0.0005 / 0.0006 | 0.0005 / 0.0009 | 192 B |
| 4 | Static | 4 | 0.0018 / 0.0019 | 0.0004 / 0.0005 | 0.0004 / 0.0004 | 192 B |
| 4 | Growing | 6 | 0.0022 / 0.0023 | 0.0006 / 0.0008 | 0.0005 / 0.0008 | 288 B |

A second identical local run gave full-tick medians of 0.0017, 0.0019, 0.0025, 0.0028, 0.0018, and 0.0022 ms in table order. The first two configurations varied substantially at this scale; the table is a recorded run, not a stable speed estimate.

The opt-in microbenchmark `cargo test --release -p world-simulation finite_allocation_and_growth_commit_benchmark --locked -- --ignored --nocapture --test-threads=1` times the real finite demand/solver function and `Organism::grow_allocated` separately in 50 serial single-root repetitions. Replenishment, direction construction, and snapshots are outside these two timings. All root nodes allocate one child; values are median / p95 milliseconds:

| Organisms | Demand and allocation | Growth commit |
| ---: | ---: | ---: |
| 1 | 0.0003 / 0.0003 | 0.0003 / 0.0003 |
| 2 | 0.0004 / 0.0004 | 0.0005 / 0.0006 |
| 4 | 0.0007 / 0.0008 | 0.0010 / 0.0011 |

These are very small scenes and near timer-resolution costs; they do not predict populated-world or full-frame latency. The phase microbenchmark is not a partition of the full tick because it uses a controlled root-only fixture and omits direction construction. A four-second hidden native viewport smoke run on NVIDIA GeForce RTX 4070 SUPER / Vulkan / wgpu 30.0.1 reported one 1280 × 720 status sample at tick 86 with 32 primitives, 0.035 ms GPU compute, 0.81 ms CPU submit, and 118.7 FPS. It was not a controlled GPU benchmark or a manual UI check; no reliable full-frame latency distribution was collected. The offscreen test confirms image change and direct/BVH parity, not speed. Do not infer an acceleration win from CPU query time alone: the 0.7.1 contact BVH rebuild dominates its query on the measured 380-solid scene.

Open questions: demand based on the strongest tip may overfavor broad organisms; source proximity and allocation are coupled but structural crowding is not; replenishment every tick is a chosen toy rule; no resource recycling or geometry union exists. New experiments should compare other policies only after a reproducible baseline.
