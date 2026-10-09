# Foundation 0.9.2 — local quality and public-integration record

Status: **experimental local evidence**, 2026-10-09. Baseline `6bdd63d2c1074f6527ba4d49734ea47a198a1b81`; reported earlier baseline `c794898` is an ancestor. The user reported a complete **7/7 manual playthrough** on the reference machine. That report is not an agent-run native input test and does not fill the [manual QA checklist](../guides/playtest-qa.md).

## Method and gameplay results

The new application tests call the same fixed-step method used by the window loop, with application-level key state and actions. Branch selection is obtained from a real CPU analytic ray pick before it is supplied to the application state. The existing headless runtime test still performs collision-aware movement, a rejected root cut, source relocation and prune on the same tick, save/load, replay, and victory. Tests print structured `scenario` and `result` records with `--nocapture`.

| Scenario | Evidence |
| --- | --- |
| Successful route, collision before cut, save/load and replay | Existing `passage_playthrough_replay_and_restart` test; reaches the goal after source move and cut. |
| Wrong organism cut | Existing `cutting_other_organism_does_not_clear_passage` and new application test retain the central blocker. |
| Correct cut, source movement before and after cut, same-tick movement | Application `passage_picking_actions_and_highlight` test verifies the authoritative world after each fixed step. |
| Pause while moving, focus loss, partial and post-victory restart | Application `passage_input_pause_focus_and_restart` test verifies ticks, position, held keys, reset state and win lock. |
| Growth and removed selections | `growing_topology_keeps_only_live_semantic_selection` verifies stable node IDs remain actionable across growth; a removed branch loses its highlight and is rejected by pruning. Growth alone does not invalidate an unchanged semantic ID. |

These tests inject **application-level** input state. They do not send Windows mouse or keyboard events through a real window. Native input automation was not run in this environment; the available computer-control interface does not expose native Windows controls, and synthetic foreground input would risk interacting with another open application. Escape, real camera keys, pointer focus and actual native frame pacing remain manual QA items.

## GPU images and readability

The opt-in `passage_gpu_state_images_match_direct_and_bvh` test renders seven real offscreen states at 640×360: initial, selected, source moved, pruned, victory, restart, and dense overlapping analytic spheres. It compares all RGB bytes from direct and BVH traversal on one adapter, plus 15 ray identities and hit depths per state. It tests initial geometry at 320×180 and 800×600 as additional aspect ratios. All seven direct/BVH image comparisons had zero differing pixels; restart matched initial exactly. Captures are written only when `GENESIS_PASSAGE_CAPTURE_DIR` is set and remain under ignored `target/` locally. No binary reference images enter Git.

Local adapter: NVIDIA GeForce RTX 4070 SUPER, Vulkan, NVIDIA driver 591.86, Windows 11 Pro build 26200, Rust 1.94.0. The 640×360 BMP captures are `target/passage-captures/passage-<state>.bmp`; initial SHA-256 `0F4AE5CE395402A8B1355B42BAB77C30DCDD19FC3D4B7DD4294D26180AB99A9E`, selected `E85B5DDDEF9765963792BEF989CB041C2748578181724C18FBD196D4706D449E`, restart equal to initial. These hashes identify this local capture, not a portable cross-GPU golden image.

The selected branch now recolors its **existing authoritative node and connection** in the disposable render snapshot. Picking, bounds, contact and `WorldState` remain unchanged. Visual inspection of the actual initial, selected, pruned and victory BMPs found no missing geometry, but the selected lower stem still has few exposed pixels behind blue wall spheres. A CPU picking sweep at yaw −0.6, −0.3, 0, 0.3 and 0.6 radians found 0, 8, 18, 3 and 0 selectable pixels respectively at 160×90; the original yaw 0 remained the best of those candidates. Selection feedback is clearer after a successful click, while preselection discoverability remains a usability limitation.

## Release-profile measurements

Offscreen GPU measurement: ten post-warmup samples per case, BVH traversal, explicit GPU timestamp readback. `total` includes draw setup, queue submission and waiting for the GPU timestamp; it excludes a window surface, HUD, presentation and frame pacing. The growing scene was produced by 120 fixed steps after source movement. These are local diagnostics rather than full player frame times.

| Scene | Primitives | Resolution | Total p50 / p95 ms | GPU compute p50 / p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Initial | 67 | 1280×720 | 0.225 / 0.285 | 0.064 / 0.064 |
| Initial | 67 | 1920×1080 | 0.294 / 0.350 | 0.134 / 0.134 |
| Initial | 67 | 2560×1440 | 0.407 / 0.449 | 0.233 / 0.233 |
| Growing | 107 | 1280×720 | 0.225 / 0.270 | 0.071 / 0.072 |
| Growing | 107 | 1920×1080 | 0.313 / 0.345 | 0.150 / 0.151 |
| Growing | 107 | 2560×1440 | 0.455 / 0.565 | 0.262 / 0.263 |

`--passage-measure` on the same release build reported CPU fixed-step p50/p95 0.0058/0.0119 ms, isolated contact BVH build 0.0056/0.0058 ms, contact query 0.0008/0.0009 ms, and snapshot 0.0033/0.0054 ms. These isolated samples are not additive to the offscreen GPU figures. Native full-frame median/p95, frame pacing, and process memory under play remain unmeasured.

Reproduce with `cargo test --release -p first-light --locked passage_gpu_resolution_measurement -- --ignored --nocapture`, `cargo run --release -p first-light --locked -- --passage-measure`, and, for captures, set `GENESIS_PASSAGE_CAPTURE_DIR` then run `cargo test -p first-light --locked passage_gpu_state_images_match_direct_and_bvh -- --ignored --nocapture`.

## Dependency licenses and Windows package

The Windows runtime closure still has 89 registry packages and 172 copied license/notice files. `profiling 1.0.18` declares `MIT OR Apache-2.0`, reached through `wgpu`, `wgpu-core` and `wgpu-hal`; `spirv 0.4.0+sdk-1.4.341.0` declares `Apache-2.0`, reached through `naga`. Cargo's dependency graph confirms compilation dependencies, but does not prove which symbols survive linking into the EXE. The package conservatively includes both in its notices.

Both published crate archives omit a top-level license file. The crates' `.cargo_vcs_info.json` ties them to upstream commits [profiling `8271551`](https://github.com/aclysma/profiling/tree/8271551172eb6fa4cba47369aedd93790c623df9) and [rspirv `8afc3d0`](https://github.com/gfx-rs/rspirv/tree/8afc3d0ac8e158128cd1410bb2e4b4c26ab11bb4). At those commits, profiling has `LICENSE-APACHE` and `LICENSE-MIT`, rspirv has `LICENSE`, and neither repository root has a `NOTICE`. The packaged SPDX Apache-2.0 fallback for `spirv` matched upstream `LICENSE` exactly after trimming; profiling's upstream Apache text matched after whitespace normalization. The fallback is clearly labeled, and the package identifies each crate and upstream URL. No additional license text was found necessary for the Apache option. This is an engineering redistribution audit, not legal advice; a public binary should receive maintainer review.

The local static-CRT package was built from a modified tree and extracted to a temporary path containing spaces. The extracted `ThePassage.exe --passage-measure` exited 0. The ZIP had 181 files and no debug artifacts; its SHA-256 was `53160F5207173D0DEF987C3E11B7FFC47A5C71B8F113B50087080E103CF29B7B` (before the new sidecar script was rerun). PE imports were Windows system DLLs (`kernel32`, `user32`, `gdi32`, `dxgi`, `setupapi`, `ole32`, `dwmapi` and related system APIs), with no dynamic MSVC CRT import. The package script now emits a `.zip.sha256` sidecar; the next package build verifies this addition. A second clean computer, unavailable-GPU launch, and native `PLAY.cmd` interaction were not tested.

## Public integration boundary

Public GitHub API results for baseline `6bdd63d` on 2026-10-09: Linux `ubuntu-24.04` CI, Windows `windows-2022` CI, documentation/site, and release-governance workflows completed successfully. No PR was open at query time. The unauthenticated rulesets endpoint returned no visible rulesets, so branch protection and release environment configuration still require administrator verification. This feature branch has no hosted CI result until it is pushed and reviewed. Its CI addition builds and smoke-tests the portable ZIP on Windows PRs; PR workflows do not receive release publication secrets. No push, release or Pages deployment was performed for this work.

## Local verification inventory

`cargo fmt --all -- --check`, strict workspace Clippy, locked workspace build and test, locked rustdoc, and `cargo deny check advisories licenses` passed. The ordinary workspace run had **83 passed, 0 failed, 22 ignored**. Running all ignored tests serially in release mode had **22 passed, 0 failed**. Of those 22, 18 need a native GPU (16 renderer tests and two Passage tests); four are CPU timing experiments. Thus the earlier phrase “17 opt-in GPU tests” counted the renderer's CPU BVH benchmark among its 17 ignored tests. GPU tests are separate because ordinary Linux/Windows CI must remain headless and hardware independent.

`actionlint 1.7.7`, PR-title self-test (including `build(deps)`), Astro build and 6,574 local links under `/Genesis/` passed. The site still emits existing i18n, 404-route and sitemap warnings without failing its build. The new Windows packaging CI step has passed local script and extracted-binary checks, but has **not** run in hosted GitHub Actions. Security scanning is scheduled or manually dispatched; it is not a PR job. No native OS-input or real-window frame-time result is claimed.
