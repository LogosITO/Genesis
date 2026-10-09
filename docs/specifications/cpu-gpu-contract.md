# CPU/GPU primitive intersection contract

## Established knowledge and decisions

The authoritative `Sphere`, `AxisAlignedBox`, `Transform`, and `WorldState` are CPU `f64` values. The renderer is a bounded `f32` snapshot of those analytic parameters. GPU intersections use quadratic roots for spheres and slab intervals for boxes. They do not use `IdealDistanceBound`: its real-arithmetic proof is not a certified `f32` marching step. General CSG zero sets are not rendered as solids.

World space is right-handed, with user-chosen length units. A renderer ray has a normalized direction and a closed `[near, far]` interval in those units. The earliest boundary crossing in the interval is returned; an interior start returns the exit crossing. A ray starting exactly on a surface with `near = 0` can return distance zero. Tangent contact counts as a hit. Box face-parallel rays outside a slab miss. At box edges and corners, the first axis in X/Y/Z order determines a displayed normal; the geometric normal is not unique there. Equal-distance objects use insertion order. Normals point outward. No tolerance is silently applied to analytic hit classification.

Camera rays pass through pixel centers. `(0,0)` is the upper-left pixel. The perspective uses vertical field of view, positive camera-right and up basis vectors, and a finite default view segment `[0.001, 1000]`. In the sample camera located at negative Z and looking toward positive Z, screen-right points toward negative world X, as required by the chosen right-handed view basis.

## Supported input range

The renderer rejects nonfinite color and coordinates, coordinates outside `[-10,000, 10,000]`, primitive world dimensions outside `[0.0001, 10,000]`, duplicate IDs, and world IDs that do not fit `u32`. GPU primitive storage has an explicit application budget and adapter ceiling, reported by `PrimitiveCapacity`; see [render snapshot](render-snapshot.md). Colors are linear RGB in `[0,1]`. Positive uniform scale is baked into world radius or half extents. Rotation and nonuniform scale are not supported by the existing `Transform` model. The range is an input guard, not a precision guarantee: a 0.0001-wide feature near coordinate 10,000 cannot be resolved reliably in `f32`.

## Numerical limits

CPU `f64` and GPU `f32` can disagree near tangencies, surfaces, coincident hits, and cancellation in quadratic roots. WGSL implementations and hardware may differ. The renderer makes no cross-platform bitwise identity or formal hit/miss guarantee. A GPU miss is an analytic calculation in finite precision, not a proof that the ideal solid is missed. Extreme relative scales within the accepted absolute range are not comprehensively tested.

The opt-in parity fixture compares hit/miss and object ID exactly for chosen rays, distance within `max(0.0002 world units, 0.0002 × CPU distance)`, and normal dot product at least `0.999`. These are **test acceptance tolerances**, not runtime safety margins or universal error bounds. Cases include center, tangent and near-tangent sphere rays, interior and surface starts, box-parallel hit/miss, overlap and depth ordering, and radius `0.001` and `1000`. CPU `field::trace` is checked separately on exterior non-grazing primitives; it has different semantics for interior and tangent starts and does not return an ID or normal. Its `Indeterminate` status is not converted to a GPU miss.

## Foundation 0.5 capsule extension

The renderer also copies analytic capsules as endpoint A, endpoint B, and radius in `f32`. CPU and WGSL intersect the finite cylinder side and two hemispherical caps. The [capsule contract](capsule.md) defines the field and supported domain. The opt-in parity fixture exercises side, cap, interior, tangent, near-parallel, seam, and degenerate rays with distance tolerance `0.0003` world units and normal dot product at least `0.998`. These selected-ray tolerances are not numerical certificates. Overlapping node spheres and connection capsules are drawn as separate nearest-hit primitives; no watertight CSG union is claimed.

The authoritative world permits 512 ordinary spheres and up to 4 × 48 growth nodes. A complete four-organism scene with one source has 381 render primitives and fits the tested reference adapter's dynamic storage budget. Larger scenes can fail explicitly at the configured GPU budget or device limits; there is no silent omission. Growth node IDs use the 1,000,000 range, connection IDs the 2,000,000 range, and resource marker IDs the 3,000,000 range, derived from stable world and node identities. See the [growth model](growth-model.md) and [First Scale measurements](../research/first-scale-benchmark.md).

## Open questions

Numerically certified conservative steps, robust intersections over larger dynamic ranges, CSG solid-boundary semantics, GPU field lowering, cross-adapter parity, and high-object-count acceleration remain research tasks. Any future change must retain explicit uncertainty rather than silently treating iteration exhaustion as a miss.
