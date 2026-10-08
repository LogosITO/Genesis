# Coordinate system and units

Version 0.1 uses a right-handed Cartesian 3D coordinate system. `x`, `y`, and `z` are arbitrary world length units; users must choose a consistent unit (for example metres). Simulation durations are seconds and growth rates are local radius units per second. The origin is `(0,0,0)`.

`Transform` provides translation and strictly positive uniform scale. A world point is mapped to local coordinates as `(point - translation) / scale`; an exact local SDF sample is multiplied by scale to express world distance. Rotation and nonuniform scale are absent. Large finite inputs can overflow intermediate arithmetic and return `NonFinite`.

Coordinates use IEEE-754 `f64`. Construction rejects NaN and infinities. No tolerance is silently applied at surfaces: zero is boundary and counts as inside. Tests use exact values only where binary representation makes that appropriate. No cross-platform bitwise identity is promised.
