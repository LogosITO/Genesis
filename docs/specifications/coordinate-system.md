# Coordinate system and units

The foundation uses a right-handed Cartesian 3D coordinate system. `x`, `y`, and `z` are arbitrary world length units; users must choose a consistent unit (for example metres). Simulation durations are seconds and growth rates are local radius units per second. The origin is `(0,0,0)`.

`Transform` provides translation and strictly positive uniform scale. Mathematically, a world point maps to local coordinates as `(point - translation) / scale`, and a local field sample is multiplied by scale to express world distance. Transformed spheres use the equivalent world-space formula `||point - translation|| - radius × scale` to avoid an unnecessary division. Rotation and nonuniform scale are absent. Large finite inputs can overflow intermediate arithmetic and return `NonFinite`.

Coordinates use IEEE-754 `f64`. Construction rejects NaN and infinities. No tolerance is silently applied at surfaces: zero is boundary and counts as inside. Tests use exact values only where binary representation makes that appropriate. No cross-platform bitwise identity is promised.
