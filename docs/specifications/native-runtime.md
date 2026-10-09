# Native runtime lifecycle (Foundation 0.9.6)

## Scope and authority

The native `first-light --passage` window presents a derived analytic render snapshot. `WorldState` and its fixed simulation tick remain authoritative. Camera changes, window resizing, surface reconfiguration, and GPU buffer rebuilds do not modify world state. This contract describes the current `wgpu 30.0.1` implementation, not a cross-driver guarantee.

## Startup and drawable size

Startup creates a winit window, `wgpu` instance, surface, adapter, device, queue, renderer, optional timestamp timer, and HUD. Initialization failure reports an error and exits. `GENESIS_WINDOW_SIZE=WIDTHxHEIGHT` sets the requested physical client size for local measurement; it must contain positive decimal dimensions no larger than 16,384. The actual adapter's `max_texture_dimension_2d` is checked before surface configuration and at each resize. The operating system may choose a different client size; the measured JSON reports the actual size.

An unchanged drawable size leaves the configured surface intact. A changed valid size reconfigures only the surface; the renderer manages its size-dependent output texture on draw. A zero-size window is skipped until a valid size returns. Minimize can produce either a zero size or an occluded surface; neither advances a rendered frame. Simulation time still follows the event loop's fixed-step accumulator when redrawing continues. Camera rays use the current drawable dimensions. Invalid oversized dimensions are reported and skipped, with no attempted GPU texture allocation.

## Surface and device outcomes

| `CurrentSurfaceTexture` outcome | Policy |
| --- | --- |
| `Success` | Draw and present. |
| `Suboptimal` | Draw and present, then reconfigure. |
| `Outdated`, `Lost` | Reconfigure the existing surface and retry on a later redraw. Exit after three consecutive failed acquisitions. A successful acquisition or actual size change resets the count. |
| `Timeout`, `Occluded` | Skip this presentation attempt; keep resources and try again on a later redraw. |
| `Validation` | Report and exit. |

The device error callback sets a flag. The next redraw reports the device error and exits before submitting more GPU work. **Device recreation is not implemented.** Surface reconfiguration is not surface recreation or device recovery. A real driver reset was not induced; the unit fault test verifies only the explicit classification and retry policy.

GPU snapshot overflow, validation, allocation, or draw errors remain explicit errors; they do not silently omit analytic primitives. Closing the window requests event-loop exit. Neither the renderer nor the HUD owns simulation identity.

## Measurement contract

Set `GENESIS_MEASURE_SCENARIO=idle|growth|interaction` with `--passage` to run fixed, repeatable local scenarios. This also enables `GENESIS_MEASURE`. Input keys and mouse buttons are ignored in this diagnostic mode except Escape. `idle` pauses at the initial tick; `growth` moves the source once; `interaction` moves the source, toggles it off/on, prunes a branch, moves/stops the body, and restarts every 180 scenario ticks. Ordinary play has no scripted events. The scenario is a performance workload, not a deterministic frame-timing promise.

Each `native-frame-window` JSON line summarizes about two seconds. Arrays are `[p50,p95]` milliseconds; `interval_ms` and `loop_ms` also include p99. `interval_ms` is the elapsed wall time between redraw entries and is an observed presentation *request* interval, not a display scan-out timestamp. `loop_ms` covers one redraw including surface acquisition, CPU encoding/submission, presentation call, and optional timestamp readback. `acquire_ms` can include Fifo/vsync waiting. `encode_submit_ms` is CPU time through renderer submission, not GPU execution; `present_call_ms` is only the CPU call. `step_ms` includes fixed-step environment allocation, contact and growth; `snapshot_ms` is the CPU copy into `Scene`; `upload_enqueue_ms` and `bvh_build_ms` come from the renderer's latest upload report, with zero when reused. `gpu_compute_ms` uses optional GPU timestamp readback sampled roughly every 30 frames. Its sample count is separate and often too small for reliable p95. A missing timer gives zero samples, not a measured zero duration. Input-to-photon latency is not instrumented.

The fixed-step accumulator caps catch-up at four steps per redraw and drops excess debt after sustained slow frames. A restart during a step clears the accumulator; the redraw loop must not subtract a step afterward. This preserves memory bounds but means simulation can run slower than wall time under sustained overload. Timing windows do not constitute benchmark results on other GPUs or operating systems.
