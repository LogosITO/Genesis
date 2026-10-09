# Foundation 0.12.4: restricted moving-sphere contact candidate

**Status: partial / experimental.** A read-only CPU query traces a positive-radius
moving sphere against the supported Field Graph as an exterior point query of
`F-r`. It reuses the bounded first-entry traversal and returns an approximate
travel distance, a strict expanded-bounds miss, or explicit uncertainty. No
field graph participates in authoritative `WorldState` contact.

The focused tests exercise a 20-unit sweep across a sphere, an 0.2-unit-thick
box, an initial overlap, an exact zero start, a within-AABB miss, a tangent,
budget exhaustion and invalid input. A separate differential test compares
three one/two-sphere union scenes with the existing `ContactScene` analytic
sweep; approximate time fractions agree within `1e-4` away from tangencies.
These are deterministic regressions, not certification of arbitrary graph
contacts. The existing analytic collider path remains the gameplay reference.

The query cannot report a certified contact: sample error, grazing roots,
underflow and normalization rounding are not enclosed. It does not supply a
surface normal, robust initial-overlap resolution, simultaneous-contact policy,
or sliding response. A zero displacement is unsupported. Cross-platform CPU
and GPU comparisons for this moving query have not been performed. These limits
block graduation to an authoritative collision API; an interval or direct
primitive contact method with a proof of reported outcomes is needed first.
