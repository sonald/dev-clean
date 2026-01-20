# Dev Cleaner 0.2 开发 TODO

基于 `spec.md`（路线图 0.2+），按优先级推进；每个勾选项完成后都需要跑测试并提交。

## P0（目标：0.2）

- [x] Trash 生命周期管理：`trash list/show/purge/gc` + 跨设备移动（EXDEV）fallback
- [x] 扫描引擎接入配置：`exclude_dirs` 剪枝 + `custom_patterns` 规则 + `--explain` 体现来源优先级
- [x] 分类/风险体系落地：category/risk/confidence + 过滤 flags（`--category/--risk/--max-risk`）与默认安全策略
- [x] 预算清理：新增 `recommend`（或复用 `clean --recommend-only`）支持 `--cleanup/--free-at-least` 并可输出 `plan.json`
- [ ] 文档与版本：更新 `README.md` / `QUICKSTART.md` / `config.example.toml`，版本升级到 `0.2.0`

## 可选 / 后续

- [ ] Keep/Protect：`.dev-cleaner-keep` / `.dev-cleaner-keep-patterns` + config keep_paths/keep_roots
- [ ] History（JSONL）：`history list/show/top-growth`
- [ ] TUI 升级：搜索/过滤/排序/详情面板/流式更新
