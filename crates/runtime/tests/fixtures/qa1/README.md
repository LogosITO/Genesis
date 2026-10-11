# QA-1 historical save provenance

These bytes were emitted on Windows with Rust 1.94.0 by the **original producer
implementation** at each listed commit. Each commit was extracted with
`git archive` into ignored `target/qa1/producers/`; no branch was checked out or
rewritten. The historical `crates/runtime/tests/field_save.rs` was modified
only in that extracted copy: immediately after the selected test's
`runtime.save_bytes().unwrap()` assignment, a `std::fs::write` copied the
returned bytes to `GENESIS_QA1_FIXTURE`. A clone of that runtime was advanced
by three historical `tick()` calls and its bytes written to
`GENESIS_QA1_CONTINUATION`. The production serializer and original input state
were unmodified. The named historical test passed. Each version
used a separate Cargo target directory to prevent cross-checkout test-binary
reuse; a first attempt with one shared target directory produced **zero
selected tests** for v8–v10 and was discarded.

| File | Producer commit | Historical test | SHA-256 | Bytes | State |
| --- | --- | --- | --- | ---: | --- |
| `v7.json` | `0b1dfaa4808b0270e676e95cc919621c2396b740` | `field_world_round_trip_and_tamper_rejection` | `9ee0dbb60235afbcc56ffdbc5522acfafd6d3c50e7c3cb98950eb1f0a6832cc1` | 8,271 | One visual field, tick 1. |
| `v8.json` | `dd274bfe00bb9f7f4acb252c9ee96f97143890e0` | `dynamic_field_requires_v8_and_survives_repeated_save_load` | `868496260ec13654601a1384da08fa4681fb2ea9f00201ec73a54999d81f1ece` | 8,504 | One growing field, allocated 0.25 units, tick 0. |
| `v9.json` | `440b6e6effead1307db69f7eca2e18190fd6e73b` | `two_lobe_morphology_requires_v9_and_rebuilds_exact_source` | `6027a0c09b08f6d46d49c510f93fd06cba880c98a406f79aed90e8f9e51aedea` | 10,166 | One two-lobe field, allocated `[0.5, 0.25]`, tick 0. |
| `v10.json` | `81ea2c0f0ff6d8f09229684d0aa7f7519582d47a` | `generic_parameter_world_uses_one_ledger_and_requires_v10` | `d4cc41a305623a0a36b3509f7e3dea8c750036bdfc1619a597a5896a00e84b0f` | 14,880 | Two parameterized fields, one finite source, tick 1. |

| Historical continuation | SHA-256 | Bytes |
| --- | --- | ---: |
| `v7-after3.json` | `c0f330ced66280b7f6a8f2b852ed57a0d83ff3acab8a6b4b5eaf5d2b976c8144` | 8,271 |
| `v8-after3.json` | `d7ee3a1fc689c07e5f77371ad8e3dafd295ec6348f53f62124d56b8b3805375f` | 8,504 |
| `v9-after3.json` | `47ee9be7c39b14379307d0fa76cbbad960e431b84a2a96736d762d160fc6210e` | 10,166 |
| `v10-after3.json` | `9d07158487bb94af9a49939569723a438f566c29984856e511f53bb02d4a76fa` | 14,849 |

The current-reader regression is
`cargo test -p world-runtime --test historical_saves --locked`. It compares
current three-tick continuation to each producer's continuation as full
validated runtime state, then checks re-save, truncation and downgrade rejection.

These fixtures prove compatibility for these four representative historical
states. They do not exhaust every legal historical save, platform-specific
floating-point continuation, or all corrupted-input classes.
