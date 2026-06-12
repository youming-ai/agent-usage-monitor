# Changelog

## [0.13.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.12.0...v0.13.0) (2026-06-12)


### Features

* add MiMo Code support and remove Cursor quota ([5214767](https://github.com/youming-ai/agent-usage-monitor/commit/52147679b903b0608894378fd9f9f68ca91afe1b))
* add MiMo Code support and remove Cursor quota ([2e44cd1](https://github.com/youming-ai/agent-usage-monitor/commit/2e44cd1f547fa31d87d4fec9feb3b7bbb0276bad))


### Bug Fixes

* resolve merge conflicts and apply core improvements ([37f0bc9](https://github.com/youming-ai/agent-usage-monitor/commit/37f0bc9d29425c26ba3e29868503d7ea1a332358))

## [0.12.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.11.2...v0.12.0) (2026-06-09)


### Features

* add Copilot CLI and Antigravity CLI support ([7d17bb7](https://github.com/youming-ai/agent-usage-monitor/commit/7d17bb7aae925330827629a7a337a1682613a9c6))
* add platform registry, fixture tests, and README updates ([f5c9704](https://github.com/youming-ai/agent-usage-monitor/commit/f5c970474bb43abdc7a5c82db3ff7a45a3972e0f))

## [0.11.2](https://github.com/youming-ai/agent-usage-monitor/compare/v0.11.1...v0.11.2) (2026-06-08)


### Bug Fixes

* **ui:** align agent tab colors with official CLI themes ([af1e488](https://github.com/youming-ai/agent-usage-monitor/commit/af1e48899f55d6cd41ad62c5be9c67dcdf8acdde))
* **ui:** align tab accent colors with official CLI themes ([ec2b985](https://github.com/youming-ai/agent-usage-monitor/commit/ec2b985d32eb16abc02cac46f65313f85dd51a13))

## [0.11.1](https://github.com/youming-ai/agent-usage-monitor/compare/v0.11.0...v0.11.1) (2026-06-08)


### Documentation

* update README for Grok Build and Cursor CLI support ([e8adde6](https://github.com/youming-ai/agent-usage-monitor/commit/e8adde6fa0d4e01bc1c69841830e229b4e52aee8))
* update README for Grok Build and Cursor CLI support ([5c8b334](https://github.com/youming-ai/agent-usage-monitor/commit/5c8b33467a7dcf4e4810a6a1d0006cd4736bc56c))

## [0.11.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.10.0...v0.11.0) (2026-06-08)


### Features

* add Grok Build and Cursor CLI support; fix critical Cursor reader bugs ([f7efc85](https://github.com/youming-ai/agent-usage-monitor/commit/f7efc85ba825f19ad1e738054b2f18322bb2a83d))
* add Grok Build and Cursor CLI support; fix critical Cursor reader bugs ([49b6bbe](https://github.com/youming-ai/agent-usage-monitor/commit/49b6bbe5e3aeb6013a361ea4a81cf7ed77545c7e))

## [0.10.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.9.0...v0.10.0) (2026-06-05)


### Features

* add CLI args, config, and UI for pi/openclaw/hermes/factory ([599bb3a](https://github.com/youming-ai/agent-usage-monitor/commit/599bb3a01ef838008ad5dff3922f1263507bb12a))
* add OpenClawReader for openclaw agent ([021b653](https://github.com/youming-ai/agent-usage-monitor/commit/021b6538598c32611607920d93dd3012a62b69fe))
* add PiReader for pi agent ([5e9f8d7](https://github.com/youming-ai/agent-usage-monitor/commit/5e9f8d7b7b4ba79e7e3cce7e5cacb054d5c4b885))
* add support for pi, openclaw, hermes-agent, Factory AI ([9c02026](https://github.com/youming-ai/agent-usage-monitor/commit/9c0202690b32a471158454c36948ada25ab69e2c))
* extend Platform/Tab enums for pi, openclaw, hermes, factory ([2250bd7](https://github.com/youming-ai/agent-usage-monitor/commit/2250bd775df0a00cbf173020d722eebee2952444))
* **ui:** update tab_line to show only available tabs ([c2c9d33](https://github.com/youming-ai/agent-usage-monitor/commit/c2c9d331f5eef90e7f8a14e479b900be2b08d398))


### Bug Fixes

* opencode tab detection — use XDG path, not macOS App Support ([40f26b2](https://github.com/youming-ai/agent-usage-monitor/commit/40f26b26d93bf87d754d3789af39ebfc9865618c))
* use next_in/prev_in for tab navigation with available_tabs ([2c643e4](https://github.com/youming-ai/agent-usage-monitor/commit/2c643e4db5231585204cf9480a85da10456e995f))


### Refactors

* remove unused Tab::next/prev (replaced by next_in/prev_in) ([e5c5013](https://github.com/youming-ai/agent-usage-monitor/commit/e5c50136d6683ec073c5701ceee0c3c2cd16cf05))


### Documentation

* update README for new agents; ignore planning artifacts ([dc6705e](https://github.com/youming-ai/agent-usage-monitor/commit/dc6705edf3a484f207f2aa09ba6be843337dda1a))

## [0.9.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.8.0...v0.9.0) (2026-06-05)


### Features

* add Kimi Code CLI support ([bf96e51](https://github.com/youming-ai/agent-usage-monitor/commit/bf96e5123c0f66b7594cfea24579208d9536a48f))
* add Kimi Code CLI support ([30338a7](https://github.com/youming-ai/agent-usage-monitor/commit/30338a7237f27049996708e2f819f36e29f85125))

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
