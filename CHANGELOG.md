# Changelog

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
