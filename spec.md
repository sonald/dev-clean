# Dev Cleaner 产品规格（spec.md）

> 目标：面向个人开发者的本地开发产物清理工具，优先清理项目内可再生目录，并保持安全、可解释、可回滚。
>
> 本文档按最新代码实现更新，是“产品/技术合一”的当前规格说明：描述已经落地的用户体验、命令边界、核心数据结构与后续扩展点。

## 0. 文档信息

- 状态：Current implementation spec
- 适用版本：`dev-cleaner` 当前代码基线（plan schema v3）
- 目标平台：Linux、macOS（Windows 代码层面部分路径逻辑可兼容，但不是当前优先验证平台）
- 核心场景：扫描、解释、选择、清理、推荐、计划执行、trash 回滚、审计
- 形态：CLI/TUI（`dev-cleaner`）与 macOS SwiftUI App（`macos/DevCleanerMac`，通过 `dev-cleaner bridge` 调用同一套核心逻辑）

---

## 1. 产品定位

Dev Cleaner 是一个本地优先的开发者空间清理工具：

- 快：使用 `ignore::WalkBuilder` 做 ripgrep 风格遍历，支持流式候选发现与并行 size 计算。
- 准：通过项目 marker files、内置规则、用户自定义规则、`.gitignore` 保守发现和 CMake 启发式识别可清理目标。
- 稳：默认 `--max-risk medium`，隐藏高风险依赖目录和 `.gitignore` 发现项；默认阻止 protected/recent/in-use 目标。
- 可解释：每个 `ProjectInfo` 可携带 category、risk、confidence、matched_rule、protected/recent/selection/skip reason。
- 可回滚：clean/apply/TUI/macOS App 默认把目标移入 Dev Cleaner 回收站（可恢复批次），`--permanent` 才直接删除；支持 `undo` 和 `trash list/show/purge/gc`。
- 可审计：clean/apply/TUI 以及 restore/purge/gc 记录本地 JSONL audit log（逐项真实结果），可查询和导出。

---

## 2. 核心概念

- Project Root：通过 marker files 识别出的项目目录，例如 `package.json`、`Cargo.toml`、`pyproject.toml`。
- Cleanable Target：项目根内可再生目录，例如 `target/`、`node_modules/`、`.venv/`、`dist/`。
- Rule：目标被判定为 cleanable 的依据。来源为 `custom`、`builtin`、`gitignore`、`heuristic`。
- Category：清理类型，当前为 `cache`、`build`、`deps`、`unknown`。
- Risk Level：`low`、`medium`、`high`。CLI 默认最多显示/选择 `medium`。
- Confidence：`high`、`medium`、`low`、`unknown`，由规则来源派生，主要用于解释和机器输出。
- Protection：keep policy 命中时标记为 protected，默认不可见/不可清理，除非显式 include/force。
- Recent：最近修改目标，默认隐藏/跳过；阈值默认 7 天，可通过 `--recent-days` 调整。
- Batch：一次 trash 操作的逻辑批次，用于 undo、show、purge、gc。

---

## 3. 当前命令规格

全局参数：

- `--config <PATH>`：指定 TOML 配置文件，否则使用 `Config::default_path()`。
- `--profile <NAME>`：使用 config 中的命名 scan profile；与命令位置参数 `[PATH]` 互斥。

### 3.1 `scan [PATH]`

扫描 cleanable targets，使用 streaming UX：

- 过滤参数：`--depth`、`--min-size <MB>`、`--older-than <DAYS>`、`--gitignore`。
- 元数据过滤：`--category cache|build|deps|all`，默认 `all`。
- 风险过滤：`--max-risk low|medium|high|all`，别名 `--risk`，默认 `medium`。
- 可见性：`--include-protected`、`--include-recent`、`--recent-days <DAYS>`。
- 输出：普通表格、`--json`、`--explain`。结果列表总是写 stdout（进度条在 stderr），管道/重定向/CI 下同样完整。
- `--profile` 多根时逐根列出（重叠根去重）；只有 `--json` 才输出 JSON。

实现要点：

- 扫描根先转为绝对路径（不解析 symlink），结果、plan、trash log、audit 中的路径与当前工作目录无关。
- CLI 路径走 `Scanner::scan_with_streaming()`，先发现候选，再并行计算 size 并流式输出。
- size threshold 在 size 计算完成后由 CLI 过滤。
- `--gitignore` 只控制 traversal 是否尊重 `.gitignore`；`.gitignore` 发现候选是独立能力。

### 3.2 `clean [PATH]`

扫描后清理目标：

- 支持 scan 同类过滤：`--depth`、`--min-size`、`--older-than`、`--gitignore`、`--category`、`--max-risk`。
- 执行模式：默认移入 Dev Cleaner 回收站；`--permanent`（别名 `--permanent-delete`）直接删除；`--trash` 保留兼容，与 `--permanent` 互斥；另有 `--dry-run`、`--auto`（别名 `--yes`/`-y`）、`--force`、`--verbose`、`--share`。
- 安全覆盖：`--include-in-use`、`--include-recent`、`--include-protected`、`--force-protected`、`--recent-days`。

行为：

- 默认不会清理 in-use、protected、recent 目标。
- `--auto`/`--yes` 跳过选择与确认，但仍跳过 in-use；`--include-in-use` 只放行 in-use，仍需确认；`--force` 放行 in-use 并跳过确认；`--force-protected` 才允许 protected 目标进入删除。
- TTY 且未 `--auto/--force` 时使用键盘选择器；非 TTY 时退化为传统 prompt。
- 回收站是 Dev Cleaner 自有 trash root，而不是系统 Trash；dry-run 汇总区分 "Would move to trash" 与 "Would free"。
- `--share` 输出可复制摘要，并写入本地 metrics event。

### 3.3 `tui [PATH]`

启动 ratatui 全屏交互模式：

- 支持 `--include-recent`、`--include-protected`、`--recent-days`。
- 展示目标列表、详情、risk/source/protection 信息，并使用相同 safety 语义。
- Enter 没有二次确认，因此把选中项移入 Dev Cleaner 回收站（可 `undo`），并写入 audit；结束时打印 undo/purge 命令。
- 相比 `scan`/`clean`，TUI 的命令行过滤面较窄；仍是后续体验增强点。

### 3.4 `stats [PATH]`

生成清理统计：

- 支持 `--depth`、`--top`、`--json`、`--gitignore`、`--category`、`--max-risk`、`--include-recent`、`--include-protected`、`--recent-days`。
- 输出总量、类型/年龄/大小等 breakdown 和轻量 recommendations。

### 3.5 `plan [PATH]`

生成 machine-readable cleanup plan：

- 支持 `scan` 同类过滤和可见性参数。
- `-o/--output <PATH>` 写入 JSON；未指定时打印到 stdout。
- 当前生成 `schema_version = 3`，并写入 `tool_version`、`created_at`、`scan_root`、`params`、`projects`。

### 3.6 `recommend [PATH]`

基于目标空间生成推荐清单，不执行删除：

- 目标：`--cleanup <SIZE>` 或 `--free-at-least <SIZE>`。
- 策略：`--strategy safe-first|balanced|max-space`，默认 `safe-first`。
- 安全覆盖：`--include-in-use`、`--include-recent`、`--include-protected`、`--recent-days`。
- 过滤：`--depth`、`--min-size`、`--older-than`、`--gitignore`、`--category`、`--max-risk`。
- 输出：普通摘要、`--json`、`--explain`、`--output-plan <PATH>`。

推荐逻辑：

- 先过滤超出 max-risk、in-use、protected、recent 的候选，并累计 blocked summary。
- 按策略打分排序：
  - `safe-first`：更重视低风险和更久未改动。
  - `balanced`：兼顾 size、age、risk。
  - `max-space`：更重视释放空间。
- 累加候选直到达到 target bytes。
- 被选中目标写入 `selection_reason`，被挡住目标按类别统计。

### 3.7 `apply <PLAN>`

执行 plan JSON：

- 默认移入 Dev Cleaner 回收站；`--permanent` 直接删除（`--trash` 保留兼容）。
- 支持 `--dry-run`、`--force`（跳过确认并放行 in-use）、`-y/--yes`（只跳过确认）、`--no-verify`、`--include-recent`、`--force-protected`、`--recent-days`、`--verbose`。
- 接受 schema v1/v2/v3；当前生成 v3。
- 默认会重新验证 plan 目标：
  - cleanable_dir 必须位于 project root 下。
  - absolute scan_root 下的目标必须仍位于 scan_root 下。
  - 重新用 scanner 校验规则仍匹配。
  - 重新计算 keep/recent/in-use safety。
- `--no-verify` 会跳过规则重验，但仍做路径归一化和 safety 处理。

### 3.8 `undo`

恢复 trash batch：

- `undo --batch <ID>`；未指定 batch 时恢复最新 batch。
- 支持 `--dry-run`、`--force`、`--verbose`。
- restore 时更深路径优先，目标已存在时默认跳过，`--force` 会覆盖。
- 恢复成功的条目从 trash log 移除，不再计入 `trash list` 和回收站大小；批次全部恢复后删除空的批次目录。

### 3.9 `trash`

管理 Dev Cleaner trash：

- `trash list [--top N] [--json]`
- `trash show --batch <ID> [--json]`
- `trash purge --batch <ID> [--force]`
- `trash gc [--keep-days N] [--keep-gb N] [--dry-run]`

Trash root：

- 默认：`dirs::data_dir()/dev-cleaner/trash`，fallback 到 home/current-dir 风格路径。
- 可通过 `DEV_CLEANER_TRASH_DIR` 覆盖。
- 移入回收站只做同卷 rename：与 trash root 不在同一卷的目标放进该卷根目录的 `.dev-cleaner-trash/<batch>/`，不会跨卷复制（复制既不释放空间又会占满系统盘）。restore 仍允许跨卷复制，以兼容旧批次。
- purge/gc 同时删除各卷上的批次目录；卷未挂载（挂载点不存在或为空）时 purge 失败并保留记录，重新挂载后可再次 purge。
- purge/gc 拒绝清理 symlink batch path。
- `trash list/show` 与 `gc --dry-run` 是只读操作，不写 audit。

### 3.10 `profile`

管理命名扫描配置：

- `profile list`
- `profile show <NAME>`
- `profile add <NAME> --path <PATH>... [--depth N] [--min-size-mb N] [--max-age-days N] [--gitignore] [--category ...] [--max-risk ...]`
- `profile remove <NAME>`

Profile 存储在 config 的 `[scan_profiles.<name>]` 下。使用 `--profile` 时不可同时传 `[PATH]`。

### 3.11 `audit`

查询本地 audit log：

- `audit list [--top N] [--json]`
- `audit show --run <RUN_ID> [--json]`
- `audit export [--run <RUN_ID>] [--format json|csv] [-o <PATH>]`

Audit 默认开启，日志会按大小轮转：

- clean/apply/TUI：run start（含 `mode`：`dry_run`/`trash`/`permanent_delete`）、每个目标的真实结果（`completed`/`failed`/`skipped`+原因/`dry_run`）、run finish。
- restore（`undo`）、purge、gc 各记一条 run（`mode` 为 `restore`/`purge`/`gc`），带真实条数与字节数。

### 3.12 `bridge` 与 macOS App

`dev-cleaner bridge <scan|recommend|clean|apply|trash|audit|config>` 输出 JSONL 事件，供 `macos/DevCleanerMac` 使用：

- App 偏好（扫描根、深度、recent 天数、是否显示高风险、回收站保留天数/提醒阈值、自动清理过期批次）保存在 bridge 的 GUI preferences 文件中，修改即保存。
- App 的所有清理都先展示确认表单，列出将处理的条目，并且只移入回收站（可先 Dry Run）。执行时传入与扫描一致的 `--recent-days`/`--include-recent`。
- 多扫描根：plan 的 `scan_root` 取各根的公共父目录；`bridge recommend` 接受多个路径并去重。
- 启动时按保留天数自动清理过期批次（只按天数，不按大小，避免刚移入的批次被清掉）。

---

## 4. 扫描与匹配系统

### 4.1 ProjectType

当前内置项目类型：

- Node.js、Rust、Python、Java、Kotlin、Scala、Clojure、Dart、Haskell、Go、C、C++、Ruby、Swift、PHP、Elixir、.NET、Maven、Gradle、Terraform、Elm、Unity、Generic。

检测方式：

- marker file 优先，例如 `package.json`、`Cargo.toml`、`pyproject.toml`、`pom.xml`、`build.gradle`、`go.mod`、`Package.swift`、`composer.json`、`mix.exs` 等。
- 一个目录可同时命中多个类型（`ProjectDetector::detect_all`），匹配时依次尝试每个类型的规则；in-use 检查所有命中类型的锁文件。
- Terraform 用 `*.tf`，Elm 用 `elm.json`，Unity 用 `ProjectSettings/ProjectVersion.txt`（in-use：`Temp/UnityLockfile` 存在即编辑器打开中）。
- C/C++ 使用 `CMakeLists.txt`；单独的 `Makefile` 只在没有其他类型命中时才归为 C++（避免给 Go/Python 等项目加上 `out` 规则）。
- Generic 主要用于 `.gitignore` 发现项等没有明确项目类型的目标。

### 4.2 Cleanable Patterns

当前主要内置规则：

- Node.js：`node_modules`、`.next`、`.nuxt`、`dist`、`build`、`.cache`、`.turbo`、`.parcel-cache`、`.vite`、`.nyc_output`、`storybook-static`
- Rust：`target`
- Python：`.venv`、`venv`、`__pycache__`、`.pytest_cache`、`.mypy_cache`、`.tox`、`.hypothesis`、`*.egg-info`、`.eggs`、`build`、`dist`
- Java/Maven：`target`、`out`
- Kotlin/Gradle：`build`、`.gradle`、`out`
- Scala：`target`、`project/target`
- Clojure：`target`
- Dart：`build`、`.dart_tool`
- Haskell：`dist`、`dist-newstyle`、`.stack-work`
- Go：`vendor`、`bin`
- C/C++：`build`、`cmake-build-debug`、`cmake-build-release`、`out`
- Ruby：`vendor/bundle`、`.bundle`
- Swift：`.build`、`DerivedData`、`.swiftpm`
- PHP：`vendor`
- Elixir：`_build`、`deps`
- .NET：`bin`、`obj`
- Terraform：`.terraform`（deps，high risk）
- Elm：`elm-stuff`
- Unity：`Library`、`Temp`、`obj`

范围说明：本项目定位为 Mole（`mo clean` / `mo purge`）的补充，Mole 已覆盖的全局缓存和项目产物规则不重复实现。`bazel-*`、Nix `result` 是符号链接，删除不释放空间，不纳入。

### 4.3 `.gitignore` 的两种语义

Traversal：

- 默认不尊重 `.gitignore`，这样才能扫描常见 gitignored 产物目录。
- `--gitignore` 开启后，walker 会跳过 gitignored entries。

Discovery：

- 对 Git project，`ProjectDetector::parse_gitignore` 会保守提取“像目录”的 ignore pattern。
- 跳过空行、注释、negation、明显文件 pattern、复杂 wildcard、VCS/source 目录、常见 dotfile。
- `.gitignore` 发现项强制 `risk=high`、`confidence=low`，默认不会出现在 `--max-risk medium` 的结果中。

### 4.4 Custom Patterns

Config 中的 `custom_patterns` 已接入扫描引擎：

```toml
[[custom_patterns]]
name = "Unity"
directory = "Library"
marker_files = ["Assets", "ProjectSettings"]
marker_mode = "all_of"
```

- `directory` 支持 basename/relative path/glob 风格匹配。
- `marker_files` 用于判断自定义项目根。
- `marker_mode` 为 `any_of` 或 `all_of`，默认 `any_of`。
- 命中后 `RuleSource::Custom`，confidence 为 high，并保留 `project_name`。

### 4.5 Category / Risk / Confidence

Category 按目录名和相对路径静态分类：

- `deps`：`node_modules`、`.venv`、`venv`、`vendor`、`deps`、`.bundle`、`vendor/bundle`。
- `cache`：`__pycache__`、`.pytest_cache`、`.mypy_cache`、`.tox`、`.eggs`、`.cache`、`.dart_tool`、`.turbo`、`.parcel-cache`。
- `build`：`target`、`build`、`dist`、`dist-newstyle`、`out`、`_build`、`.stack-work`、`DerivedData`、`.build`、`.next`、`.nuxt`、`.gradle`、`.swiftpm`、`bin`、`obj`、`cmake-build*`、`*.egg-info`。

默认 risk：

- `cache = low`
- `build = medium`
- `deps = high`
- `unknown = medium`
- `gitignore` source 无论 category 都强制 high

Confidence：

- `custom` / `builtin` = high
- `heuristic` = medium
- `gitignore` = low
- 未归因 = unknown

### 4.6 遍历剪枝与安全护栏

- 不进入隐藏目录（扫描根本身除外）和 `node_modules` 内部继续寻找项目，避免把依赖包或工具缓存里的 `dist/`、`build/` 当成目标；`custom_patterns` 中以 `.` 开头的路径组件（如 `.yarn/cache` 的 `.yarn`）仍会进入。
- 目标目录内若有 git 跟踪的文件（`git ls-files` 非空），视为源码，不作为 cleanable target。
- 目标目录是或包含 git checkout 时也不作为 target：任意深度内有登记在外层仓库的 linked worktree（`git worktree list`），或非 builtin 规则命中的目标在三层以内有 `.git`（独立 clone）。外层仓库的 `ls-files` 看不到这些文件，否则 `.gitignore` 里的 `.worktrees/` 连同未提交改动会被列为可清理项（`ProjectDetector::contains_git_checkout`）。
- size 为实际占用的磁盘块（`st_blocks * 512`），稀疏文件不会被高估；硬链接只有在所有链接都位于目标内时才计入。

### 4.7 Dedup

扫描会去重 nested cleanable dirs：

- 若 `A` 是 `B` 的上层路径，且二者都 cleanable，默认只保留上层 `A`。
- 典型例子：保留 `.venv/`，不重复展示 `.venv/lib/.../__pycache__/`。
- 多 root/profile 聚合后，`ScanService` 也会再次 deduplicate evaluated projects。

---

## 5. Safety Policy

### 5.1 In-use

`ProjectDetector::is_in_use` 基于 lock files 最近 7 天修改判断：

- Node.js：`package-lock.json`、`npm-shrinkwrap.json`、`yarn.lock`、`pnpm-lock.yaml`、`bun.lock`、`bun.lockb`
- Rust：`Cargo.lock`
- Python：`Pipfile.lock`、`poetry.lock`、`uv.lock`、`pdm.lock`
- Dart：`pubspec.lock`
- Haskell：`stack.yaml.lock`
- Go：`go.sum`
- Ruby：`Gemfile.lock`
- PHP：`composer.lock`
- Swift：`Package.resolved`
- Elixir：`mix.lock`
- .NET：`packages.lock.json`

clean/apply 默认跳过 in-use；`--include-in-use` 或 `--force` 才允许。

### 5.2 Recent

Recent 是 cleanable target 的 `last_modified` 与当前时间比较：

- 默认 recent threshold 为 7 天。
- scan/stats/plan/recommend/clean/apply 默认隐藏或跳过 recent。
- `--include-recent` 显示/允许 recent；`--recent-days` 调整阈值。

### 5.3 Keep / Protect

Keep policy 已落地：

- 项目根存在 `.dev-cleaner-keep`：保护整个项目。
- 项目根存在 `.dev-cleaner-keep-patterns`：按相对 pattern/glob/绝对 path 保护匹配目标；解析失败时保守保护。
- Config `keep_paths`：精确路径，父子路径均视为匹配。
- Config `keep_globs`：glob path 保护。
- Config `keep_project_roots`：项目根 glob 保护。

scan/stats/plan/recommend 默认不显示 protected；clean/apply 默认不删除 protected。需要：

- `--include-protected`：让 protected 出现在候选/结果里。
- `--force-protected`：允许 clean/apply 删除 protected。

---

## 6. 数据结构与持久化

### 6.1 `ProjectInfo`

当前核心字段：

- `root`
- `project_type`
- `project_name`
- `category`
- `risk_level`
- `confidence`
- `matched_rule`
- `cleanable_dir`
- `size`
- `size_calculated`
- `last_modified`
- `in_use`
- `protected`
- `protected_by`
- `recent`
- `selection_reason`
- `skip_reason`

### 6.2 `CleanupPlan`

当前 schema：

- `schema_version: 3`
- `tool_version`
- `created_at`
- `scan_root`
- `params`
- `projects: Vec<ProjectInfo>`

`PlanParams` 当前字段：

- `cleanup_bytes`
- `free_at_least_bytes`
- `max_risk`
- `category`
- `verify_mode`
- `strategy`
- `recent_days`

Apply 兼容读取 schema v1/v2/v3，但新生成 plan 使用 v3。

### 6.3 Trash Log

Trash JSONL entry：

- `batch_id`
- `created_at`
- `original_path`
- `trashed_path`
- `size`
- `tool_version`

`trash list` 会按 batch 聚合 `entries_count` 和 `total_size`。`original_path` 总是绝对路径；`trashed_path` 可能位于其他卷的 `.dev-cleaner-trash/` 下。恢复成功的条目会从 log 中移除。

### 6.4 Audit Log

Audit record 是 tagged JSONL：

- `run_started`（可选 `mode`）
- `item_action`（`path`、`action`、`result`、`bytes`、`reason`）
- `run_finished`

默认路径为 `dirs::data_dir()/dev-cleaner/operations.jsonl`，可通过 config `[audit]` 覆盖：

- `enabled`
- `path`
- `max_size_mb`

### 6.5 Metrics Event

当前 metrics 仅用于本地事件日志，例如 `--share` 写入 `share_generated`：

- 默认路径：config dir 下 `dev-cleaner/events.jsonl`
- fallback：当前目录 `.dev-cleaner-events.jsonl`
- 不联网，不上传。

---

## 7. 配置规格

默认路径：

- Linux/macOS：`dirs::config_dir()/dev-cleaner/config.toml`
- fallback：`.dev-cleaner.toml`

Config 字段：

- `exclude_dirs`: walker 剪枝，默认 `.git`、`.svn`、`.hg`。
- `custom_patterns`: 用户自定义 cleanable rule。
- `default_depth`
- `min_size_mb`
- `max_age_days`
- `scan_profiles`
- `keep_paths`
- `keep_globs`
- `keep_project_roots`
- `[audit]`

Profile 字段：

- `paths`
- `depth`
- `min_size_mb`
- `max_age_days`
- `gitignore`
- `category`
- `max_risk`

优先级：

- 命令行显式参数优先。
- 其次 profile。
- 再其次 config default。
- 未设置时使用代码默认值，例如 `max_risk = medium`、`gitignore = false`。

---

## 8. 非功能需求与约束

性能：

- 遍历使用 `ignore` 并行 walker。
- size 计算使用 parallel/streaming 组件，并有 timeout 保护。
- `DEV_CLEANER_PERF_TRACE=json` 可输出阶段性能 trace 到 stderr，不污染正常 JSON 输出。

安全：

- 默认 exclude VCS 目录：`.git`、`.svn`、`.hg`。
- 扫描不 follow symlink；size 计算也不 follow symlink。
- apply 会校验 cleanable_dir 位于 project root 和 scan root 内。
- clean/apply/TUI 默认移入回收站，永久删除需要显式 `--permanent`；App 只提供移入回收站，释放空间通过回收站页的 purge/清空（需确认）。
- 含 git 跟踪文件的目录不会被当作清理目标。
- trash purge/gc 拒绝 symlink batch path。
- 删除失败继续处理其他目标并汇总错误。

隐私：

- 默认所有扫描、计划、trash、audit、metrics 均在本机完成。
- 无联网行为。

可维护性：

- CLI 编排在 `src/cli/mod.rs`。
- 可测试业务逻辑集中在 `crates/dev-cleaner-core/src/{app,policy}/*`、`recommend.rs`、`trash.rs`。
- Scanner 仍是核心发现引擎，配置和 safety 由 app service 统一拼接。

---

## 9. 模块地图

核心库 `crates/dev-cleaner-core/src/`：

- `app/scan.rs`：profile/config/path/filter 解析（根转绝对路径），scanner 构建，keep/recent 评估，跨 root dedup。
- `app/cleanup.rs`：clean/apply 前的 selected vs blocked 分流。
- `app/apply_plan.rs`：plan schema 校验、目标重验、apply safety 校验。
- `evaluation.rs`：`EvaluatedProject`、`SafetyFlags`、skip/selection reason。
- `scanner/walker.rs`：遍历与剪枝、候选发现、规则归因、streaming scan、dedup、risk/category/confidence。
- `scanner/detector.rs`：project type detection、内置 cleanable patterns、`.gitignore` discovery、in-use lock 检测、git 跟踪文件检测。
- `scanner/size_calculator.rs`：并行/流式 size 计算（占用块、硬链接）。
- `policy/keep.rs`：keep/protect policy。
- `recommend.rs`：goal-based recommendation。
- `cleaner/mod.rs`：实际 remove/trash 执行、progress、结果汇总。
- `trash.rs`：trash batch（含按卷存放）、undo、list/show/purge/gc。
- `plan.rs`：CleanupPlan schema v3。
- `audit.rs`：本地 JSONL audit log。
- `stats/mod.rs`：统计聚合。
- `config/mod.rs`：TOML config、profiles、custom patterns、audit config。

应用 `src/`：

- `cli/mod.rs`：clap 命令、输出、交互确认、命令编排。
- `bridge.rs`：macOS App 使用的 JSONL bridge 与 GUI preferences。
- `clean_progress.rs`：终端进度 observer，以及逐项写 audit 的 observer。
- `tui/`：ratatui 全屏 UI。
- `interactive/select.rs`：TTY keyboard selector。
- `stats/mod.rs`：统计的终端展示。
- `metrics.rs`：本地 metrics event log。

macOS App `macos/DevCleanerMac/`：SwiftUI 界面、`AppModel` 状态与 `BridgeClient`。

---

## 10. 添加或修改检测规则

新增内置项目类型：

1. 在 `crates/dev-cleaner-core/src/scanner/detector.rs` 增加 `ProjectType` variant。
2. 更新 `ProjectType::name()` 和 `ProjectType::color()`。
3. 更新 `ProjectDetector::detect()` marker checks。
4. 更新 `ProjectDetector::cleanable_dirs()`。
5. 如目录分类不正确，更新 `classify_category()`。
6. 如可判断活跃状态，更新 `ProjectDetector::is_in_use()`。
7. 更新 tests 和 README/spec 中的支持列表。

新增用户规则优先使用 config `custom_patterns`，避免把个人/小众工具链直接做成 builtin。

---

## 11. 已知限制与后续路线

当前仍值得改进的点：

1. TUI 的过滤、搜索、排序、流式增量更新能力仍不如 CLI 完整。
2. Recent/in-use 主要是启发式；还没有 git activity、lsof/fuser、procfs/libproc provider。
3. Audit 已覆盖 clean/apply，但没有完整 scan/stats/recommend 历史趋势和 top-growth。
4. 全局开发缓存不做：由 Mole `mo clean` 覆盖。
5. 还没有 ncdu/treemap 风格空间地图。
6. `.gitignore` discovery 仍刻意保守，复杂 pattern 不会全部转成候选。
7. Plan schema v3 已有 params 和 metadata，但未来如果加入更强 audit/replay 语义，需要继续 bump schema。
8. APFS clone（`cp -c` 等）共享的数据块仍按完整大小计入，删除克隆体实际释放的空间可能更少；可用 `getattrlist(ATTR_CMNEXT_PRIVATESIZE)` 改进。

建议路线：

1. TUI 与 selector 统一更多过滤/详情能力。
2. 增强 activity/in-use providers，并把来源展示为 heuristic/verified。
3. 将 audit 扩展为 history/top-growth。
4. （已取消）cache module 由 Mole 覆盖。
5. 增加空间地图或 HTML report。

---

## 12. 快速验证命令

```bash
cargo test --workspace            # 根包 + dev-cleaner-core；单独 `cargo test` 不会跑核心库测试
(cd macos/DevCleanerMac && swift test)
cargo run -- scan . --depth 2 --explain
cargo run -- scan . --depth 2 --max-risk high
cargo run -- clean . --depth 2 --dry-run
cargo run -- recommend . --cleanup 1GB --output-plan plan.json
cargo run -- apply plan.json --dry-run
cargo run -- trash list --top 5
cargo run -- audit list --top 5
```
