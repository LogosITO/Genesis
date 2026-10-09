# RFC: mathematical world authoring beyond static grammars

**Status:** research proposal for richer authoring. The bounded grammar and the first world-owned static occurrence are implemented; see [the v1 grammar](../specifications/mathematical-authoring.md) and [instance contract](../specifications/authored-world-instances.md).

## Context and implemented boundary

Genesis already has separate analytic primitive, exact signed-distance, general signed-field, and ideal distance-bound contracts. A structural grammar rewrites symbols into a graph of segments; evaluating it is not a scalar-field sample. Existing living organisms have bounded topology, stable lifetime node IDs, finite resource accounting, pruning, collision and versioned saves. Directly inserting an arbitrary grammar graph into `Organism` would bypass these rules, so v1 does not do that.

The preview path is `JSON source bytes → SHA-256 revision and validated Definition → bounded derivation paths → typed Structure { Segment } → disposable renderer Scene`. Foundation 0.11 additionally stores exact definitions and distinct occurrences in `WorldState`, with `world → authoring` as a CPU-only dependency. The preview remains separate and reloadable. Neither path converts authored structures into ecological organisms.

## Implemented minimal world instance contract

`WorldState` now owns a stable `EntityId`, validated translation and positive uniform scale, pinned exact-byte definition revision, enabled/solid state and generation for each occurrence. The authored rules remain immutable source data. Generated segment paths identify topology only within that exact revision; world targets add occurrence ID and generation. Definition replacement is explicit and atomic at the CPU state level, invalidates old segment references and preserves only occurrence-level enabled/solid/transform state. Matching draw indices or paths across revisions is not treated as migration.

Collision queries use the same transformed analytic capsules as rendering, with individual-capsule contact rather than a regularized union. Version-6 saves embed exact definition bytes once per revision and validate them before accepting occurrences or pending events. Incremental L-system growth would need a deterministic frontier, bounded per-step work, stable instance-local allocation and a rule for environmental inputs; eager static expansion remains the only implemented path. The existing `Organism` limits and ecological resource accounting are not bypassed.

## Proposed later representation

Keep distinct typed families with explicit contracts rather than one universal `Field` trait:

1. **Structural generators:** deterministic or seeded stochastic grammars and IFS produce bounded typed graphs or lazy hierarchical nodes, with stable source identity and instance parameters.
2. **Continuous evaluators:** scalar fields, signed inside/outside fields, exact SDFs, and conservative distance bounds retain separate types and numerical guarantees. A typed expression graph could compose only operations whose contracts are preserved.
3. **Parametric geometry:** curves and surfaces provide bounded evaluation domains, derivatives where available, and explicit spatial bounds. A curve's rendered tube could become a capsule chain only as a documented approximation, while its source curve remains authoritative.
4. **Material fields:** color and other appearance values are sampled separately from geometric membership and distance.
5. **Runtime instances:** a versioned definition plus validated parameters can be instantiated in `WorldState` only after identity, mutation, collision, persistence and replay semantics are specified. Render snapshots and acceleration structures remain derived.

GPU compilation or interpretation is a possible optimization for supported typed graphs, not the definition language itself. CPU evaluation and queries must remain possible without a GPU. Interval evaluation and conservative bounds may support culling and stepping, but their guarantees require separate proofs or targeted tests. IFS and stochastic grammars need explicit seeds, order, depth and work budgets before introduction.

## Alternatives and risks

- Treating all objects as SDFs would misstate CSG and structural-grammar guarantees; rejected.
- Baking authored structures into meshes/voxels would replace the mathematical source of truth; rejected.
- Mapping grammar segments directly to living `GrowthNode`s would violate the current 48-active-node limit and resource/lifecycle rules; deferred until a deliberate world-schema design.
- Rendering only a precomputed file of coordinates would not exercise rewriting; rejected.
- Eager expansion is simple and bounded now, but the [local scaling experiment](../research/foundation-010.md) shows BVH build and snapshot cost rising with segment count. Lazy or hierarchical evaluation needs evidence from larger valid worlds before implementation.

Open research: justified correspondence under rule edits, collision semantics for overlapping generated capsules, exact or conservative bounds for composed fields, GPU/CPU numerical agreement for expression graphs, revisioned save migration, and safe community content distribution. No general authoring language, ecological grammar growth, or arbitrary shader execution is claimed.
