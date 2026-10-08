# Mathematical field contracts

`ScalarField` returns a scalar with no implied geometry. `SignedField` adds the convention: negative inside, zero boundary, positive outside. `ExactSdf` marks a signed field whose magnitude is exact Euclidean distance to the primitive surface. `Sphere` and `AxisAlignedBox` implement it in local space. Positive uniform scaling in `Entity::signed_distance` preserves this property.

`Field` combines primitives with `min(a,b)` for union, `max(a,b)` for intersection, and `max(a,-b)` for difference. These formulas preserve sign-set semantics for the regular inputs used here. A CSG result implements `SignedField`, **not** `ExactSdf`: near overlaps, its magnitude can differ from the distance to the composite boundary. No conservative distance estimator is exposed in 0.1. Such an estimator would need a separately proven bound and a specified query region before a ray marcher can rely on it.

All constructors and samples reject nonfinite inputs/results. The public `Vec3` constructor enforces finite coordinates. Radii, box half extents, and transform scale must be strictly positive. Intervals have finite ordered endpoints and include both endpoints. A finite query can still overflow during evaluation; this returns `NonFinite`. The API has no epsilon or approximate equality policy. Deep recursive `Field` expressions may exhaust the call stack; compilation or iterative evaluation is future work.

Examples of excluded semantics: a sign field is not automatically a distance bound; a scalar density is not automatically an inside/outside classifier; a sampled image is not the geometry source of truth.
