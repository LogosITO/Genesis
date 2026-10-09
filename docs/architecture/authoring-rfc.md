# RFC: mathematical world authoring beyond static grammars

**Status:** research proposal. Only the bounded grammar and static capsule structure described in [the v1 contract](../specifications/mathematical-authoring.md) are implemented.

## Context and implemented boundary

Genesis already has separate analytic primitive, exact signed-distance, general signed-field, and ideal distance-bound contracts. A structural grammar rewrites symbols into a graph of segments; evaluating it is not a scalar-field sample. Existing living organisms have bounded topology, stable lifetime node IDs, finite resource accounting, pruning, collision and versioned saves. Directly inserting an arbitrary grammar graph into `Organism` would bypass these rules, so v1 does not do that.

The current path is `JSON source → validated Definition → bounded expanded symbol stream → typed Structure { Segment } → disposable renderer Scene`. Source bytes and typed segments remain available independently of GPU data. The preview converts segments to analytic capsules; it does not create a second mutable world. No engine-wide authoring dependency was added to math, field, world, simulation or runtime. Only the preview example depends on the new crate.

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

Open research: stable identity under rule edits, collision semantics for overlapping generated capsules, exact or conservative bounds for composed fields, GPU/CPU numerical agreement for expression graphs, revisioned save migration, and safe community content distribution. No general authoring language, ecological grammar growth, or arbitrary shader execution is claimed.
