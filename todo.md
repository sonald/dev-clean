# Dev Cleaner 0.2 开发 TODO

基于 `spec.md`（路线图 0.2+），按优先级推进；每个勾选项完成后都需要跑测试并提交。

## P0（目标：0.2）

- [x] Trash 生命周期管理：`trash list/show/purge/gc` + 跨设备移动（EXDEV）fallback
- [x] 扫描引擎接入配置：`exclude_dirs` 剪枝 + `custom_patterns` 规则 + `--explain` 体现来源优先级
- [x] 分类/风险体系落地：category/risk/confidence + 过滤 flags（`--category/--risk/--max-risk`）与默认安全策略
- [x] 预算清理：新增 `recommend`（或复用 `clean --recommend-only`）支持 `--cleanup/--free-at-least` 并可输出 `plan.json`
- [x] 文档与版本：更新 `README.md` / `QUICKSTART.md` / `config.example.toml`，版本升级到 `0.2.0`

## 可选 / 后续

- [ ] Keep/Protect：`.dev-cleaner-keep` / `.dev-cleaner-keep-patterns` + config keep_paths/keep_roots
- [ ] History（JSONL）：`history list/show/top-growth`
- [ ] TUI 升级：搜索/过滤/排序/详情面板/流式更新

## Core 拆分审查修复清单（2026-04-28）

- [x] TUI 扫描入口改走 `ScanService`，避免绕过 keep/protect 评估。
- [x] TUI 清理路径恢复 observer 输出，避免 core 默认 no-op 后长清理无反馈。
- [x] `Cleaner` 在 destructive 层补上 `protected/recent` 防线，避免非 CLI 调用绕过 `CleanupService`。
- [x] Trash restore 从 core 中移除直接 `println!`，改为 observer/event 交给 adapter 渲染。
- [x] `recommend` 结果保留 typed `EvaluatedProject` / skip reason，避免退回纯 stringly `ProjectInfo`。
- [x] root crate 明确为 adapter/compat 层，内部改直接引用 `dev_cleaner_core`，减少 core/adapter 边界混淆。
- [x] 更新 `AGENTS.md` 的模块路径，避免继续指向已删除的 `src/scanner` 等旧路径。
- [x] `Cargo.lock` 纳入版本控制，root 依赖 core 时补 version，避免 CLI workspace 构建不可复现/发布受阻。
- [x] benchmark 改直接引用 `dev_cleaner_core`。
- [x] 补 CLI `plan -> apply` 集成测试、TUI keep/protect 边界测试、recent/in-use 时间边界测试、stats public API 兼容测试。
- [x] 记录后续范围：Swift/FFI/UniFFI、JSON DTO facade、GUI 可取消 streaming scan 属于下一阶段接口层，不在本轮 core Rust API 修复中实现。
