# Foundation 0.13.1: reproducible field authoring fixtures

**Status: partial.** The `--inspect` command validates source and reports graph
identity, revision, node/work counts, distance semantics and outward bounds as
JSON without requesting a GPU. The existing bounded JSON editor workflow compiles
before replacing an active graph. In `--watch` mode, invalid source leaves the
previous preview active. The executable prints source revision, graph node and
expanded-visit counts, outward bounds, distance semantics, CPU/GPU centre-ray
diagnostics and bounded solid-query work. The world fixture additionally prints
stable occurrence IDs and outcome counts. There is no graphical editor or
selected-instance widget yet.

Two checked fixtures make specific failure modes reproducible:

- `shared-face.json`: two adjacent unit boxes. `F(0)=0`, while points immediately
  left and right are occupied; the shared face is internal. GPU output was
  generated on the local NVIDIA GeForce RTX 4070 SUPER/Vulkan adapter. Its
  640×480 frame took 7.40 ms including upload and readback in one debug run.
- `balanced-16.json`: a 4×4 array of spheres using exactly 32 graph nodes and
  47 expanded visits per sample. All 16 centres sample to `-0.35`. The actual
  GPU capture was inspected and shows the complete grid. Its 640×480 frame
  took 19.96 ms including upload and readback in one debug run. The centre ray
  passes between spheres; the CPU solid query returns
  `Indeterminate(UncertifiedAdvance)`, preserving numerical honesty.

These single-run times are diagnostic only. The next authoring work is a
world-instance editing surface, selected identity, and a measured reload path
with stable GPU buffers. No claims about scalability or cross-adapter parity
follow from these captures.
