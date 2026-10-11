# Living World 4 evidence retained for QA-0

The original experiment ran at `2cd2913effb178832d7a6f9a0b82a7dd319af1c0`.
`source-manifest.json` records ten source hashes; all ten still matched on
2026-10-11. `experiment.json` is the original complete fixed A/B and ablation
output. A release-profile rerun on that source commit produced structurally
identical JSON. `history-bundle.json` replayed to tick 240 with `equal: true`
and digest `9103ee2194ffe042`. `spatial-oracle.json` contains actual selected
GPU distances, identities and uncertainty from the original diagnostic run.

`gpu-parity.log` records the original 40,960-pair interpreter/Combined
comparison. The two GPU paths share the ray-step algorithm, so agreement is
not an independent correctness proof. `qa0-gpu-benchmark.jsonl` contains six
new release-profile offscreen measurement records from the same RTX 4070 SUPER
and Vulkan backend. Seven samples follow two warmups. Values include GPU
compute timestamps and offscreen waits, not window presentation or VSync.

The ten original 640×360 BMPs remain in the ignored local directory
`target/living-world-4-review/captures/`; their hashes are in
`captures.sha256`. The BMPs are not committed. Control and intervention BMP
hashes agree at ticks 0 and 60 and differ at 61, 120 and 240. The tick-240
pair was visually inspected in QA-0. A new QA-0 GPU run regenerated all ten
BMPs with hashes exactly matching the originals on the same adapter/backend.
The local captures must be retained or regenerated to independently inspect
pixels.

Reproduce from the repository root:

```text
cargo run --release -p first-light --locked -- --living-rules-experiment
cargo run --release -p first-light --locked -- --living-morph-replay docs/research/evidence/living-world-4/history-bundle.json
cargo test --release -p first-light --locked two_topologies_native_gpu_resolution_measurement -- --ignored --nocapture --test-threads=1
cargo test -p first-light --locked two_topologies_native_gpu_identity_and_images -- --ignored --nocapture --test-threads=1
```

The last two commands require a compatible native GPU. Set
`GENESIS_LW4_CAPTURE_DIR` to an absolute writable directory to regenerate
the BMPs. No DX12 or second-adapter result is included here.
