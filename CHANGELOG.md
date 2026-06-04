# Changelog

## [0.8.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.7.1...v0.8.0) (2026-06-04)


### Features

* add opencode support (usage tab + reserved quota slot) ([6be1fa2](https://github.com/youming-ai/agent-usage-monitor/commit/6be1fa21c05bae9cdb9bd67d4db68f068d1722b4))
* add OpenCode tab scaffolding (Platform/Tab/state/UI) ([5a41da5](https://github.com/youming-ai/agent-usage-monitor/commit/5a41da5597bcb420297962eeb3c0b7f8ddf4174b))
* add OpencodeReader (read-only SQLite usage source) ([2c8de2d](https://github.com/youming-ai/agent-usage-monitor/commit/2c8de2d1b1a21cd43e420dc7748f92312031c4ed))
* reserve opencode quota slot (no public API yet) ([d20e108](https://github.com/youming-ai/agent-usage-monitor/commit/d20e10820c620f1fd9c70ac243d59f342ec30a08))
* wire opencode reader via config/CLI opencode_path ([1adc925](https://github.com/youming-ai/agent-usage-monitor/commit/1adc925628f6ae65d5cf99159d9a762524dbe6e1))


### Bug Fixes

* **config:** treat empty XDG_DATA_HOME as unset; document opencode_path key ([b563ae4](https://github.com/youming-ai/agent-usage-monitor/commit/b563ae4db3d40ccd60673c3152f3be20478929b1))


### Refactors

* **opencode reader:** count orphan-session usage; log prepare errors; document cursor ([450856c](https://github.com/youming-ai/agent-usage-monitor/commit/450856c56d37725fa9ed2931f5357ee142e918c9))
* unify readers behind a UsageSource trait ([5ab1ce8](https://github.com/youming-ai/agent-usage-monitor/commit/5ab1ce8915a86462e91f36caa5a0d0c9c7ae3dc2))


### Documentation

* **cli:** mention opencode in the --help about string ([efdfb31](https://github.com/youming-ai/agent-usage-monitor/commit/efdfb314d76ea23d82bdab5f4f03e1eddc5f5faa))
* implementation plan for opencode support ([16bcf3f](https://github.com/youming-ai/agent-usage-monitor/commit/16bcf3fe243997a78c76a35c5af3914935825aaa))
* spec for opencode support (usage now, quota reserved) ([5874f2a](https://github.com/youming-ai/agent-usage-monitor/commit/5874f2a10c51b3690b27093b876419c13db71730))

## [0.7.1](https://github.com/youming-ai/agent-usage-monitor/compare/v0.7.0...v0.7.1) (2026-06-04)


### Bug Fixes

* **ui:** stable model-table order; count cache-creation in session tokens ([da733e2](https://github.com/youming-ai/agent-usage-monitor/commit/da733e29e48884371de6fced7cd118e8bbd95028))


### Refactors

* **cli/main:** Option-based config merge; resilient reader tasks ([f2450b1](https://github.com/youming-ai/agent-usage-monitor/commit/f2450b1974ffbd50b42ad9bf677e42e2476d685d))
* **quota:** typed QuotaError + shared util; fix decode & error gating ([62ff1ad](https://github.com/youming-ai/agent-usage-monitor/commit/62ff1ad5a31df6fb6ed4169ec4bb7cdaeff193d7))
* **reader:** shared find_recursive; char-safe session labels ([f0827c4](https://github.com/youming-ai/agent-usage-monitor/commit/f0827c4d62517c9955ceba3caf158345d4796fd8))
* **state:** incremental per-model aggregates; cumulative lifetime totals ([caba1c4](https://github.com/youming-ai/agent-usage-monitor/commit/caba1c48077f2fe1400cb2654ccbdba350efb103))
* **updater:** pure-Rust download/extract; fix cross-device install ([b1562fb](https://github.com/youming-ai/agent-usage-monitor/commit/b1562fb0b713421239cc4079de1ac201d3a188b0))


### Build System

* add ureq, tar, flate2 for the pure-Rust self-updater ([f504842](https://github.com/youming-ai/agent-usage-monitor/commit/f50484250249978a65106c2061fc6a3e81a7ae91))
