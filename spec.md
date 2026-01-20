# Dev Cleaner 产品规格（spec.md）

> 目标：面向**个人开发者**的系统清理工具（优先“项目产物”），同时支持 **Linux / macOS**。
>
> 本文档是“产品/技术合一”的规格说明：既定义用户体验与功能边界，也给出可落地的实现设计与数据结构建议。

## 0. 文档信息

- 状态：Draft
- 适用版本：`dev-cleaner` `0.1.x`（现状）→ `0.2+`（迭代目标）
- 目标平台：Linux、macOS（Windows 可选延后）
- 核心场景：个人开发者清理项目产物，**安全、可回滚、少配置**

---

## 1. 问题与定位

### 1.1 问题

个人开发者机器常见“磁盘告急”来源：

- 项目构建产物：`node_modules/`、`target/`、`.venv/`、`dist/`、`build/`、`DerivedData/` 等
- 多语言/多工具链并存：同一目录树下混杂 Node/Rust/Python/Java/… 项目
- 产物高度可再生但体积巨大，且分布在多个工作目录
- 手工清理风险高：误删正在使用的项目、删错目录、不可恢复

### 1.2 产品定位（愿景）

Dev Cleaner 是一个“开发者空间管家”：

- **快**：快速扫描 + 并行计算 + 流式反馈
- **准**：识别常见项目类型与产物目录，降低误判
- **稳**：默认安全（保护 in-use/活跃项目），提供 Trash 可回滚
- **省心**：低配置即可用；一键推荐/预算清理

---

## 2. 设计原则（个人用户优先）

1. **默认安全 > 默认激进**
   - 宁可少删一点，也不默认触碰“高风险/高重建成本”目录。
2. **可解释、可预测**
   - 每条清理项都能解释“为什么被选中”（规则来源、风险等级、收益）。
3. **可回滚**
   - 重要的删除默认走 Trash（可选），并提供批次管理与自动清理策略。
4. **少配置**
   - 通过内置规则 + `.gitignore` 发现 + 轻量 config 覆盖，满足 80% 场景。
5. **跨平台一致**
   - Linux/macOS 行为一致；平台特有能力（如占用检测）以“可选增强”形式提供，确保可退化。
6. **隐私优先**
   - 默认不联网、不上传路径/项目名；所有历史与报告保存在本机。

---

## 3. 术语与概念

- **Project Root（项目根）**：通过 marker files（如 `package.json`、`Cargo.toml`）识别出的项目目录。
- **Cleanable Target（可清理目标）**：项目根内的可再生目录（如 `node_modules/`、`dist/`）。
- **Rule（规则）**：将目录判定为可清理的依据，来源可能是：
  - 内置模式（按 ProjectType）
  - `.gitignore` 发现的目录模式（保守筛选）
  - 用户配置（custom patterns / excludes / keep）
  - 启发式（例如 CMake out-of-source build）
- **Risk Level（风险等级）**：清理的“误伤概率/重建成本/不确定性”综合。
- **Batch（批次）**：一次 trash/restore 的逻辑单元，便于回滚与管理。

---

## 4. 现状能力（0.1.x：代码基线）

> 这一节用于对齐“已有能力”与“未来扩展点”，避免重复造轮子。

### 4.1 命令形态

已有命令：

- `scan`：支持流式 size 计算与进度条；支持 `--json`、`--explain`
- `clean`：支持 dry-run、交互选择、`--auto`、`--trash`、`--force`、`--verbose`
- `tui`：基本的全屏选择清理（功能相对简化）
- `stats`：聚合统计 + 推荐；支持 `--json`
- `plan` / `apply`：生成/执行可复用的清理计划 JSON
- `undo`：按 batch 回滚 trash

### 4.2 识别策略

- 通过 marker files 识别 ProjectType（Node/Rust/Python/...）
- 通过内置 cleanable patterns 识别目标目录
- 解析项目根的 `.gitignore` 来发现额外目录模式（保守策略，避免误判）
- 对 nested cleanable dirs 进行去重（只保留最上层目标）
- “in_use” 检测：基于 lock 文件最近 7 天是否修改（启发式）

### 4.3 关键缺口（为 spec 铺垫）

- `config.custom_patterns` / `exclude_dirs` 结构已存在，但尚未接入扫描引擎（当前仅用于 CLI 默认参数）。
- TUI 目前没有过滤/搜索/排序/流式更新能力，与 CLI 体验不一致。
- Trash 缺少 list/purge/gc 的生命周期管理能力。
- “推荐/预算清理”还未形成完整产品化流程（目前 stats 有轻量推荐，但不可直接生成可执行方案）。

---

## 5. 功能需求总览（可做功能清单）

> 以“个人开发者系统清理”为中心，优先项目产物；也包含中长期可扩展功能（标注优先级与可选模块）。

### 5.1 P0（强烈建议优先：价值最大、最贴近个人场景）

#### A. 预算清理（Goal-based Cleaning）

**用户价值**：我只想“释放 X GB”或“把磁盘空闲提升到 Y”，不想手工挑。

设计：

- 新增参数（CLI/TUI 都支持）：
  - `--cleanup <SIZE>`：例如 `--cleanup 10GB`
  - `--free-at-least <SIZE>`：例如 `--free-at-least 50GB`
  - `--max-risk <LEVEL>`：默认 `medium`（避免默认删高风险目录；因此默认不会自动清理 `deps` 类目标）
- 算法：先按风险等级分层，再在同层内按“收益/成本比”排序，直到达成目标。
  - 目标函数（可实现为启发式，不要求最优解）：
    - 优先：低风险、非 in_use、老项目、大目录
    - 约束：不超出 `max-risk`；命中 keep 规则则不可选
- 输出：
  - 以 “推荐清单” + “预计释放空间” + “未达标原因（受风险/keep 限制）” 展示
  - 可直接生成 `plan.json`（复用 `plan/apply`），并支持 `--explain` 输出理由

#### B. 风险分层与“清理档位”

**用户价值**：我希望先删“绝对安全的缓存”，再考虑“重建成本高的依赖”。

设计：

- 目录分类（可配置扩展）：
  - `cache`：`.cache/`、`.pytest_cache/`、`__pycache__/` 等（低风险、收益中等）
  - `build`：`dist/`、`build/`、`out/`、`DerivedData/`、`_build/`（中风险、收益中等/高）
  - `deps`：`node_modules/`、`.venv/`、`vendor/`（默认 risk=`high`；自动清理默认不包含，需显式启用）
  - `toolchain`（可选）：例如 `.gradle/`、`.m2/`（默认不纳入“项目产物”，但可作为模块）
- 新增筛选：
  - `--category cache|build|deps|all`
  - `--risk low|medium|high|all`（默认 `low,medium`）

#### C. Keep/Protect 机制（强安全护栏）

**用户价值**：我有一些项目/目录永远不想被清理。

设计（多层覆盖，越近越优先）：

1. 项目内标记文件：
   - `.dev-cleaner-keep`：项目根存在则该项目下所有目标默认不可清理（除非 `--force`）
   - `.dev-cleaner-keep-patterns`：可选，列出额外 keep 子目录（相对路径或 glob）
2. 用户级配置：
   - `keep_paths`（绝对路径或 glob）
   - `keep_project_roots`（如 `~/work/important/*`）
3. CLI 临时参数：
   - `--exclude <glob>` 可重复
   - `--include <glob>`（用于覆盖 exclude 时需谨慎）

#### D. Trash 生命周期管理（从“可回滚”到“可管理”）

设计：

- 新增命令组 `trash`：
  - `trash list [--top N] [--json]`：按 batch 聚合显示占用
  - `trash show --batch <ID>`：列出 batch 里的条目
  - `trash purge --batch <ID> [--force]`：永久删除某 batch
  - `trash gc [--keep-days N] [--keep-gb M] [--dry-run]`：自动回收旧 batch/超额空间
- 跨设备移动：
  - `rename` 失败（EXDEV）时，自动 fallback：copy → fsync/校验（可选）→ 删除源
- Trash 的默认路径：使用 `dirs` 提供的 data dir（Linux/macOS），并允许 env 覆盖：
  - `DEV_CLEANER_TRASH_DIR=/path/to/trash`

#### E. 配置真正接入扫描（custom_patterns / exclude_dirs）

设计：

- `exclude_dirs`：作为 walker 的剪枝规则（filter_entry），并用于 `--exclude` 的默认补集。
- `custom_patterns`：支持用户自定义“项目类型识别 + 产物目录”：
  - `marker_files`：存在任意/全部？（建议：支持 `any_of`/`all_of`，默认 any）
  - `directory`：目标目录（支持相对路径，如 `vendor/bundle`）与 glob（如 `cmake-build-*`）
  - `name`：展示名称与 JSON 输出字段
- 与 `.gitignore` 发现的模式合并去重，并在 `--explain` 中体现来源优先级。

### 5.2 P1（体验增强：让工具更像“日常助手”）

#### F. TUI 升级（搜索/过滤/排序/详情面板/流式更新）

设计：

- 左侧列表：目标目录（可多列：类型、大小、年龄、in_use、风险）
- 右侧详情：解释（匹配规则来源）、风险原因、预计重建命令提示（可选）
- 顶部筛选条：
  - 搜索：按路径/项目名
  - 过滤：类型、category、risk、in_use、大小/年龄区间
  - 排序：大小/年龄/风险/类型
- 流式 size 计算：
  - 初始列表可先展示 “Calculating...” 并随着 size 结果流入实时更新与重排（可选：重排需稳定策略，避免光标跳动）。

#### G. 项目活跃度判断增强（仍以“保守安全”为主）

在不依赖平台特有 API 的前提下，增强“活跃项目保护”：

- Git 活跃度（可选）：读取项目根 `.git`（或 worktree）关键文件的修改时间：
  - `.git/index`、`.git/HEAD`、`.git/logs/HEAD`
  - 若最近 N 天有变化，可提升风险或默认 exclude
- 编辑器痕迹（可选、保守）：检测 `.vscode/`、`.idea/` 变化仅作为弱信号
- 接口：统一为 `ActivitySignals`，汇总后进入 risk scoring

#### H. 历史与趋势（本地、不上传）

设计：

- 每次 `scan/stats/clean/apply` 记录一条本地 run log：
  - 运行时间、扫描 root、发现数量、总空间、实际释放、错误、工具版本
- 新增命令：
  - `history list [--json]`
  - `history show <RUN_ID>`
  - `history top-growth`（对比近期扫描结果，找增长最快的目录）
- 存储：本地 JSONL 或 SQLite（推荐 JSONL 起步，后续可迁移）

#### I. 预设扫描根（个人用户更省心）

设计：

- `preset` 概念：
  - `projects`：`~/projects ~/work ~/src ~/workspace`（可配置）
  - `home-light`：仅在家目录常见开发目录扫描，避免误扫大媒体库
- CLI：
  - `scan --preset projects`
  - `clean --preset projects --cleanup 10GB`

### 5.3 P2（更酷/更大：可选模块或长期路线）

#### J. 空间地图（ncdu 风格的“产物热力图”）

设计：

- 按项目聚合：每个项目占用、最大目标、category breakdown
- 支持 treemap/层级视图（TUI 或生成 HTML 报告）

#### K. 全局开发缓存清理（默认关闭或单独模块）

> 虽然当前优先“项目产物”，但个人磁盘爆炸常见也来自全局缓存（npm/pip/cargo/docker）。

设计：

- 独立命令 `cache`：
  - `cache scan` / `cache clean` / `cache stats`
- 风险更高，默认强限制：
  - 保留最近 N 天/最近 N 个版本
  - 仅清理“明显可再生/可重建”的缓存项

#### L. 更强 in_use 检测（平台增强，可选）

设计：

- Provider 链：
  1. 纯 Rust 启发式（lock/git activity）
  2. 外部 `lsof`（Linux/macOS）/ `fuser`（Linux）可选：仅对“最终候选清单”执行
  3. 平台 API（长期）：Linux procfs、macOS libproc（可选）
- 结果用于：
  - 默认跳过（除非 `--force`）
  - 在 UI 中标记 “IN USE (verified)” vs “in_use (heuristic)”

#### M. 规则包/生态（团队共享/社区扩展）

设计：

- rule pack（TOML/JSON）可导入导出，包含：
  - marker rules、cleanable patterns、category、默认风险
  - 保护目录与例外
- 未来可选：WASM 插件（限制 IO 权限）

---

## 6. 核心交互与用户体验设计

### 6.1 典型用户旅程（个人用户）

1. **第一次使用**
   - `dev-cleaner scan --preset projects` → 输出总空间、Top N、风险提示
   - 引导：建议先 `--dry-run` 或 `--trash`
2. **一键释放空间**
   - `dev-cleaner clean --preset projects --free-at-least 30GB --trash`
   - 输出：推荐清单 + 需要确认（除非 `--auto --max-risk low`）
3. **后悔/恢复**
   - `dev-cleaner trash list` → 找到 batch
   - `dev-cleaner undo --batch <ID>`
4. **日常维护**
   - `dev-cleaner stats --preset projects`
   - `dev-cleaner trash gc --keep-days 30 --keep-gb 20`

### 6.2 默认策略（建议）

- `clean` 默认行为建议：
  - 不带 `--auto` 时：必须确认 + 可交互选择
  - 个人用户强烈建议默认 `--trash`（或首次运行提示开启）
- `--force` 的语义：
  - 允许清理 in_use/keep 标记（仍需二次确认，避免误触）

---

## 7. CLI 规格（命令与参数）

> 保持现有命令兼容；新增能力尽量通过新命令/新 flag 扩展。

### 7.1 现有命令（保持）

- `scan [PATH]`：发现目标（可 `--json --explain --min-size --older-than --depth --gitignore`）
- `clean [PATH]`：执行清理（可 `--dry-run --trash --auto --force --verbose`）
- `tui [PATH]`
- `stats [PATH]`
- `plan [PATH] -o plan.json`
- `apply plan.json`
- `undo [--batch <ID>]`
- `init-config [PATH]`

### 7.2 新增命令/参数（建议）

#### `recommend`（可选：也可做成 `clean --recommend-only`）

输出推荐清单（不执行），可一键生成 plan：

- `recommend [PATH|--preset] [--cleanup <SIZE>|--free-at-least <SIZE>]`
- `--max-risk <low|medium|high>`
- `--category <cache|build|deps|all>`
- `--output-plan <PATH>`：输出 `plan.json`

#### `trash`（管理生命周期）

- `trash list [--json]`
- `trash show --batch <ID> [--json]`
- `trash purge --batch <ID> [--force]`
- `trash gc [--keep-days N] [--keep-gb M] [--dry-run]`

#### `history`（本地历史）

- `history list [--json]`
- `history show <RUN_ID> [--json]`
- `history top-growth [--days N]`

#### 通用 flags（建议）

这些 flags 应尽可能在 `scan/clean/tui/stats/recommend` 共享：

- `--preset <name>`：替代/叠加 PATH
- `--exclude <glob>`（repeatable）
- `--include <glob>`（谨慎）
- `--category ...`
- `--risk ...`
- `--cleanup / --free-at-least`

---

## 8. 扫描/匹配规则系统设计

### 8.1 规则来源优先级（用于 explain 与冲突处理）

建议优先级（高→低）：

1. `keep` / `protect`（永不清理）
2. 用户显式 `--include/--exclude`
3. 用户 config 的 `custom_patterns`
4. 内置 patterns（ProjectType cleanable dirs）
5. `.gitignore` 发现 patterns（保守，且可标记为 “low confidence”；默认仅用于展示/候选发现，不默认参与自动清理/推荐）
6. 启发式（例如 CMake build dir）

### 8.2 分类（category）与默认风险（risk）

每条 rule/pattern 建议携带：

- `category`：cache/build/deps/...
- `default_risk`：low/medium/high
- `confidence`：high/medium/low（用于 explain 与推荐）

示例（仅示意）：

- `__pycache__`：category=cache, risk=low, confidence=high
- `dist/`：category=build, risk=medium, confidence=high
- `node_modules/`：category=deps, risk=high, confidence=high
- `.gitignore` 中发现的 `tmp/`：category=cache, risk=medium, confidence=low

### 8.3 Dedup 策略

保留“顶层目标”以减少噪音：

- `A` 是 `B` 的前缀路径 → 如果 `A` 与 `B` 均可清理，保留更上层 `A`
- 例：`.venv/` 与 `.venv/lib/.../__pycache__/` 同时命中时保留 `.venv/`

注意：

- 若上层是 `deps` 且下层是 `cache`，是否应保留两者？（可选策略）
  - 默认仍只保留上层，避免“误导用户以为 cache 会单独清理”
  - 高级模式可允许“拆分展示但执行时做互斥”

---

## 9. 风险评分与推荐算法（设计建议）

> 目标：对个人用户可解释、可控、可退化。先用启发式即可。

### 9.1 风险因子

建议因子（0~1 或离散等级）：

- `in_use`：启发式/验证式（强因子）
- `age_days`：越久越低风险（但依赖/工具链可能仍高成本）
- `category`：cache=low < build=medium < deps=high（默认）
- `confidence`：低置信度（如 `.gitignore` 发现）提升风险
- `project_activity`：近期 git 活跃 → 提升风险
- `size`：越大越“值得清理”（影响收益，不直接影响风险）

### 9.2 推荐排序（示意）

可用一个简单的 score：

- `risk_score`（越大越危险）：
  - base(category) + penalty(in_use) + penalty(activity) + penalty(low_confidence)
- `value_score`（越大越值得清理）：
  - log(size) + bonus(old_age)

推荐逻辑：

1. 过滤：`keep`、超出 `max-risk`、`in_use`（除非 `--force` 或 `--include-in-use`）
2. 分层：先 low risk，再 medium，再 high（默认不进入 high）
3. 排序：同层按 `value_score` 降序
4. 预算：累加 size 直到满足 `cleanup/free-at-least`

Explain 输出应包含：

- 命中规则（来源 + pattern）
- risk 的构成（category、in_use、activity、confidence）
- 推荐理由（size/age 等）

---

## 10. 数据结构与持久化格式

### 10.1 扩展 `ProjectInfo`（建议，保持向后兼容）

现有字段：

- `root`、`project_type`、`cleanable_dir`、`size`、`last_modified`、`in_use`

建议新增（可选）：

- `category: String`（cache/build/deps）
- `risk_level: String`（low/medium/high）
- `risk_score: f32`
- `confidence: String`
- `matched_rule: RuleRef`（含来源与 pattern）
- `activity: ActivitySignals`（用于 explain）

### 10.2 `CleanupPlan` schema（建议）

当前 schema_version=1，仅包含 projects 列表。

建议 schema_version=2+：

- 写入 tool 版本、生成参数（filters/budget）、risk 策略、是否包含 in_use
- 存储 `matched_rule`，使 apply 时可解释与审计

### 10.3 Trash log（建议增强）

当前为 JSONL：

- `batch_id`、`created_at`、`original_path`、`trashed_path`、`size`

建议增强：

- `tool_version`
- `hostname`（可选）
- `project_type/category`（用于统计与 list）

### 10.4 History run log（建议）

格式：JSONL 或 SQLite

字段建议：

- `run_id`、`created_at`、`command`、`roots`、`filters`
- `found_count`、`found_bytes`
- `cleaned_count`、`freed_bytes`、`trash_batch_id`
- `errors_count`、`duration_ms`、`tool_version`

---

## 11. 非功能需求（NFR）

### 11.1 性能

- 目录遍历应支持多线程（现有 `ignore` 并行 walker）
- size 计算应并行且可超时/可取消（现有 streaming + timeout 基础）
- UI（TUI）应避免频繁重排导致光标跳动；可用“稳定排序 + 延迟重排”

### 11.2 安全与边界

- 永远不扫描/不清理：`.git/.svn/.hg` 等 VCS 目录（强约束）
- 删除前验证：
  - 目标必须存在且为目录
  - 目标应位于 scan_root 或显式 roots 之内（plan/apply 已有此类检查）
  - 不跟随 symlink（扫描已不跟随；清理建议显式拒绝 symlink 目标）
- 错误处理：
  - 删除失败要继续下一个，并聚合错误报告

### 11.3 隐私

- 默认不联网、不采集路径/项目名
- 若将来加入“更新检查”，默认关闭或仅请求版本号，不上传路径

---

## 12. 实施路线图（建议）

> 按“个人用户价值/可控风险/开发成本”排序。

1. **Trash 管理命令（list/purge/gc）+ EXDEV fallback**
2. **Config 接入扫描（exclude_dirs/custom_patterns）+ explain 来源**
3. **Risk/category 体系落地（先静态映射）**
4. **预算清理（recommend + 生成 plan + clean 复用）**
5. **TUI 升级（过滤/搜索/排序/详情）**
6. **History（JSONL）与 top-growth**
7. （可选）in_use providers：lsof/fuser 增强
8. （长期）空间地图与全局缓存模块

---

## 13. 未决问题（Open Questions）

- dedup 的互斥展示策略：上层 deps 与下层 cache 同时命中时，如何在 UI 中解释？
- 历史存储：JSONL vs SQLite（先 JSONL，后续迁移）
- 跨平台占用检测：外部命令依赖与 fallback 策略如何在 UX 中呈现？
