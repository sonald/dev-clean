# Dev Cleaner 0.3 详细设计与实现说明

本文档对应 `dev-clean` 当前实现，覆盖以下能力：

1. Profile 工作区配置
2. Keep/Protect 保护体系
3. Recent 安全预选
4. 审计日志与 `audit` 命令
5. `apply` 二次校验
6. 推荐引擎 v2
7. TUI v2（过滤/搜索/排序/详情）

---

## 1. Profile 工作区配置

### 设计方案
- 在配置层引入 `scan_profiles`，每个 profile 描述多 root 路径和默认参数。
- 在 CLI 层引入全局 `--profile`，并支持 `profile list/show/add/remove`。
- 引入统一解析函数 `resolve_scan_inputs()`，处理 `path` 与 `--profile` 的冲突、默认参数合并。

### 实现位置
- `src/config/mod.rs`
  - `ScanProfile`
  - `Config.scan_profiles`
- `src/cli/mod.rs`
  - `ProfileCommands`
  - `run_profile(...)`
  - `resolve_scan_inputs(...)`

### 关键代码片段
```rust
fn resolve_scan_inputs(
    path: Option<PathBuf>,
    profile: Option<&str>,
    config: &Config,
) -> Result<ResolvedScanInput> {
    match (path, profile) {
        (Some(_), Some(_)) => anyhow::bail!("Use either [PATH] or --profile, not both"),
        (None, Some(name)) => {
            let p = config.scan_profiles
                .get(name)
                .with_context(|| format!("Profile `{}` not found", name))?;
            Ok(ResolvedScanInput::from_profile(p))
        }
        (Some(path), None) => Ok(ResolvedScanInput::from_path(path)),
        (None, None) => Ok(ResolvedScanInput::from_path(PathBuf::from("."))),
    }
}
```

---

## 2. Keep/Protect 保护体系

### 设计方案
- 新增 `KeepPolicy`，统一判断是否保护：
  - `.dev-cleaner-keep`
  - `.dev-cleaner-keep-patterns`
  - `keep_project_roots`
  - `keep_paths`
  - `keep_globs`
- 对解析失败采取安全降级（判为保护）。
- 在 `ProjectInfo` 上增加保护标记字段，不在扫描阶段删除候选。

### 实现位置
- `src/policy/keep.rs`
  - `KeepPolicy`
  - `ProtectionDecision`
- `src/config/mod.rs`
  - `keep_paths/keep_globs/keep_project_roots`
- `src/scanner/mod.rs`
  - `ProjectInfo.protected/protected_by`
- `src/cli/mod.rs`
  - `enrich_project_flags(...)`
  - `--include-protected` / `--force-protected`

### 关键代码片段
```rust
impl KeepPolicy {
    pub fn evaluate(&self, info: &ProjectInfo) -> ProtectionDecision {
        if info.root.join(".dev-cleaner-keep").exists() {
            return ProtectionDecision {
                protected: true,
                reason: Some("project_marker:.dev-cleaner-keep".to_string()),
            };
        }
        // ...keep-pattern-file / keep_paths / keep_globs...
        ProtectionDecision { protected: false, reason: None }
    }
}
```

---

## 3. Recent 安全预选

### 设计方案
- 使用 `days_since_modified() < recent_days` 判断 recent。
- 对 recent 默认隐藏（除非 `--include-recent`）。
- `ProjectInfo` 增加 `recent` 字段，便于 JSON 输出和下游命令使用。

### 实现位置
- `src/scanner/mod.rs`
  - `ProjectInfo.recent`
- `src/cli/mod.rs`
  - `--recent-days`
  - `--include-recent`
  - `filter_by_visibility(...)`

### 关键代码片段
```rust
fn enrich_project_flags(projects: &mut [ProjectInfo], keep_policy: &KeepPolicy, recent_days: i64) {
    for project in projects {
        let decision = keep_policy.evaluate(project);
        project.protected = decision.protected;
        project.protected_by = decision.reason;
        project.recent = project.days_since_modified() < recent_days;
    }
}
```

---

## 4. 审计日志与 `audit` 命令

### 设计方案
- 新增 `audit` 模块，使用 JSONL 记录：
  - `RunStarted`
  - `ItemAction`
  - `RunFinished`
- 支持日志轮转（按大小），默认路径：
  - `dirs::data_dir()/dev-cleaner/operations.jsonl`
- CLI 新增：
  - `audit list`
  - `audit show --run`
  - `audit export --format json|csv`

### 实现位置
- `src/audit.rs`
- `src/config/mod.rs`
  - `AuditConfig`（`enabled/path/max_size_mb`）
- `src/cli/mod.rs`
  - `run_audit(...)`
  - `run_clean/run_apply/run_undo/run_trash` 中的日志埋点

### 关键代码片段
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditRecord {
    RunStarted { run_id: String, command: String, ts: String },
    ItemAction { run_id: String, command: String, path: String, action: String, result: String, bytes: u64, reason: Option<String>, ts: String },
    RunFinished { run_id: String, command: String, ts: String, cleaned: usize, skipped: usize, failed: usize, freed_bytes: u64 },
}
```

---

## 5. `apply` 二次校验

### 设计方案
- 计划版本升级至 `schema_version = 3`，并向后兼容读取 `1/2/3`。
- `apply` 默认进行二次校验，支持 `--no-verify` 绕过。
- 校验项：
  - 路径在 `scan_root` 下
  - 目标仍可被当前规则识别（`Scanner::revalidate_target`）
  - 未命中 protect/recent/in_use 阻断（除非强制）

### 实现位置
- `src/plan.rs`
  - `schema_version = 3`
  - `PlanParams.verify_mode/strategy/recent_days`
- `src/scanner/walker.rs`
  - `Scanner::revalidate_target(...)`
- `src/cli/mod.rs`
  - `run_apply(...)`

### 关键代码片段
```rust
pub fn revalidate_target<P: AsRef<Path>>(&self, cleanable_dir: P) -> Option<ProjectInfo> {
    let dir = cleanable_dir.as_ref();
    if !dir.is_dir() {
        return None;
    }
    self.check_directory(dir).filter(|info| info.cleanable_dir == dir)
}
```

---

## 6. 推荐引擎 v2

### 设计方案
- 引入三种策略：
  - `safe-first`
  - `balanced`
  - `max-space`
- 推荐前做阻断统计：
  - `in_use`
  - `protected`
  - `recent`
  - `risk`
- 输出 `blocked` 汇总和每项 `selection_reason`。

### 实现位置
- `src/recommend.rs`
  - `RecommendStrategy`
  - `RecommendOptions`
  - `BlockedSummary`
  - `recommend_projects(...)`
- `src/cli/mod.rs`
  - `--strategy`
  - `run_recommend(...)`

### 关键代码片段
```rust
fn score_project(p: &ProjectInfo, strategy: RecommendStrategy) -> i64 {
    let risk_penalty = match p.risk_level {
        RiskLevel::Low => 0,
        RiskLevel::Medium => 30,
        RiskLevel::High => 80,
    };
    let age_bonus = p.days_since_modified().clamp(0, 365);
    let size_mb = (p.size / (1024 * 1024)) as i64;
    match strategy {
        RecommendStrategy::SafeFirst => age_bonus * 2 + size_mb - risk_penalty * 3,
        RecommendStrategy::Balanced => age_bonus + size_mb * 2 - risk_penalty * 2,
        RecommendStrategy::MaxSpace => size_mb * 4 + age_bonus - risk_penalty,
    }
}
```

---

## 7. TUI v2

### 设计方案
- 引入可视列表 `visible_indices`，避免直接操作原始数据。
- 支持：
  - 搜索（直接输入）
  - 分类过滤（`c`）
  - 风险过滤（`r`）
  - recent/protected 开关（`R`/`P`）
  - 排序切换（`s`）
- 增加详情面板展示 rule/risk/confidence/protect/recent/in_use。

### 实现位置
- `src/tui/mod.rs`
  - `AppState` / `recompute_visible()`
  - `run_tui_projects(...)`
  - 新 UI 布局和键位
- `src/cli/mod.rs`
  - `run_tui(...)`（CLI 包装）

### 关键代码片段
```rust
fn recompute_visible(&mut self) {
    self.visible_indices.clear();
    for (idx, p) in self.projects.iter().enumerate() {
        if !self.include_protected && p.protected { continue; }
        if !self.include_recent && p.recent { continue; }
        // ...category/risk/query...
        self.visible_indices.push(idx);
    }
    self.visible_indices.sort_by(|a, b| /* sort_key */);
}
```

---

## 数据结构与接口变更总览

- `Config`
  - `scan_profiles`
  - `keep_paths/keep_globs/keep_project_roots`
  - `audit`
- `ProjectInfo`
  - `protected/protected_by`
  - `recent`
  - `selection_reason/skip_reason`
- `CleanupPlan`
  - `schema_version = 3`
  - `PlanParams` 新增 `verify_mode/strategy/recent_days`
- CLI
  - 新命令：`profile`、`audit`
  - 新参数：`--profile`、`--include-recent`、`--recent-days`、`--include-protected`、`--force-protected`、`--strategy`、`--no-verify`

---

## 验证状态

- 本地测试：`cargo test`
- 结果：全部通过（40 unit tests + 1 doc test）

