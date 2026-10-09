# Changelog

## [0.2.0](https://github.com/LogosITO/Genesis/compare/world-simulation-v0.1.0...world-simulation-v0.2.0) (2026-10-09)


### ⚠ BREAKING CHANGES

* **life:** Organism::grow now accepts per-node samples and returns the number of new nodes; new saves use format version 2.
* **life:** WorldState now rejects a 33rd ordinary sphere so First Life work and renderer snapshots remain bounded.

### Features

* **life:** add bounded branching and analytic capsules ([de48627](https://github.com/LogosITO/Genesis/commit/de48627f9c114112ca9c31c0f3e989143f5fddb7))
* **life:** add bounded environmental growth and replay ([83054d4](https://github.com/LogosITO/Genesis/commit/83054d4a5b68cd3b0d6cc58eee4899cf56026f86))
