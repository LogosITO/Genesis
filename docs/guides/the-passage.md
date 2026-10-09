# The Passage — local playable prototype

**Experimental / Research Stage.** This small game uses Genesis's authoritative mathematical world. It is a local vertical slice, not a public release or a general game framework.

## Run

Build from source with Rust 1.94: `cargo run --release -p first-light --locked -- --passage`. On Windows, `powershell -ExecutionPolicy Bypass -File tools/package-passage.ps1` builds a portable ZIP in `target/dist/` and verifies extraction into a path with spaces. Extract the ZIP outside the repository and open `PLAY.cmd`. The ZIP includes a build ID and executable hash in `BUILD.txt`, an offline playtest card, a feedback template, the project licenses, and dependency license files. Neither Rust, Cargo, network access, nor source files are needed at runtime.

The only tested native configuration so far is Windows x86_64 with an NVIDIA GeForce RTX 4070 SUPER using Vulkan and Rust's `wgpu` 30.0.1. A Vulkan or DirectX 12 adapter and a suitable driver are required. Minimum GPU generation, VRAM, Windows version, and other adapters have **not** been established. The launcher keeps an initialization error visible if the GPU cannot start. Linux source compilation is configured in CI; the portable ZIP is Windows-only.

## Objective and controls

Move the orange sphere through the narrow passage to the magenta goal. The two colored structures grow from a shared finite resource. The green central stem blocks the route. **M** moves the yellow resource source away from that stem, **left-click** selects a visible green node or connection, and **P** cuts the selected branch at the next simulation tick. Then move through the gap. Victory requires the actual body centre inside the far goal region, the central first branch removed, and the source moved. The magenta sphere is a visible marker; it is not collision geometry.

| Input | Action |
| --- | --- |
| W/A/S/D or I/J/K/L | Move on the horizontal plane |
| Arrow keys | Orbit and tilt the camera |
| Left click | Select an analytic world target |
| P | Prune a selected non-root branch |
| M | Toggle the source between the central and outer locations |
| Space | Pause or resume fixed-step simulation |
| R | Restart the original scenario |
| Esc | Exit |

The HUD shows the selected target, source position (NEAR/AWAY), central stem state (BLOCKS/CLEAR), current status, pause state, and success. The window title shows the tick and node counts. Inputs are cleared on focus loss. A cut or source move queued while paused waits until simulation resumes. A stale, root, ambiguous, or missing target cannot authorize a cut. Cutting the blue organism does not clear the green route; the HUD says so. Restart discards pending actions, selection, and derived collision state. You can restart any time if growth or resource use makes the route hard to read.

## Engineering notes

The lane walls are ordinary analytic spheres and participate in swept-sphere contact. Organism nodes and connections are analytic spheres and capsules. The goal marker is renderer-only and does not affect contact. The body is restricted to constant `y = 0.35` by gameplay input; there is no gravity or vertical motion. The fixed step is 1/60 simulated second; rendering does not advance movement directly. Growth and resource use continue during play. The scenario starts after 45 deterministic growth ticks, so a branch is already present.

`Scene::from_world` builds a disposable renderer snapshot. Semantic picks map back to stable world and node IDs. Pruning and source moves use typed tick-indexed events. Contact uses the CPU analytic swept-sphere BVH; the GPU is used for display only. Save/load and replay are exercised by the headless playthrough test: `cargo test -p first-light passage_playthrough --locked -- --nocapture`. The test prints one JSON result on success. Player-facing save/load is not offered, avoiding ambiguous file overwrite rules.

Known limitations: the wall spheres overlap visibly, the lane can be walked around, and the demo's objective therefore requires the world-state changes as well as goal position. Contact has no sliding or depenetration. Overlapping organism primitives are separate solids, not a watertight union. The lower green stem can be partially hidden by the walls; use the arrow keys to change the view. CPU semantic picking confirms it remains selectable in the default view. A selected branch now highlights its existing node and connection in yellow, but this does not make a hidden branch visible before selection. Growth can fill the view over time; use **R** to reset. The user reported a complete 7/7 manual playthrough on the reference machine; the [manual QA checklist](playtest-qa.md) has not been independently completed by an agent or second tester. See the [0.9.2 research record](../research/foundation-092.md) for automated evidence and limits.
