# Replay and deterministic update contract

## Exact ordering

The first step applies events for completed tick 1. Each runtime schedules events at future ticks and assigns a monotonically increasing order number. At a tick, events apply in insertion order before resource evaluation. Repeated moves to the same source or `SetBodyVelocity` events therefore leave the last value effective. A `PruneBranch` event removes its target subtree before sampling, so surviving nodes alone participate in that tick's growth. Finite sources replenish, all active nodes form source-specific demands, and each source's stock is proportionally allocated across organisms before any organism grows. Organisms are read in allocation order, sources in insertion order, and old nodes in local-ID order. Growth proposals are applied to a cloned world, then existing ordinary sphere radii advance. The kinematic body is swept against the resulting geometry before world and integer tick commit together. Applied events are removed only after success. Conflicting cuts are rejected when scheduled or during validation; a stale event or uncertain collision aborts its tick atomically. See [finite allocation](resource-allocation.md).

One tick performs at most 16 events; the runtime holds at most 128 future events. Rendering is read-only and has no influence on the tick count. The native viewport caps catch-up at four fixed steps per draw and discards excess wall-time debt; this affects correspondence to wall time under sustained slow rendering, never the result of a given number of successful ticks and events.

## Reproduction

On the same Rust build, floating-point model, and event script, the fixed loop is sequential and has no PRNG or unordered iteration. The tests demand exact `Runtime` equality for repeat runs and for uninterrupted versus saved/restored continuation. The headless example records a deterministic FNV-1a hash of canonical save bytes for diagnosis; it is neither a cryptographic signature nor mathematical proof of equivalence. Machine-readable summaries include tick, node count, tip coordinates, and snapshot primitive count.

Different CPU architectures, compiler settings, and future code revisions may round `f64` operations differently. Cross-platform bit identity has not been established. For cross-platform comparison, inspect graph identities and topology exactly and use a declared coordinate tolerance appropriate to the experiment. No tolerance is used in the local continuation tests.
