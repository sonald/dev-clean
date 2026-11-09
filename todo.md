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

## 评审改进计划（2026-11-03）

来源：成熟度/交互/功能评审。原则：先修信任，再修精度，再加功能；每阶段结束跑 `cargo test --workspace`、`swift test`，并用临时目录 fixture 做端到端验证。

### 阶段 1：信任修复（GUI + bridge）
- [x] 1.1 不可逆操作二次确认：Trash 页 Delete Permanently / Empty Trash、Settings 的 Empty Trash；Recommendations 的清理也要走确认表单。
- [x] 1.2 apply 不再静默丢项：bridge 把校验阶段跳过的项发 `cleanup_skipped` 事件，计入 summary 和 audit；GUI 写 plan 时 `scan_root` 取所有扫描根的公共祖先。
- [x] 1.3 区分预演 / 移入回收站 / 永久删除：summary 带 `mode`，audit run 记录 mode；GUI 汇总页与 History 按 mode 显示，dry-run 和 trash 不算"已释放"；CLI 的 dry-run 文案改为 "Would free"。
- [x] 1.4 去掉假数据：Dashboard 用真实卷容量和真实保护规则；Recommendations 用真实可用空间；汇总页分类按实际清理项计算；规则表空时显示空状态；回收站路径从 bridge 读取。
- [x] 1.5 设置要么生效要么删除：接通扫描深度、recent 天数、显示最近修改项、显示 .gitignore 发现项、回收站保留天数/容量上限；删除没有实现的开关（跟随软链、忽略隐藏目录、开机启动、菜单栏、提醒、大文件保护、高风险/完整性校验）和不支持的浅色外观。
- [x] 1.6 GUI 安全默认：详情页按 risk/confidence/in-use/recent 给结论，不再统一显示 "Safe to Clean"；in-use 项不可勾选、不计入可释放空间。
- [x] 1.7 审计补齐：bridge 的 restore / purge / gc 写审计（命令名与 CLI 一致：`undo` / `trash`）；History 的 "Last 30 Days" 改按日期，"Restore" 按 `undo` 过滤。

### 阶段 2：检测精度（core）
- [x] 2.1 遍历不进入隐藏目录（扫描根本身除外；`.venv`、`.next` 等目标照常识别），也不进入 `node_modules` 内部寻找项目。实测 home 扫描：GUI 等效 79→12 项、13.7s→2.9s；CLI 默认 51→5 项，45 个扩展 `dist` 全部排除。
- [x] 2.2 含有 git 跟踪文件的目录永不作为候选（`git ls-files`；不在仓库或 git 不可用时保持原行为）。已知代价：`target/` 里误提交了少量文件时整个目录也会被排除（本机 tokio 源码目录 1.18 GB）。
- [x] 2.3 in-use 锁文件补全：`uv.lock`、`bun.lock`、`bun.lockb`、`Package.resolved`、`mix.lock` 等。
- [x] 2.4 大小按实际占用块计算，硬链接只在全部链接都位于目标内时计入（pnpm/uv 硬链接模式不再虚高）。APFS 克隆仍按全量计：本机实测目标目录无克隆；升级路径 `getattrlist(ATTR_CMNEXT_PRIVATESIZE)` 已验证可用。

### 阶段 3：回收站真正释放空间
- [x] 3.1 GUI 启动时和每次清理后按保留天数自动 `trash gc`（容量上限改为超限提醒：按容量清理会删掉刚移入的批次，破坏撤销）。
- [x] 3.2 跨卷移动不再把数据拷到系统盘：其他卷上的项目放进该卷根目录的 `.dev-cleaner-trash`，清空时在原卷释放空间；卷被推出时清空失败并保留记录。顺带修复：恢复后条目从 trash 日志移除（原来已恢复的批次仍被列出并计入大小）。
- [x] 3.3 Dashboard 显示回收站占用；超过提醒阈值时 Dashboard 和 Trash 页给出警告。

### 阶段 4：交互体验
- [x] 4.1 确认表单列出将处理的条目；详情页按钮只作用于当前项。
- [x] 4.2 批量选择：选中当前筛选结果 / 清空选择（不会选中隐藏、in-use、受保护项）。
- [x] 4.3 行内显示修改时间与命中规则；`BridgeProject` 保留 `matched_rule`。
- [x] 4.4 扫描中不实时重排（结束时排序一次）；清理开始/结束不强制切页。顺带修复：GUI 执行清理时带上与扫描一致的 `--recent-days`/`--include-recent`，否则"最近修改"项会被全部跳过。
- [x] 4.5 Recommendations 覆盖全部扫描根（bridge `recommend` 支持多路径、去重），使用扫描深度设置，生成中有进度，放回侧栏。
- [x] 4.6 History 日期格式化、可查看单次运行明细；Trash 恢复结果有反馈、清空失败（如卷未挂载）有提示、条目不截断。顺带修复：按钮禁用态、History 筛选控件标签挤压、窄窗口工具栏折行。
- [x] 4.7 Settings 统一即时保存。

### 阶段 5：CLI
- [x] 5.1 `scan` 在非 TTY 下也输出结果列表（结果总是写 stdout，进度条在 stderr）；多根目录逐根列出，不再自动切换成 JSON。
- [x] 5.2 `clean` 默认移入回收站（新增 `--permanent`，`--trash` 保留兼容）；新增 `--yes`（只跳过确认）和 `--include-in-use`（只放行 in-use），`--force` 保持原义。
- [x] 5.3 只读的 `trash list/show` 和 gc dry-run 不再写审计记录；purge/gc 记录真实条数、字节数和 mode。
- [x] 5.4 （评审外，验证中发现）
  - 扫描根统一转绝对路径：此前 `dev-cleaner clean`（默认 `.`）把相对路径写进回收站日志，换目录 `undo` 会恢复到错误位置；plan、审计同样受影响；`keep_paths` 对相对路径扫描完全不生效。
  - CLI `clean`/`apply` 审计按每项真实结果写入（completed/failed/skipped+原因/dry_run），不再统一写 "attempted"。
  - `apply` 同样默认移入回收站，新增 `--permanent`、`--yes`。
  - TUI 按 Enter 直接清理且无确认：改为移入回收站并写审计，提示文案同步。
  - dry-run 汇总区分 "Would move to trash" 与 "Would free"。

### 阶段 6：工程
- [x] 6.1 CI：`.github/workflows/ci.yml`，Ubuntu + macOS 跑 fmt、clippy（暂不 `-D warnings`，存量 45 条）、`cargo test --workspace --locked`，macOS 跑 `swift test`。修复了只在 macOS 通过的 metrics 测试（写死 `Library/Application Support`，Linux 走 `XDG_CONFIG_HOME`）。已在 Linux 容器验证并模拟 runner 的共享 XDG 目录，无泄漏。
- [x] 6.2 AGENTS.md / README / QUICKSTART / spec 同步（测试命令改为 `--workspace`、克隆地址占位符、默认移入回收站与新参数、剪枝/git 保护/占用块、按卷回收站、macOS App 与 bridge、过时的模块路径、最低 Rust 版本 1.85）。

### 阶段 7（取消）：全局开发缓存模块
- [-] 7.1 独立、默认关闭；覆盖 Xcode DerivedData、包管理器缓存等，优先调用官方清理命令。
  - 取消（2026-11-04）：本项目定位为 Mole 的补充，`mo clean` 已覆盖全局缓存。

### 阶段 8：Mole 之外的检测补齐（2026-11-04）
- [x] 8.1 多类型检测：`detect_all` 返回所有命中类型，混合根目录（如 Node+Python）每个生态的规则都生效；in-use 检查所有类型；裸 `Makefile` 只在无其他类型时归为 C++。实测 `~/work` 多发现 1 项（Node+Python 项目的 `.venv`）。
- [x] 8.2 补 Mole 也没有的规则：Node `.vite`/`.nyc_output`/`storybook-static`，Python `.hypothesis`，Terraform `.terraform`，Elm `elm-stuff`，Unity `Library`/`Temp`/`obj`。不做：`bazel-*`、Nix `result`（符号链接）、`.vercel`/`.netlify`（含项目链接配置）、`.ipynb_checkpoints`（笔记本备份）。
- [x] 8.3a 修复：`.gitignore` 里的 `.worktrees/` 等目录连同其中 worktree 的未提交改动会被列为可清理项（外层仓库 `ls-files` 看不到 worktree 文件）。现在任意深度内有登记的 worktree、或非 builtin 目标三层内有 `.git`（独立 clone）都不作为候选；扫描与 apply 复核共用。`~/work` 实测结果与耗时不变。
- [-] 8.3b 进入 `<repo>/.claude/worktrees`、`~/.codex/worktrees` 扫描其中产物。取消（2026-11-09）：`mo purge` 已覆盖（实测 `scan_purge_targets`，`fd --hidden` 会进入隐藏目录）。Mole 的缺口只是搜索根以下 6 层的深度限制，与 worktree 无关，可用 `~/.config/mole/purge_paths` 把深层项目加为搜索根。
- [ ] 8.3c（搁置，2026-11-09）废弃 git worktree 清理（Mole 明确不删 worktree）。本机实测可安全删除的只有约 35 MB，等废弃 worktree 占用明显空间时再做；届时需先定"废弃"标准。
