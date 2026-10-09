# Analytic primitive bounds

Bounds are renderer acceleration data, not a geometry representation or an authoritative world property. They use world units and closed intervals on X, Y, and Z.

- Sphere: `center ± world_radius` on each axis.
- Axis-aligned box: `center ± world_half_extent[axis]`.
- Capsule: `min(endpoint_a, endpoint_b) − radius` and `max(endpoint_a, endpoint_b) + radius` on each axis. Coincident endpoints remain a sphere-shaped capsule.

The primitive constructors validate finite coordinates, positive dimensions, and the renderer's supported range before bounds are built. Uniform scale is already applied to sphere and box parameters. The endpoint operations are rounded one `f64` value outward. Bounds containment and overlap include their boundaries. CPU BVH slab traversal also expands computed ray entry and exit by one representable value; a nonfinite slab quotient declines to reject the node.

These steps reduce false rejection from ordinary rounding, but do not establish a certified bound for every finite ray, primitive, or platform. The direct analytic path remains authoritative for renderer comparisons. No `f64` bounds are downcast to GPU culling data in Foundation 0.6.
