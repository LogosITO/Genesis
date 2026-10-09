# The Passage — local playable prototype

**Experimental / Research Stage.** This small game uses Genesis's authoritative mathematical world. It is a local vertical slice, not a public release or a general game framework.

## Run

Build from source with Rust 1.94: `cargo run --release -p first-light --locked -- --passage`. On Windows, a portable package can be made with `powershell -ExecutionPolicy Bypass -File tools/package-passage.ps1`; open `PLAY.cmd` in the resulting directory. The executable needs a Vulkan or DirectX 12 adapter and a current graphics driver. It does not need a network connection or runtime assets. Linux compilation is covered by source CI; this package script is Windows-only.

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

The HUD shows the selected target, tick, current status, pause state, and success. Inputs are cleared on focus loss. A cut or source move queued while paused waits until simulation resumes. A stale, root, ambiguous, or missing target cannot authorize a cut. Restart discards pending actions, selection, and derived collision state. You can restart any time if growth or resource use makes the route hard to read.

## Engineering notes

The lane walls are ordinary analytic spheres and participate in swept-sphere contact. Organism nodes and connections are analytic spheres and capsules. The goal marker is renderer-only and does not affect contact. The body is restricted to constant `y = 0.35` by gameplay input; there is no gravity or vertical motion. The fixed step is 1/60 simulated second; rendering does not advance movement directly. Growth and resource use continue during play. The scenario starts after 45 deterministic growth ticks, so a branch is already present.

`Scene::from_world` builds a disposable renderer snapshot. Semantic picks map back to stable world and node IDs. Pruning and source moves use typed tick-indexed events. Contact uses the CPU analytic swept-sphere BVH; the GPU is used for display only. Save/load and replay are exercised by the headless playthrough test: `cargo test -p first-light passage_playthrough --locked -- --nocapture`. The test prints one JSON result on success. Player-facing save/load is not offered, avoiding ambiguous file overwrite rules.

Known limitations: the wall spheres overlap visibly, the lane can be walked around, and the demo's objective therefore requires the world-state changes as well as goal position. Contact has no sliding or depenetration. Overlapping organism primitives are separate solids, not a watertight union. Camera picking can be ambiguous near overlaps. Growth can fill the view over time; use **R** to reset. A manual end-to-end playtest on another Windows system and hosted Windows/Linux CI are still needed before a public playable claim.
