# Changelog

## [1.6.0](https://github.com/youming-ai/agent-usage-monitor/compare/v1.5.0...v1.6.0) (2026-08-10)


### Features

* use square heatmap cells ([#66](https://github.com/youming-ai/agent-usage-monitor/issues/66)) ([536a1a9](https://github.com/youming-ai/agent-usage-monitor/commit/536a1a9aea34f815cfb16055f58e73fdf49325b0))


### Bug Fixes

* keep the heatmap grid rectangular ([#68](https://github.com/youming-ai/agent-usage-monitor/issues/68)) ([d288f5a](https://github.com/youming-ai/agent-usage-monitor/commit/d288f5a21b28d0db9fe1f6effbfcdc9782d7441a))

## [1.5.0](https://github.com/youming-ai/agent-usage-monitor/compare/v1.4.0...v1.5.0) (2026-08-10)


### Features

* stretch the heatmap week columns to fill the width ([#64](https://github.com/youming-ai/agent-usage-monitor/issues/64)) ([c530b2c](https://github.com/youming-ai/agent-usage-monitor/commit/c530b2c1b5f58e04ab19c7ce1034d510f9fcbb87))

## [1.4.0](https://github.com/youming-ai/agent-usage-monitor/compare/v1.3.1...v1.4.0) (2026-08-10)


### Features

* show a year-long GitHub-style contribution heatmap ([#63](https://github.com/youming-ai/agent-usage-monitor/issues/63)) ([176e8bc](https://github.com/youming-ai/agent-usage-monitor/commit/176e8bc060f79ac1841ab078a347eb80c77d89a6))


### Documentation

* clarify current month heatmap ([2be7f2a](https://github.com/youming-ai/agent-usage-monitor/commit/2be7f2a00b93f18149abc9013be731e90c45172e))
* trim duplicate layout note and Windows watcher mention ([8d703e8](https://github.com/youming-ai/agent-usage-monitor/commit/8d703e8e886c6dd85ea47ffce13e2efba21c7cc2))

## [1.3.1](https://github.com/youming-ai/agent-usage-monitor/compare/v1.3.0...v1.3.1) (2026-08-07)


### Bug Fixes

* show current month daily heatmap ([dd3dc43](https://github.com/youming-ai/agent-usage-monitor/commit/dd3dc437d87de9ba07989ce19c368e8cd9d4fa6d))
* **tui:** show only current month daily heatmap ([e3b5bae](https://github.com/youming-ai/agent-usage-monitor/commit/e3b5baebfbca1077544136e78678621db35a6b27))

## [1.3.0](https://github.com/youming-ai/agent-usage-monitor/compare/v1.2.1...v1.3.0) (2026-08-07)


### Features

* Claude/Codex-only TUI with heatmap and live quota ([e1ada28](https://github.com/youming-ai/agent-usage-monitor/commit/e1ada28281828fb4e4a56397fe2b605e53ea4bf6))
* **quota:** surface richer live usage from Claude and Codex APIs ([39a2679](https://github.com/youming-ai/agent-usage-monitor/commit/39a2679b7a573f8a2958b87d5b0ed52d035273fa))
* slim to Claude/Codex, add heatmap and richer usage aggregates ([3decc64](https://github.com/youming-ai/agent-usage-monitor/commit/3decc644e1579827f93cbcf089e26e728fa3a355))
* **tui:** aggregate contribution heatmap by week ([1720bb7](https://github.com/youming-ai/agent-usage-monitor/commit/1720bb7ce294246a31de18a9677479c36110c0cb))
* **tui:** Claude Code–style heatmap and overview stats ([88614bf](https://github.com/youming-ai/agent-usage-monitor/commit/88614bf4729123fa1286799323327c20dce85188))


### Bug Fixes

* restore weekly heatmap and correct usage aggregates ([798c954](https://github.com/youming-ai/agent-usage-monitor/commit/798c954e36359a2395e9c0a891d00a7956e80faa))
* **tui:** distribute heatmap cell widths to fill residual columns ([4fb440d](https://github.com/youming-ai/agent-usage-monitor/commit/4fb440d73fa90f451e8ce7ef9e17d9d2d4153de7))
* **tui:** spread heatmap across full terminal width ([068ba7a](https://github.com/youming-ai/agent-usage-monitor/commit/068ba7a3f4afc2b104ea693ada26017c98ca495d))
* **tui:** weekday columns instead of month labels on heatmap ([3cebd1e](https://github.com/youming-ai/agent-usage-monitor/commit/3cebd1e79a4cd9de98c6b25f7d10d3f0d6e72ab6))


### Refactors

* **tui:** show only contribution heatmaps per platform ([5b4ffd8](https://github.com/youming-ai/agent-usage-monitor/commit/5b4ffd80d0b19047be5e4d786ade7bd94bcdf4d0))

## [1.2.1](https://github.com/youming-ai/agent-usage-monitor/compare/v1.2.0...v1.2.1) (2026-07-31)


### Bug Fixes

* **ui:** show quota reset time again ([699fc3e](https://github.com/youming-ai/agent-usage-monitor/commit/699fc3ea327b2e272c9a305ef4034c01f6c8589d))
* **ui:** show quota reset time again ([8530232](https://github.com/youming-ai/agent-usage-monitor/commit/85302323c8146b70d54969798a6912adb7d155f4))

## [1.2.0](https://github.com/youming-ai/agent-usage-monitor/compare/v1.1.2...v1.2.0) (2026-07-31)


### Features

* restore Pi usage support ([2423c1c](https://github.com/youming-ai/agent-usage-monitor/commit/2423c1c34abc4c2561bacac8418ed1adf3a93c62))
* restore Pi usage support ([bef3e19](https://github.com/youming-ai/agent-usage-monitor/commit/bef3e19689d290523fb58faed0e606d5b9d111b1))


### Bug Fixes

* harden Pi record identity and session labels ([8d0fa8c](https://github.com/youming-ai/agent-usage-monitor/commit/8d0fa8cf7cdce1fa0e5138da9b0dd69b5a243af6))

## [1.1.2](https://github.com/youming-ai/agent-usage-monitor/compare/v1.1.1...v1.1.2) (2026-07-29)


### Refactors

* **tui:** compact dashboard layout ([e1d6485](https://github.com/youming-ai/agent-usage-monitor/commit/e1d64851a77d8ea14ae1b7965c4006ee8f67c160))
* **tui:** compact dashboard layout ([152f22c](https://github.com/youming-ai/agent-usage-monitor/commit/152f22cfbdb8807ec9aae05b3f632efb103f0829))

## [1.1.1](https://github.com/youming-ai/agent-usage-monitor/compare/v1.1.0...v1.1.1) (2026-07-28)


### Bug Fixes

* harden monitor refresh scheduling ([fcd2fb7](https://github.com/youming-ai/agent-usage-monitor/commit/fcd2fb77aef7e3796752656124a32862dabcc730))
* harden monitor refresh scheduling ([e1bb38a](https://github.com/youming-ai/agent-usage-monitor/commit/e1bb38af215a8fbaa2725cb1af892634af1e6dd0))

## [1.1.0](https://github.com/youming-ai/agent-usage-monitor/compare/v1.0.0...v1.1.0) (2026-07-23)


### Features

* **tui:** resume a session by selecting it in the sessions list ([e605f8e](https://github.com/youming-ai/agent-usage-monitor/commit/e605f8e193da8f6d6d4bd0eaf3a917961ea02d95))
* **tui:** resume agent session from sessions list in new terminal window ([f5fe7d7](https://github.com/youming-ai/agent-usage-monitor/commit/f5fe7d72e91695bd5b4db11f407da7c6c6b92a4c))
* **tui:** resume in a new terminal window, keep monitoring ([5ba300a](https://github.com/youming-ai/agent-usage-monitor/commit/5ba300a5cb827b7f04597f5dfe850e9618c2cd21))
* **tui:** show session titles; recolor Codex tab blue ([715c206](https://github.com/youming-ai/agent-usage-monitor/commit/715c206e367d143f8d6daf61e9bb26a6c409c503))


### Bug Fixes

* **mcp:** update platform count and config-set help examples for 3-platform registry ([69921de](https://github.com/youming-ai/agent-usage-monitor/commit/69921ded1039c77af57ae0cfb47a43cbb79c020a))
* **tui:** expand Linux terminal fallback chain, resolve Cursor cwd, and guard usage metrics ([226cad0](https://github.com/youming-ai/agent-usage-monitor/commit/226cad028b452eaab6120c4bfc9b3992fb4bfbe6))


### Refactors

* **tui:** address PR [#45](https://github.com/youming-ai/agent-usage-monitor/issues/45) review findings ([ba27cc4](https://github.com/youming-ai/agent-usage-monitor/commit/ba27cc4f52d861172205a2e1730d12adb8dc0719))
* **tui:** extract session launcher and cleanup app_state usage guards ([a39e914](https://github.com/youming-ai/agent-usage-monitor/commit/a39e9140afa019d270407bee35ab7076a755381f))

## [1.0.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.18.1...v1.0.0) (2026-07-22)


### ⚠ BREAKING CHANGES

* config keys and CLI flags for the removed agents no longer exist. Old config.toml files still load (serde ignores unknown keys).

### Features

* support only Claude Code, Codex & Cursor ([26a0eba](https://github.com/youming-ai/agent-usage-monitor/commit/26a0eba1eaca8c58fb06b8feec162eacfd6986de))

## [0.18.1](https://github.com/youming-ai/agent-usage-monitor/compare/v0.18.0...v0.18.1) (2026-07-21)


### Bug Fixes

* **updater:** provision the release minisign public key ([46ad3a2](https://github.com/youming-ai/agent-usage-monitor/commit/46ad3a20906dd7339feba617a7b33ff0b1d13044))
* **updater:** provision the release minisign public key ([d2d6a10](https://github.com/youming-ai/agent-usage-monitor/commit/d2d6a103bb2483425dc972e21d219320dce6e517))

## [0.18.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.17.0...v0.18.0) (2026-07-21)


### Features

* **platform:** add omp (Oh My Pi) support & order tabs by usage ([e4aa040](https://github.com/youming-ai/agent-usage-monitor/commit/e4aa040e1c1ff7bcdbb429b8bc1d187362f0dc29))

## [0.17.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.16.2...v0.17.0) (2026-07-21)


### Features

* **security:** sign and verify release artifacts ([2ae876e](https://github.com/youming-ai/agent-usage-monitor/commit/2ae876e6548e6e935ab9c8e82f53e03016d989b7))

## [0.16.2](https://github.com/youming-ai/agent-usage-monitor/compare/v0.16.1...v0.16.2) (2026-07-17)


### Refactors

* dedup reader scan/read loops and quota, drop dead code ([b6f4123](https://github.com/youming-ai/agent-usage-monitor/commit/b6f4123f74feece6f173978b139017badd8c9b72))
* dedup reader scan/read loops and quota, drop dead code ([19f514a](https://github.com/youming-ai/agent-usage-monitor/commit/19f514a40a116315f3596ae1bd40da530bbe9ded))

## [0.16.1](https://github.com/youming-ai/agent-usage-monitor/compare/v0.16.0...v0.16.1) (2026-07-16)


### Bug Fixes

* correct usage and cost accuracy bugs across readers ([705a6c7](https://github.com/youming-ai/agent-usage-monitor/commit/705a6c7a4609780b03e8aa78aace5206acbc3209))
* correct usage and cost accuracy bugs across readers ([68b212f](https://github.com/youming-ai/agent-usage-monitor/commit/68b212f7e27b4fb8f72d8a90a5ad4c5d2b461cef))

## [0.16.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.15.0...v0.16.0) (2026-07-07)


### Features

* support local login and auth status checking for 6 additional platforms ([d0816eb](https://github.com/youming-ai/agent-usage-monitor/commit/d0816eb7442711e6990cb9497e84e40b04e7ebf6))
* support local login and auth status checking for 6 additional platforms ([c449233](https://github.com/youming-ai/agent-usage-monitor/commit/c449233c392c4aa87402ac58f97eefe04189c927))

## [0.15.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.14.2...v0.15.0) (2026-06-22)


### Features

* **mcp:** adapt all 13 readers to construct UsageRecord using intern() ([d25a7ed](https://github.com/youming-ai/agent-usage-monitor/commit/d25a7edcfa1eb4aa0478a2f534886948bcd1e871))
* **mcp:** adapt MCP server handlers to CompactDate and Spur ([84d27e1](https://github.com/youming-ai/agent-usage-monitor/commit/84d27e1f73a9f0e5a67cbeee74de395828e2ed42))
* **mcp:** adapt stats module to CompactDate and Spur ([a8106bf](https://github.com/youming-ai/agent-usage-monitor/commit/a8106bf44976f29530fcadbc2656465cd1341876))
* **mcp:** adapt TUI model and session tables to Spur + resolve() ([7b833cb](https://github.com/youming-ai/agent-usage-monitor/commit/7b833cb342f1ac07774269cba931a0801d52c9f0))
* **mcp:** add 7 file_ops fields to UsageRecord (default 0) ([e3de62c](https://github.com/youming-ai/agent-usage-monitor/commit/e3de62ce4980e091d6671c8d1d582c8ade629e61))
* **mcp:** add aum mcp subcommand with main.rs routing ([77824a7](https://github.com/youming-ai/agent-usage-monitor/commit/77824a7f6e11af9e8722bad293e67642045c4190))
* **mcp:** add MCP server skeleton with AumMcpServer struct (fix plan bugs: get_info, AgentPaths from state) ([f6d61c8](https://github.com/youming-ai/agent-usage-monitor/commit/f6d61c86f0c7b5ab4cc89a437231201284f07d55))
* **mcp:** implement 2 resources (aum://summary, aum://platforms) ([6cf7eed](https://github.com/youming-ai/agent-usage-monitor/commit/6cf7eed4aa0cb03f8a9bc1273266d884031be0f8))
* **mcp:** implement 6 tool handlers (get_daily_stats, get_model_usage, get_cost_breakdown, get_file_operations, get_session_stats, get_quota) ([7a8286b](https://github.com/youming-ai/agent-usage-monitor/commit/7a8286b07db0fc2731da6b9860440b3705a51016))
* **mcp:** update UsageRecord and state definition fields to Spur ([475569c](https://github.com/youming-ai/agent-usage-monitor/commit/475569c96046f67bf70c9dad4b3d781e0f457d2d))
* stats, watcher, MCP server, and memory refactor ([5be6f6c](https://github.com/youming-ai/agent-usage-monitor/commit/5be6f6cda4dabbecfc780c30549335270ccec221))
* **stats:** add build_platform_report aggregator with tests ([a5eb91d](https://github.com/youming-ai/agent-usage-monitor/commit/a5eb91d513af61a0e4603525a8c17e7b5fbcd069))
* **stats:** add collect() function wiring readers + quota ([eb23dce](https://github.com/youming-ai/agent-usage-monitor/commit/eb23dceb9ef599d750e7b9182f78ebe34b322c56))
* **stats:** add Filters struct with platform/date matching ([528d845](https://github.com/youming-ai/agent-usage-monitor/commit/528d845c93babf17632e03ec405acbd1d5d0161b))
* **stats:** add QuotaView::from_info constructor ([b6377da](https://github.com/youming-ai/agent-usage-monitor/commit/b6377da3bd14967af9de1012c8106c3f6c23ebb7))
* **stats:** add resolve_platform_filter accepting 3 key forms ([9b2b1ac](https://github.com/youming-ai/agent-usage-monitor/commit/9b2b1ac7c198035f82dfe230d48476f155178b00))
* **stats:** add stats module with data types ([6372afa](https://github.com/youming-ai/agent-usage-monitor/commit/6372afaa13417b2edd3f03c8f2be6b2ee6d1e01c))
* **stats:** add stats subcommand to CLI ([b99e281](https://github.com/youming-ai/agent-usage-monitor/commit/b99e281a0d8458dc66fdac19d3ae164dd92fde68))
* **stats:** add write_json with pretty/compact modes ([22a1d99](https://github.com/youming-ai/agent-usage-monitor/commit/22a1d9937cc2326f2546b7b555dac5a69a4273db))
* **stats:** route stats subcommand in main ([e4e34ed](https://github.com/youming-ai/agent-usage-monitor/commit/e4e34edcb6b8dc439c6fe01c170684887b968a6a))
* **watcher:** add get_watch_directories() to UsageSource trait + 13 impls ([3711f0a](https://github.com/youming-ai/agent-usage-monitor/commit/3711f0a53d8a2dff729271a5cf69adb09f74a498))
* **watcher:** add PlatformWatcher with notify 50ms debounce per platform ([da35fa7](https://github.com/youming-ai/agent-usage-monitor/commit/da35fa77346be27b4276cf20924fbd86542bdea9))
* **watcher:** implement global INTERNER and CompactDate with tests ([9152e95](https://github.com/youming-ai/agent-usage-monitor/commit/9152e951b4f0e7666ab1900a4f7318661a9eaa18))
* **watcher:** replace 1s polling with FS events + 30s fallback in main ([b8ebbc8](https://github.com/youming-ai/agent-usage-monitor/commit/b8ebbc86c36b16e46805bc358de62741c773f91c))


### Refactors

* **stats:** extract platform_canonical_key helper for reuse in collect() ([97f599e](https://github.com/youming-ai/agent-usage-monitor/commit/97f599ee74be3a1e8a1cc16b74235844e1627674))


### Build System

* add lasso 0.7 dependency with multi-threaded feature ([e27e3f2](https://github.com/youming-ai/agent-usage-monitor/commit/e27e3f269e753fd5c34cf769ce9c725d66bb434d))
* add notify 8 and notify-debouncer-full 0.6 for FS watcher ([7120d70](https://github.com/youming-ai/agent-usage-monitor/commit/7120d709635ef51bf36e72a9b66037f72561c5b5))
* add rmcp 0.12 for MCP server ([6f8d030](https://github.com/youming-ai/agent-usage-monitor/commit/6f8d030da29f7bdca0c7f5b4b88facd929fcf64d))


### Documentation

* add design spec for aum mcp server ([ab884cf](https://github.com/youming-ai/agent-usage-monitor/commit/ab884cf2f808a460d26cd5eba78b3da38fd57f08))
* add design spec for aum stats --json subcommand ([e41ff59](https://github.com/youming-ai/agent-usage-monitor/commit/e41ff595d2aaf1da55af3b16a497e251dd272292))
* add design spec for memory refactor ([d2fb2bc](https://github.com/youming-ai/agent-usage-monitor/commit/d2fb2bc790c61370cb2bab6f22b8a10c4f7126df))
* add design spec for notify watcher ([a8228ba](https://github.com/youming-ai/agent-usage-monitor/commit/a8228ba8509f2f955f83255fb7c428a716bbb69e))
* add implementation plan for aum mcp server ([3de6113](https://github.com/youming-ai/agent-usage-monitor/commit/3de61137eed902228d53eb195af50a1fb85f2895))
* add implementation plan for aum stats --json subcommand ([b863d1e](https://github.com/youming-ai/agent-usage-monitor/commit/b863d1e72fa0fe7252398fe496417dd80ef3dfe7))
* add implementation plan for notify watcher ([da32a14](https://github.com/youming-ai/agent-usage-monitor/commit/da32a14a3fb7435473e796d5d35ed02558ac8427))
* add JSON stats section to README ([7dc0b27](https://github.com/youming-ai/agent-usage-monitor/commit/7dc0b27a22895d15eb293ce63b9396daff98d611))
* fix plan - schemars as direct dep (not via rmcp re-export) ([547cec8](https://github.com/youming-ai/agent-usage-monitor/commit/547cec8a79c03745c63f55434c1f12c3c3c4e1de))
* fix plan bugs (AgentPaths from state, get_info not server_info) ([e928280](https://github.com/youming-ai/agent-usage-monitor/commit/e92828018bf7d0ac78739164dcf7ce9516093f9e))
* **mcp:** add MCP server section to README ([49a95f0](https://github.com/youming-ai/agent-usage-monitor/commit/49a95f0994f63a5ef7d7ea1f32c87db7471ba745))

## [0.14.2](https://github.com/youming-ai/agent-usage-monitor/compare/v0.14.1...v0.14.2) (2026-06-12)


### Bug Fixes

* deduplicate AppState, unify SQLite readers, decouple quota fetchers ([9bd86ba](https://github.com/youming-ai/agent-usage-monitor/commit/9bd86baa4ad910eced0d154c6c864d8173d1bc40))

## [0.14.1](https://github.com/youming-ai/agent-usage-monitor/compare/v0.14.0...v0.14.1) (2026-06-12)


### Documentation

* add MiMo Code to README ([f708610](https://github.com/youming-ai/agent-usage-monitor/commit/f708610a92e2d8ec69d740c03b6370107b570f3d))
* add MiMo Code to README ([74ba356](https://github.com/youming-ai/agent-usage-monitor/commit/74ba356874f461804d2b50210ba4d995231508c4))

## [0.14.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.13.0...v0.14.0) (2026-06-12)


### Features

* add --takeover mode to intercept default 11434 port ([a318622](https://github.com/youming-ai/agent-usage-monitor/commit/a3186225ccbfce7126fd596dc257d91682e35163))
* add axum proxy with SSE usage interception ([054c494](https://github.com/youming-ai/agent-usage-monitor/commit/054c494149596b0b51c51e13089b1d273720cee8))
* add CLI args, config, and UI for pi/openclaw/hermes/factory ([599bb3a](https://github.com/youming-ai/agent-usage-monitor/commit/599bb3a01ef838008ad5dff3922f1263507bb12a))
* add CLI argument parsing with clap ([44e316d](https://github.com/youming-ai/agent-usage-monitor/commit/44e316dc729626b033077348c1763c17dc0ee86e))
* Add color distinction between Claude Code and Codex tabs ([956acbc](https://github.com/youming-ai/agent-usage-monitor/commit/956acbcb95ec5663e9d90c83f37554484a94cda2))
* add Copilot CLI and Antigravity CLI support ([7d17bb7](https://github.com/youming-ai/agent-usage-monitor/commit/7d17bb7aae925330827629a7a337a1682613a9c6))
* add Copilot CLI and Antigravity CLI support ([c6a00c7](https://github.com/youming-ai/agent-usage-monitor/commit/c6a00c7c0db63963717e0d2c6498526a687065c2))
* add crossterm event loop with tick and key events ([2cd4c7e](https://github.com/youming-ai/agent-usage-monitor/commit/2cd4c7e2b662746e571111976de992daad987498))
* add Grok Build and Cursor CLI support; fix critical Cursor reader bugs ([f7efc85](https://github.com/youming-ai/agent-usage-monitor/commit/f7efc85ba825f19ad1e738054b2f18322bb2a83d))
* add Grok Build and Cursor CLI support; fix critical Cursor reader bugs ([49b6bbe](https://github.com/youming-ai/agent-usage-monitor/commit/49b6bbe5e3aeb6013a361ea4a81cf7ed77545c7e))
* add Kimi Code CLI support ([bf96e51](https://github.com/youming-ai/agent-usage-monitor/commit/bf96e5123c0f66b7594cfea24579208d9536a48f))
* add Kimi Code CLI support ([30338a7](https://github.com/youming-ai/agent-usage-monitor/commit/30338a7237f27049996708e2f819f36e29f85125))
* add MiMo Code support and remove Cursor quota ([5214767](https://github.com/youming-ai/agent-usage-monitor/commit/52147679b903b0608894378fd9f9f68ca91afe1b))
* add MiMo Code support and remove Cursor quota ([2e44cd1](https://github.com/youming-ai/agent-usage-monitor/commit/2e44cd1f547fa31d87d4fec9feb3b7bbb0276bad))
* add OllamaClient for polling /api/ps ([3999d0e](https://github.com/youming-ai/agent-usage-monitor/commit/3999d0eb3228d2c5cf7dc9893c22139e6c67709e))
* add OpenClawReader for openclaw agent ([021b653](https://github.com/youming-ai/agent-usage-monitor/commit/021b6538598c32611607920d93dd3012a62b69fe))
* add opencode support (usage tab + reserved quota slot) ([6be1fa2](https://github.com/youming-ai/agent-usage-monitor/commit/6be1fa21c05bae9cdb9bd67d4db68f068d1722b4))
* add OpenCode tab scaffolding (Platform/Tab/state/UI) ([5a41da5](https://github.com/youming-ai/agent-usage-monitor/commit/5a41da5597bcb420297962eeb3c0b7f8ddf4174b))
* add OpencodeReader (read-only SQLite usage source) ([2c8de2d](https://github.com/youming-ai/agent-usage-monitor/commit/2c8de2d1b1a21cd43e420dc7748f92312031c4ed))
* add PiReader for pi agent ([5e9f8d7](https://github.com/youming-ai/agent-usage-monitor/commit/5e9f8d7b7b4ba79e7e3cce7e5cacb054d5c4b885))
* add platform registry, fixture tests, and README updates ([f5c9704](https://github.com/youming-ai/agent-usage-monitor/commit/f5c970474bb43abdc7a5c82db3ff7a45a3972e0f))
* add ratatui UI components for models, usage, and status bar ([e29ef69](https://github.com/youming-ai/agent-usage-monitor/commit/e29ef69de274c445976dcf3716d4c9a357018e44))
* add support for pi, openclaw, hermes-agent, Factory AI ([9c02026](https://github.com/youming-ai/agent-usage-monitor/commit/9c0202690b32a471158454c36948ada25ab69e2c))
* Add update subcommand for self-updating ([b52d928](https://github.com/youming-ai/agent-usage-monitor/commit/b52d928cc22e23e560f7776749c2361eb44d20a4))
* define AppState with RunningModel and ApiCall ([7cdacfc](https://github.com/youming-ai/agent-usage-monitor/commit/7cdacfc7dc7adcdba114de5ba13c8d24fdb0f036))
* extend Platform/Tab enums for pi, openclaw, hermes, factory ([2250bd7](https://github.com/youming-ai/agent-usage-monitor/commit/2250bd775df0a00cbf173020d722eebee2952444))
* key the sessions panel per conversation, not per directory ([16c9fcc](https://github.com/youming-ai/agent-usage-monitor/commit/16c9fcccb23a775ab9c56ebe560325cfcc97cbf0))
* platform registry, fixture tests, and README updates ([4389196](https://github.com/youming-ai/agent-usage-monitor/commit/43891968e47e821dfdea04a9fb53ddb5b8649802))
* Redesign TUI layout with progress bar and remove Recent API Calls ([53acb0d](https://github.com/youming-ai/agent-usage-monitor/commit/53acb0db1bb2c0d4d809568db9c66626fd7f3a83))
* replace recent-calls feed with per-session usage panel ([21f847e](https://github.com/youming-ai/agent-usage-monitor/commit/21f847e3a6d9148d7b4943dd410e0444a94a595a))
* reserve opencode quota slot (no public API yet) ([d20e108](https://github.com/youming-ai/agent-usage-monitor/commit/d20e10820c620f1fd9c70ac243d59f342ec30a08))
* **ui:** update tab_line to show only available tabs ([c2c9d33](https://github.com/youming-ai/agent-usage-monitor/commit/c2c9d331f5eef90e7f8a14e479b900be2b08d398))
* Update tab labels to uppercase and change Codex color to blue ([aecffd3](https://github.com/youming-ai/agent-usage-monitor/commit/aecffd3375c9593d63df46fbe299d806de254035))
* wire opencode reader via config/CLI opencode_path ([1adc925](https://github.com/youming-ai/agent-usage-monitor/commit/1adc925628f6ae65d5cf99159d9a762524dbe6e1))
* wire proxy, polling, and TUI into main runtime ([f191bf3](https://github.com/youming-ai/agent-usage-monitor/commit/f191bf30b8290546507041fa19c27e3e84ae6cf8))


### Bug Fixes

* **config:** treat empty XDG_DATA_HOME as unset; document opencode_path key ([b563ae4](https://github.com/youming-ai/agent-usage-monitor/commit/b563ae4db3d40ccd60673c3152f3be20478929b1))
* correct delta tracking, model attribution, pricing, and Linux quota ([b1d23e9](https://github.com/youming-ai/agent-usage-monitor/commit/b1d23e936f5dee520ec4aa0ed5740a14f413ac0c))
* extract model from turn_context events in Codex rollout parsing ([6fe4314](https://github.com/youming-ai/agent-usage-monitor/commit/6fe4314ba96650d6473644adda99a38f73f0f115))
* extract usage from root-level JSON fields and handle non-streaming responses ([f00d8d4](https://github.com/youming-ai/agent-usage-monitor/commit/f00d8d4efcb329446ac490dc04745b8784ebd05f))
* line-buffer streaming chunks and normalize ollama host URL ([a9f64c2](https://github.com/youming-ai/agent-usage-monitor/commit/a9f64c22a65bcb7befe2ce97d8c9e83e1a4e75a6))
* opencode tab detection — use XDG path, not macOS App Support ([40f26b2](https://github.com/youming-ai/agent-usage-monitor/commit/40f26b26d93bf87d754d3789af39ebfc9865618c))
* Progress bar now uses platform-specific color instead of green ([62e6c30](https://github.com/youming-ai/agent-usage-monitor/commit/62e6c3007cb2734c0dbd81b205ecde832215200d))
* Remove unused imports and variables ([ded0c81](https://github.com/youming-ai/agent-usage-monitor/commit/ded0c81b6e0cda24a83c5afa4ad34e2eac36ecd2))
* resolve merge conflicts and apply core improvements ([37f0bc9](https://github.com/youming-ai/agent-usage-monitor/commit/37f0bc9d29425c26ba3e29868503d7ea1a332358))
* **ui:** align agent tab colors with official CLI themes ([af1e488](https://github.com/youming-ai/agent-usage-monitor/commit/af1e48899f55d6cd41ad62c5be9c67dcdf8acdde))
* **ui:** align tab accent colors with official CLI themes ([ec2b985](https://github.com/youming-ai/agent-usage-monitor/commit/ec2b985d32eb16abc02cac46f65313f85dd51a13))
* **ui:** stable model-table order; count cache-creation in session tokens ([da733e2](https://github.com/youming-ai/agent-usage-monitor/commit/da733e29e48884371de6fced7cd118e8bbd95028))
* use next_in/prev_in for tab navigation with available_tabs ([2c643e4](https://github.com/youming-ai/agent-usage-monitor/commit/2c643e4db5231585204cf9480a85da10456e995f))


### Refactors

* Claude Code cost from JSONL only, Codex keeps pricing table ([8fdee3c](https://github.com/youming-ai/agent-usage-monitor/commit/8fdee3c3dba12c5a82e383636d82e7b1b72a40f2))
* **cli/main:** Option-based config merge; resilient reader tasks ([f2450b1](https://github.com/youming-ai/agent-usage-monitor/commit/f2450b1974ffbd50b42ad9bf677e42e2476d685d))
* **opencode reader:** count orphan-session usage; log prepare errors; document cursor ([450856c](https://github.com/youming-ai/agent-usage-monitor/commit/450856c56d37725fa9ed2931f5357ee142e918c9))
* **quota:** typed QuotaError + shared util; fix decode & error gating ([62ff1ad](https://github.com/youming-ai/agent-usage-monitor/commit/62ff1ad5a31df6fb6ed4169ec4bb7cdaeff193d7))
* **reader:** shared find_recursive; char-safe session labels ([f0827c4](https://github.com/youming-ai/agent-usage-monitor/commit/f0827c4d62517c9955ceba3caf158345d4796fd8))
* redesign TUI and add real-time recent-calls feed ([d048386](https://github.com/youming-ai/agent-usage-monitor/commit/d048386bb5d9e6919028a0cd7eb9fd5023194358))
* remove the per-tab ☁/⚡ icons ([c05562f](https://github.com/youming-ai/agent-usage-monitor/commit/c05562f1cff3f986cdab3fcb305eb847787be14f))
* remove unused Tab::next/prev (replaced by next_in/prev_in) ([e5c5013](https://github.com/youming-ai/agent-usage-monitor/commit/e5c50136d6683ec073c5701ceee0c3c2cd16cf05))
* rename to agent-usage-monitor, command aum; release v0.6.0 ([de70c19](https://github.com/youming-ai/agent-usage-monitor/commit/de70c19b6b493b8083fa3ef3793a817529055f0d))
* simplify session table to show per-model totals only ([08d3f15](https://github.com/youming-ai/agent-usage-monitor/commit/08d3f152d6063cfc86408ae2c65d6383ab9c2850))
* Simplify UI colors and uppercase all text ([c340ca8](https://github.com/youming-ai/agent-usage-monitor/commit/c340ca832f3aa1373b4d28449921f0b06778ae95))
* **state:** incremental per-model aggregates; cumulative lifetime totals ([caba1c4](https://github.com/youming-ai/agent-usage-monitor/commit/caba1c48077f2fe1400cb2654ccbdba350efb103))
* transform ollama-monitor into Claude Code + Codex usage monitor ([3da9787](https://github.com/youming-ai/agent-usage-monitor/commit/3da9787bc6c7ff9d6399500e80aa282f52253225))
* unify readers behind a UsageSource trait ([5ab1ce8](https://github.com/youming-ai/agent-usage-monitor/commit/5ab1ce8915a86462e91f36caa5a0d0c9c7ae3dc2))
* **updater:** pure-Rust download/extract; fix cross-device install ([b1562fb](https://github.com/youming-ai/agent-usage-monitor/commit/b1562fb0b713421239cc4079de1ac201d3a188b0))


### Build System

* add ureq, tar, flate2 for the pure-Rust self-updater ([f504842](https://github.com/youming-ai/agent-usage-monitor/commit/f50484250249978a65106c2061fc6a3e81a7ae91))


### Documentation

* add MIT license and README ([4e70c5f](https://github.com/youming-ai/agent-usage-monitor/commit/4e70c5f3d9c6a0a655aed95014c173fe9bc14871))
* **cli:** mention opencode in the --help about string ([efdfb31](https://github.com/youming-ai/agent-usage-monitor/commit/efdfb314d76ea23d82bdab5f4f03e1eddc5f5faa))
* implementation plan for opencode support ([16bcf3f](https://github.com/youming-ai/agent-usage-monitor/commit/16bcf3fe243997a78c76a35c5af3914935825aaa))
* simplify README pricing section ([b0a3176](https://github.com/youming-ai/agent-usage-monitor/commit/b0a3176c1df51c0aa242f7f84ffdea1649075dc6))
* spec for opencode support (usage now, quota reserved) ([5874f2a](https://github.com/youming-ai/agent-usage-monitor/commit/5874f2a10c51b3690b27093b876419c13db71730))
* tighten README, clarify usage, fix per-session description ([3501572](https://github.com/youming-ai/agent-usage-monitor/commit/3501572fe419ebf6900668bc27c11e2c9d317cfc))
* update README for Grok Build and Cursor CLI support ([e8adde6](https://github.com/youming-ai/agent-usage-monitor/commit/e8adde6fa0d4e01bc1c69841830e229b4e52aee8))
* update README for Grok Build and Cursor CLI support ([5c8b334](https://github.com/youming-ai/agent-usage-monitor/commit/5c8b33467a7dcf4e4810a6a1d0006cd4736bc56c))
* update README for new agents; ignore planning artifacts ([dc6705e](https://github.com/youming-ai/agent-usage-monitor/commit/dc6705edf3a484f207f2aa09ba6be843337dda1a))
* Update README with simplified UI design ([3bed0df](https://github.com/youming-ai/agent-usage-monitor/commit/3bed0df9a60903492e5c4eecf58f3ad130bde409))

## [0.13.0](https://github.com/youming-ai/agent-usage-monitor/compare/v0.12.1...v0.13.0) (2026-06-12)


### Features

* add MiMo Code support and remove Cursor quota ([5214767](https://github.com/youming-ai/agent-usage-monitor/commit/52147679b903b0608894378fd9f9f68ca91afe1b))
* add MiMo Code support and remove Cursor quota ([2e44cd1](https://github.com/youming-ai/agent-usage-monitor/commit/2e44cd1f547fa31d87d4fec9feb3b7bbb0276bad))

## [0.12.1](https://github.com/youming-ai/agent-usage-monitor/compare/v0.12.0...v0.12.1) (2026-06-10)


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
