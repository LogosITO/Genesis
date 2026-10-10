# Experimental mathematical surface materials

`analytic-renderer::material` separates surface appearance from geometry.
`Material::sample(world_position, simulation_seconds)` returns linear RGB base
color and emission. The supported operations are constant color, a clamped
linear gradient, and alternating half-cycle bands. Positions are world units
in the existing right-handed coordinate system. The axis is a unit world-space
direction, frequency is cycles per world unit, and phase is dimensionless.
Time is the caller's deterministic simulation time in seconds, not wall time.
There are no UV coordinates, texture maps, arbitrary WGSL, or user shaders.

Colors lie in `[0,1]`; emission components lie in `[0,4]` before display
clamping. Axis length must be within 0.001 of one, frequency `[0.001,10]`,
phase magnitude at most 1000, speed magnitude at most 10 cycles per second,
and simulation time `[0,1,000,000]`. All inputs must be finite. The CPU
reference and WGSL evaluator use `f32`; small rounding differences near band
transitions are expected. No physically based color management is claimed.

The display model uses an ambient factor 0.18 and a directional Lambert factor
0.82 toward normalized `(0.5,0.8,-0.6)`. Emission is added after diffuse
lighting and then clamped to the output range. It visibly brightens the
surface; it does not illuminate neighboring objects or create indirect light.
Diagnostic uncertainty colors override material appearance.

`MaterialSet` assigns materials by stable `EntityId` to ordinary world spheres
and world-owned field occurrences. A missing assignment retains the existing
renderer color. Other analytic kinds keep their existing colors in this slice.
Assignments are presentation data and do not change `WorldState`, field
definitions, contact, or BVH geometry. A material-only update can change
material/index buffers while reusing graph nodes and analytic BVH nodes.

The opt-in GPU test
`mathematical_materials_match_cpu_and_reload_without_geometry_upload` checks
CPU/GPU center pixels, invalid inputs, changed appearance, shared graph
definitions, and unchanged graph/BVH upload counters. It writes an actual GPU
capture under `target/mixed-captures/materials.bmp`. External JSON material
authoring and transactional scene reload belong to the separate World Lab
workflow; this typed API alone does not constitute that workflow.
