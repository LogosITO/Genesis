---
title: Research
template: splash
---

The [evidence ledger](../reference/research/references/) distinguishes established knowledge, hypotheses, decisions, and open questions. New experiments should use the [template](../reference/research/experiment-template/) and include reproducible data. The [First Light local benchmark](../reference/research/first-light-benchmark/) and [First Life measurement record](../reference/research/first-life-measurements/) document local observations without general performance claims.

The [First Structure measurement record](../reference/research/first-structure-measurements/) compares bounded node counts in development and release builds, with separate GPU timing and explicit snapshot overflow.

The [First Scale measurement record](../reference/research/first-scale-benchmark/) reports the 381-primitive GPU scene, direct-path scaling, and the CPU-only BVH experiment.

The [GPU BVH experiment](../reference/research/gpu-bvh-acceleration/) compares CPU direct, CPU BVH, GPU direct, and GPU BVH using offscreen readback and matched-scene timings.

The [First Interaction measurements](../reference/research/first-interaction-measurements/) record CPU picking, pruning, snapshot/BVH rebuilds, GPU changes, and deterministic save/replay outcomes. The [interaction contract](../reference/specifications/first-interaction/) defines what a selected branch means.

The [First Contact measurements](../reference/research/first-contact-measurements/) compare direct and BVH collision queries, narrow phase, rebuild and full tick costs, plus contact/pruning replay outcomes. The [contact contract](../reference/specifications/kinematic-contact/) states the numerical and geometry limits.

The [Foundation 0.9.6 local verification](../reference/research/foundation-096/) records native window measurements, resize evidence, and the opt-in test audit. The [native runtime contract](../reference/specifications/native-runtime/) defines surface and device failure policy.

The [Foundation 0.10 authoring experiment](../reference/research/foundation-010/) records two external structures, GPU readbacks and bounded scaling measurements. The [authoring RFC](../reference/architecture/authoring-rfc/) separates structural generators from continuous mathematical fields.

The [Foundation 0.10.1 provenance evaluation](../reference/research/foundation-0101/) measures exact-byte revisions, derivation identities, snapshot integrity and reload behavior. [ADR 0008](../reference/adr/0008-content-addressed-authoring/) records the decision.

The [First Ecology experiment](../reference/research/first-ecology-experiment/) records six finite-resource scenarios, local CPU timings, replay, contact integration, and open measurement gaps.

The [Foundation 0.12 field-graph experiment](../reference/research/foundation-012/) records direct WGSL graph evaluation, CPU/GPU readback, two real captures, numerical edge cases, and bounded scaling measurements.
