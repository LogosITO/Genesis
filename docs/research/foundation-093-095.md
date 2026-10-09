# Foundation 0.9.3–0.9.5 local research record

## Established knowledge

The Passage uses separate CPU analytic picking, CPU swept-sphere contact, and GPU analytic visualization. Geometry and world state remain authoritative on CPU. A goal marker is visual only and has no gameplay collider.

## Hypotheses and methods

The central stem appeared difficult to select because of both wall occlusion and small screen projection. A diagnostic reduction of wall sphere radius did not raise its 160 × 90 default-camera selectable-pixel count above 18. A yaw sweep at −0.6, −0.3, 0, 0.3, and 0.6 radians exposed 0, 8, 18, 3, and 0 such pixels. The radius experiment was reverted. The selected design emphasizes the existing visible geometry and reports strict occlusion rather than allowing through-wall selection.

Seven deterministic offscreen Passage states at 640 × 360 were rendered with direct and GPU BVH traversal on NVIDIA GeForce RTX 4070 SUPER / Vulkan. Each state had zero differing pixels between the two GPU paths. The locally captured initial and selected BMPs are in `target/passage-captures-093/`; this ignored directory is not a published fixture. A prior comparison on the same scene found 274 changed initial-frame pixels, with mean brightness increasing from 71.4 to 119.0; selected-branch changed pixels increased from 100.5 to 180.3. These measurements describe only the chosen capture and adapter. The green lower branch remains small in the initial view.

Release-profile offscreen measurements used ten post-warmup samples per case and GPU timestamp readback on that adapter. The full offscreen duration includes draw setup, submission, and synchronization, but excludes window presentation, HUD, and frame pacing. The GPU compute medians for the initial 67-primitive scene were 0.064, 0.135, and 0.234 ms at 720p, 1080p, and 1440p. The 107-primitive growing scene measured 0.072, 0.151, and 0.263 ms. The corresponding offscreen total medians were 0.335/0.337/0.441 ms and 0.271/0.354/0.473 ms. The prior 0.9.2 GPU compute medians were 0.064/0.134/0.233 ms and 0.071/0.150/0.262 ms, respectively. The small differences are within the limits of this ten-sample diagnostic; no performance improvement or regression is established. Full-frame presentation remains unmeasured.

## Decisions

CPU `pick_ray` remains the selection authority. A branch is strictly occluded only when its first analytic intersection lies beyond a different unambiguous semantic hit by more than the pick margin. A near tie is indeterminate. Visual overlap does not imply body contact; the analytic body sweep and its collider set remain unchanged. The source switch is a fixed-tick event, so the game can pause resource input without adding a new simulation subsystem.

## Limitations and open questions

Captures prove offscreen compute output on one local adapter, not window presentation, interactive Windows input, or other GPU drivers. A selected stem behind walls still cannot be seen or clicked. A contour pass may be useful if external playtests show the visible accent is insufficient. Surface recovery from a `Lost` result can be reconfigured, but actual device loss and presentation behavior need real hardware testing. Hosted CI and the portable Windows package must be checked against the final commit before any 0.9.5 completion claim.
