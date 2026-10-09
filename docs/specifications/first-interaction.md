# First Interaction: authoritative branch pruning

Foundation 0.7.1 additionally maps the kinematic body to `SemanticTarget::Body`; selecting that target cannot authorize a branch cut. See the [contact contract](kinematic-contact.md).

**Status:** experimental Foundation 0.7 vertical slice. `WorldState` remains authoritative; CPU picking reads a `Scene::from_world` snapshot and cannot mutate it. GPU traversal only draws that snapshot.

## Semantic identity and picking

`EntityId` identifies ordinary spheres, organisms, and sources. Each organism assigns monotonically increasing local node IDs. Active nodes stay in allocation order, but pruning leaves gaps; a node ID is never a vector index after a cut. A connection belongs to its child ID. `SemanticTarget` distinguishes ordinary sphere, growth-node sphere, child connection, and resource-source marker. The numeric `Primitive.id` is a sequential, snapshot-local renderer ID and must not be saved as an event target; `SemanticTarget` is the authoritative mapping.

`Scene::pick_pixel` uses the camera's pixel-center ray and an explicit maximum distance; `pick_ray` accepts an already bounded `Ray`. Both use existing `f64` analytic primitive intersections. Picking builds the CPU BVH and checks its nearest result against CPU direct traversal; disagreement returns `Indeterminate`, including a BVH miss where direct hits. The selected hit contains semantic target, world distance, hit position, and outward normal. Exact equal distances use snapshot insertion order. Different semantic targets whose distances differ by at most `max(1e-8, distance × 1e-7)` world units return `Ambiguous`; no event is authorized. This threshold is a local interaction policy, not a certified uncertainty bound. Coincident and near-coincident surfaces can be hard to select. CPU `f64` and GPU `f32` need not agree near tangencies or close depths; GPU highlighting, if added later, cannot authorize a cut.

Clicks outside a nonzero viewport and invalid pixel coordinates do not select anything. Manual renderer-only `Scene` primitives have no world target and return `Indeterminate` when hit. Rebuilding the scene after a tick is required before picking the new geometry. The native viewport stores the camera used for its last drawn frame and invalidates it on orbit and resize until redraw.

## Event and pruning rules

`EnvironmentEventKind::PruneBranch { organism, child }` is the typed event. `child` must identify an active non-root node of that organism. Selecting either its sphere or its parent-child capsule queues the same cut; selecting the root, a source, an ordinary sphere, or an ambiguous hit cannot. `Runtime::schedule` assigns a future tick and monotonic order. It rejects absent/root targets and pending cuts of an ancestor or descendant. A stale target is checked again at application. Manual duplicate events on one tick fail the whole tick atomically.

At each fixed step, events apply in order on a cloned world **before** source sampling, organism growth, and ordinary sphere growth. Pruning removes the child and every descendant, including their node spheres and connection capsules. Removed local energy is discarded; the organism's shared budget is retained. Surviving IDs, positions, energy, and the lifetime ID allocator do not change. Freed child slots can grow again under the existing resource rule; newborn IDs come from `next_node_id`. A maximum of 48 active nodes and 1024 lifetime allocations per organism are supported. Exhausting the lifetime allocator returns `IdExhausted` rather than reusing an ID. The root cannot be pruned. Detached rigid bodies are not created.

World and tick commit only after all events, resource sampling, growth, and validation succeed. Applied events leave the runtime queue only after success. A pending cut remains in a save. The viewport draws a fresh snapshot after the successful tick; packed-geometry change invalidates and rebuilds the renderer's GPU BVH. Neither snapshot nor BVH changes `WorldState`.

## Native workflow

Run `cargo run -p first-light --locked -- --life`. Orbit with A/D or Left/Right, tilt with W/S or Up/Down, and pause/resume with Space. Left-click a visible node or connection; the window title and stderr show the current tick and selected semantic target. Press P to queue pruning for the next completed tick. If paused, resume to apply it. M still queues a resource-source move. The title reports a queued cut, and stdout/stderr reports selection errors, ambiguity, and applied cuts. There is no persistent highlight or editing framework.

## Persistence and numerical limits

Version-3 JSON saves store `next_node_id`, sparse active nodes, current tick, budget, sources, and pending typed events. Versions 1 and 2 load by initializing the old dense allocator from their node count; new version-3 saves require an explicit valid allocator. Malformed topology or event references are rejected before replacing a running state. Same-build replay compares complete runtime state exactly. Cross-platform bitwise identity remains unproven. The initial click can become stale before its scheduled tick; that event fails explicitly and leaves world/time unchanged.
