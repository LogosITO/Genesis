# Foundation 0.12.3: bounded spatial-query diagnostics

**Status: partial.** The existing exterior-entry query now reports its actual CPU
sample calls and charged worst-case expanded graph visits. The outcome-only API
delegates to the same implementation; diagnostics cannot change its classification.
The example emits the outward graph bounds, query state and work counters.

Tests cover the first entry of spheres, boxes, transforms and unions against
independent roots; interior and zero-set starts; overlapping and coincident
solids; an internal zero face; a near tangent; strict bounds exclusion; work
exhaustion; and numeric failure. A seeded test compares 1,024 samples from
valid sphere-union graphs with independent sphere formulas. This is finite
coverage, not a floating-point proof for arbitrary graphs.

`BoundsMiss` is the only confirmed exclusion under the narrow CPU assumptions
in [the field contract](../specifications/field-graph.md). Inside starts, a
ray that misses a sphere but overlaps its AABB, and budget exhaustion remain
explicitly indeterminate. There is no certified first-exit or physical-contact
query. The report charges the validated maximum visits per sample, not actual
node visits; measuring the latter would require instrumentation in the graph
evaluator. No faster spatial path was added, so a baseline/improved CPU speedup
would be a false comparison. The next useful performance comparison requires
an actual acceleration algorithm and representative queries.
