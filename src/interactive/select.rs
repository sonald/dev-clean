use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{
        self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use dev_cleaner_core::scanner::{ProjectInfo, RuleSource};
use dev_cleaner_core::utils::format_size;
use std::cmp::Ordering;
use std::io::{self, stdout, Write};

#[derive(Clone, Copy, Debug, Default)]
pub struct SelectorOptions {
    pub force: bool,
    pub force_protected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortKey {
    Size,
    Age,
    Source,
}

impl SortKey {
    fn next(self) -> Self {
        match self {
            Self::Size => Self::Age,
            Self::Age => Self::Source,
            Self::Source => Self::Size,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Age => "age",
            Self::Source => "source",
        }
    }
}

pub struct ProjectSelector {
    state: SelectorState,
}

impl ProjectSelector {
    pub fn new(projects: Vec<ProjectInfo>, options: SelectorOptions) -> Self {
        Self {
            state: SelectorState::new(projects, options),
        }
    }

    pub fn run(&mut self) -> Result<Option<Vec<ProjectInfo>>> {
        let _terminal = TerminalSession::start()?;
        let mut out = stdout();

        loop {
            let (_, rows) = terminal::size()?;
            let page_rows = self.state.page_rows(rows as usize);
            self.state.ensure_cursor_visible(page_rows);
            self.draw(&mut out)?;

            if let Event::Key(key) = event::read()? {
                if let Some(done) = self.handle_key(key, page_rows)? {
                    return Ok(done);
                }
            }
        }
    }

    fn draw(&self, out: &mut io::Stdout) -> Result<()> {
        let (cols, rows) = terminal::size()?;
        let cols = cols as usize;
        let rows = rows as usize;

        execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;

        let selected_count = self.state.selected_count();
        let selected_size = self.state.selected_size();
        let sort_order = if self.state.reverse { "asc" } else { "desc" };

        write!(out, "Dev Cleaner - Select Targets\r\n")?;
        write!(
            out,
            "Visible: {} | Selected: {} ({})\r\n",
            self.state.visible_indices.len(),
            selected_count,
            format_size(selected_size)
        )?;
        write!(
            out,
            "Sort: {} {} | Search: {}\r\n",
            self.state.sort_key.as_str(),
            sort_order,
            self.state.search_status()
        )?;
        write!(out, "\r\n")?;

        let footer = footer_lines(cols, self.state.search_mode);
        let page_rows = self.state.page_rows(rows);
        let start = self.state.top;
        let end = (start + page_rows).min(self.state.visible_indices.len());

        if self.state.visible_indices.is_empty() {
            write!(out, "No targets match the current filter.\r\n")?;
        } else {
            for row in start..end {
                let project_idx = self.state.visible_indices[row];
                let project = &self.state.projects[project_idx];
                let marker = if row == self.state.cursor { ">" } else { " " };
                let checked = if self.state.selected[project_idx] {
                    "[x]"
                } else {
                    "[ ]"
                };
                let mut tags = Vec::new();
                if project.in_use {
                    tags.push("IN_USE");
                }
                if project.protected {
                    tags.push("PROTECTED");
                }
                if project.recent {
                    tags.push("RECENT");
                }
                if self.state.block_reason(project).is_some() {
                    tags.push("BLOCKED");
                }
                let tags_suffix = if tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", tags.join(","))
                };

                let left = format!(
                    "{} {} {:>8} source:{:<9} ",
                    marker,
                    checked,
                    format_size(project.size),
                    detection_source(project),
                );
                let path = truncate_middle(
                    &project.cleanable_dir.display().to_string(),
                    cols.saturating_sub(left.len() + tags_suffix.len()),
                );

                write!(out, "{}{}{}\r\n", left, path, tags_suffix)?;
            }
        }

        let printed_rows = end.saturating_sub(start);
        for _ in printed_rows..page_rows {
            write!(out, "\r\n")?;
        }

        write!(out, "\r\n")?;

        if let Some(current) = self.state.current_project() {
            let blocked = self
                .state
                .block_reason(current)
                .map(|r| format!(" | blocked: {}", r))
                .unwrap_or_default();
            let details = format!(
                "Current: {} ({}, source: {}, {} days old{})",
                current.cleanable_dir.display(),
                current.project_type_display_name(),
                detection_source(current),
                current.days_since_modified(),
                blocked
            );
            write!(out, "{}\r\n", truncate_middle(&details, cols))?;
        } else {
            write!(out, "Current: -\r\n")?;
        }

        for line in footer {
            write!(out, "{}\r\n", truncate_middle(&line, cols))?;
        }

        out.flush()?;
        Ok(())
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        page_rows: usize,
    ) -> Result<Option<Option<Vec<ProjectInfo>>>> {
        if key.code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.state.clear_query();
            return Ok(None);
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                return Ok(Some(None));
            }
            KeyCode::Enter => {
                if !self.state.has_any_selected() {
                    self.state.smart_select_current();
                }
                let selected = self.state.selected_projects();
                return Ok(Some(Some(selected)));
            }
            KeyCode::Up | KeyCode::Char('k') => self.state.move_up(page_rows),
            KeyCode::Down | KeyCode::Char('j') => self.state.move_down(page_rows),
            KeyCode::Char(' ') => self.state.toggle_current(),
            KeyCode::Char('a') if !self.state.search_mode => self.state.select_all_visible(),
            KeyCode::Char('d') if !self.state.search_mode => self.state.deselect_all_visible(),
            KeyCode::Char('s') if !self.state.search_mode => self.state.cycle_sort_key(),
            KeyCode::Char('o') if !self.state.search_mode => self.state.toggle_sort_order(),
            KeyCode::Char('/') => self.state.search_mode = true,
            KeyCode::Backspace => self.state.pop_query_char(),
            KeyCode::Char(ch) => {
                if self.state.search_mode && is_search_char(ch) {
                    self.state.push_query_char(ch);
                }
            }
            _ => {}
        }

        Ok(None)
    }
}

fn is_search_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | ' ')
}

fn footer_lines(width: usize, search_mode: bool) -> Vec<String> {
    if search_mode {
        if width >= 80 {
            vec![
                "Search mode: type to filter | Backspace delete | Ctrl+U clear".to_string(),
                "Enter confirm | q/Esc cancel".to_string(),
            ]
        } else {
            vec![
                "Search: type | Backspace | Ctrl+U".to_string(),
                "Enter confirm | q/Esc cancel".to_string(),
            ]
        }
    } else if width >= 130 {
        vec![
            "Up/Down/j/k move | Space toggle | a all | d none | / search".to_string(),
            "s sort(size/age/source) | o order | Enter confirm | q/Esc cancel".to_string(),
        ]
    } else if width >= 90 {
        vec![
            "Up/Down/j/k | Space | a all | d none | / search".to_string(),
            "s sort | o order | Enter confirm | q/Esc cancel".to_string(),
        ]
    } else {
        vec![
            "j/k move | Space | / search".to_string(),
            "Enter confirm | q cancel".to_string(),
        ]
    }
}

fn truncate_middle(input: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= width {
        return input.to_string();
    }

    if width <= 3 {
        return "..."[..width].to_string();
    }

    let left = (width - 3) / 2;
    let right = width - 3 - left;
    let start: String = chars.iter().take(left).collect();
    let end: String = chars
        .iter()
        .rev()
        .take(right)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!("{}...{}", start, end)
}

#[derive(Debug)]
struct SelectorState {
    projects: Vec<ProjectInfo>,
    visible_indices: Vec<usize>,
    selected: Vec<bool>,
    query: String,
    search_mode: bool,
    sort_key: SortKey,
    reverse: bool,
    cursor: usize,
    top: usize,
    options: SelectorOptions,
}

impl SelectorState {
    fn new(projects: Vec<ProjectInfo>, options: SelectorOptions) -> Self {
        let mut state = Self {
            selected: vec![false; projects.len()],
            projects,
            visible_indices: Vec::new(),
            query: String::new(),
            search_mode: false,
            sort_key: SortKey::Size,
            reverse: false,
            cursor: 0,
            top: 0,
            options,
        };
        state.recompute_visible();
        state.select_default_safe_targets();
        state
    }

    fn search_status(&self) -> String {
        if self.query.is_empty() {
            if self.search_mode {
                "/_".to_string()
            } else {
                "off".to_string()
            }
        } else if self.search_mode {
            format!("/{}_", self.query)
        } else {
            format!("/{}", self.query)
        }
    }

    fn page_rows(&self, rows: usize) -> usize {
        let footer_rows = if self.search_mode { 2 } else { 2 };
        rows.saturating_sub(8 + footer_rows).max(1)
    }

    fn selected_count(&self) -> usize {
        self.selected.iter().filter(|&&v| v).count()
    }

    fn selected_size(&self) -> u64 {
        self.projects
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.selected[*idx])
            .map(|(_, p)| p.size)
            .sum()
    }

    fn current_project_index(&self) -> Option<usize> {
        self.visible_indices.get(self.cursor).copied()
    }

    fn current_project(&self) -> Option<&ProjectInfo> {
        self.current_project_index()
            .and_then(|idx| self.projects.get(idx))
    }

    fn block_reason<'a>(&self, project: &'a ProjectInfo) -> Option<&'static str> {
        if project.in_use && !self.options.force {
            Some("IN_USE")
        } else if project.protected && !self.options.force_protected {
            Some("PROTECTED")
        } else {
            None
        }
    }

    fn is_selectable(&self, idx: usize) -> bool {
        self.projects
            .get(idx)
            .map(|p| self.block_reason(p).is_none())
            .unwrap_or(false)
    }

    fn select_default_safe_targets(&mut self) {
        for idx in &self.visible_indices {
            let project = &self.projects[*idx];
            self.selected[*idx] = self.is_selectable(*idx) && !project.recent;
        }
    }

    fn recompute_visible(&mut self) {
        let query = self.query.to_ascii_lowercase();

        self.visible_indices.clear();
        for (idx, project) in self.projects.iter().enumerate() {
            if query.is_empty() {
                self.visible_indices.push(idx);
                continue;
            }

            let path = project
                .cleanable_dir
                .display()
                .to_string()
                .to_ascii_lowercase();
            let typ = project.project_type_display_name().to_ascii_lowercase();
            if path.contains(&query) || typ.contains(&query) {
                self.visible_indices.push(idx);
            }
        }

        let sort_key = self.sort_key;
        let reverse = self.reverse;
        let projects = &self.projects;
        self.visible_indices
            .sort_by(|a, b| compare_projects_for_sort(projects, sort_key, reverse, *a, *b));

        if self.visible_indices.is_empty() {
            self.cursor = 0;
            self.top = 0;
            return;
        }

        if self.cursor >= self.visible_indices.len() {
            self.cursor = self.visible_indices.len() - 1;
        }
        if self.top > self.cursor {
            self.top = self.cursor;
        }
    }

    fn ensure_cursor_visible(&mut self, page_rows: usize) {
        if self.visible_indices.is_empty() {
            self.cursor = 0;
            self.top = 0;
            return;
        }

        if self.cursor >= self.visible_indices.len() {
            self.cursor = self.visible_indices.len() - 1;
        }

        if self.cursor < self.top {
            self.top = self.cursor;
        }

        if self.cursor >= self.top + page_rows {
            self.top = self.cursor + 1 - page_rows;
        }

        let max_top = self.visible_indices.len().saturating_sub(page_rows);
        if self.top > max_top {
            self.top = max_top;
        }
    }

    fn move_up(&mut self, page_rows: usize) {
        if self.visible_indices.is_empty() {
            return;
        }

        if self.cursor > 0 {
            self.cursor -= 1;
        }
        self.ensure_cursor_visible(page_rows);
    }

    fn move_down(&mut self, page_rows: usize) {
        if self.visible_indices.is_empty() {
            return;
        }

        if self.cursor + 1 < self.visible_indices.len() {
            self.cursor += 1;
        }
        self.ensure_cursor_visible(page_rows);
    }

    fn toggle_current(&mut self) {
        if let Some(idx) = self.current_project_index() {
            if self.is_selectable(idx) {
                self.selected[idx] = !self.selected[idx];
            }
        }
    }

    fn select_all_visible(&mut self) {
        for idx in &self.visible_indices {
            if self.is_selectable(*idx) {
                self.selected[*idx] = true;
            }
        }
    }

    fn deselect_all_visible(&mut self) {
        for idx in &self.visible_indices {
            self.selected[*idx] = false;
        }
    }

    fn cycle_sort_key(&mut self) {
        self.sort_key = self.sort_key.next();
        self.recompute_visible();
    }

    fn toggle_sort_order(&mut self) {
        self.reverse = !self.reverse;
        self.recompute_visible();
    }

    fn push_query_char(&mut self, ch: char) {
        self.query.push(ch);
        self.recompute_visible();
    }

    fn pop_query_char(&mut self) {
        self.query.pop();
        if self.query.is_empty() {
            self.search_mode = false;
        }
        self.recompute_visible();
    }

    fn clear_query(&mut self) {
        self.query.clear();
        self.search_mode = false;
        self.recompute_visible();
    }

    fn has_any_selected(&self) -> bool {
        self.selected.iter().any(|v| *v)
    }

    fn smart_select_current(&mut self) {
        if let Some(idx) = self.current_project_index() {
            if self.is_selectable(idx) {
                self.selected[idx] = true;
            }
        }
    }

    fn selected_projects(&self) -> Vec<ProjectInfo> {
        self.projects
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.selected[*idx])
            .map(|(_, project)| project.clone())
            .collect()
    }
}

fn compare_projects_for_sort(
    projects: &[ProjectInfo],
    sort_key: SortKey,
    reverse: bool,
    a: usize,
    b: usize,
) -> Ordering {
    let pa = &projects[a];
    let pb = &projects[b];

    let primary = match sort_key {
        SortKey::Size => pb
            .size
            .cmp(&pa.size)
            .then_with(|| pb.days_since_modified().cmp(&pa.days_since_modified())),
        SortKey::Age => pb
            .days_since_modified()
            .cmp(&pa.days_since_modified())
            .then_with(|| pb.size.cmp(&pa.size)),
        SortKey::Source => source_rank(pa)
            .cmp(&source_rank(pb))
            .then_with(|| pb.size.cmp(&pa.size)),
    };

    if reverse {
        primary.reverse()
    } else {
        primary
    }
}

fn detection_source(project: &ProjectInfo) -> &'static str {
    match project.matched_rule.as_ref().map(|rule| rule.source) {
        Some(RuleSource::Custom) => "custom",
        Some(RuleSource::Builtin) => "builtin",
        Some(RuleSource::Gitignore) => "gitignore",
        Some(RuleSource::Heuristic) => "heuristic",
        None => "unknown",
    }
}

fn source_rank(project: &ProjectInfo) -> u8 {
    match project.matched_rule.as_ref().map(|rule| rule.source) {
        Some(RuleSource::Custom) => 0,
        Some(RuleSource::Builtin) => 1,
        Some(RuleSource::Heuristic) => 2,
        Some(RuleSource::Gitignore) => 3,
        None => 4,
    }
}

struct TerminalSession {
    active: bool,
}

impl TerminalSession {
    fn start() -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, Hide)?;
        Ok(Self { active: true })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        let _ = disable_raw_mode();
        let mut out = stdout();
        let _ = execute!(
            out,
            Show,
            LeaveAlternateScreen,
            Clear(ClearType::All),
            MoveTo(0, 0)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use dev_cleaner_core::scanner::{Category, Confidence, ProjectType, RiskLevel};
    use std::path::PathBuf;

    fn project(
        path: &str,
        size: u64,
        days_old: i64,
        risk: RiskLevel,
        in_use: bool,
        protected: bool,
        recent: bool,
    ) -> ProjectInfo {
        ProjectInfo {
            root: PathBuf::from("/workspace/p"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: risk,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from(path),
            size,
            size_calculated: true,
            last_modified: Utc::now() - Duration::days(days_old),
            in_use,
            protected,
            protected_by: None,
            recent,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn default_selection_uses_safe_targets_only() {
        let projects = vec![
            project("/a/safe", 300, 30, RiskLevel::Low, false, false, false),
            project("/a/recent", 200, 2, RiskLevel::Medium, false, false, true),
            project("/a/protected", 100, 60, RiskLevel::High, false, true, false),
            project("/a/in_use", 80, 90, RiskLevel::Low, true, false, false),
        ];

        let state = SelectorState::new(projects, SelectorOptions::default());

        let safe_idx = state
            .projects
            .iter()
            .position(|p| p.cleanable_dir.ends_with("safe"))
            .unwrap();
        let recent_idx = state
            .projects
            .iter()
            .position(|p| p.cleanable_dir.ends_with("recent"))
            .unwrap();
        let protected_idx = state
            .projects
            .iter()
            .position(|p| p.cleanable_dir.ends_with("protected"))
            .unwrap();
        let in_use_idx = state
            .projects
            .iter()
            .position(|p| p.cleanable_dir.ends_with("in_use"))
            .unwrap();

        assert!(state.selected[safe_idx]);
        assert!(!state.selected[recent_idx]);
        assert!(!state.selected[protected_idx]);
        assert!(!state.selected[in_use_idx]);
    }

    #[test]
    fn blocked_target_cannot_be_toggled() {
        let projects = vec![project(
            "/a/protected",
            123,
            20,
            RiskLevel::Medium,
            false,
            true,
            false,
        )];
        let mut state = SelectorState::new(projects, SelectorOptions::default());
        assert!(!state.is_selectable(0));

        state.toggle_current();
        assert!(!state.selected[0]);
    }

    #[test]
    fn query_and_sort_reverse_work() {
        let mut state = SelectorState::new(
            vec![
                project("/a/one", 100, 10, RiskLevel::Low, false, false, false),
                project("/a/two", 300, 20, RiskLevel::High, false, false, false),
                project("/a/three", 200, 30, RiskLevel::Medium, false, false, false),
            ],
            SelectorOptions::default(),
        );

        state.search_mode = true;
        state.push_query_char('t');
        state.push_query_char('w');
        state.push_query_char('o');

        assert_eq!(state.visible_indices.len(), 1);
        let visible_path = state.projects[state.visible_indices[0]]
            .cleanable_dir
            .display()
            .to_string();
        assert!(visible_path.ends_with("two"));

        state.clear_query();
        state.sort_key = SortKey::Age;
        state.recompute_visible();
        let first_age_sorted = &state.projects[state.visible_indices[0]].cleanable_dir;
        assert!(first_age_sorted.ends_with("three"));

        state.toggle_sort_order();
        let first_age_sorted_reversed = &state.projects[state.visible_indices[0]].cleanable_dir;
        assert!(first_age_sorted_reversed.ends_with("one"));
    }

    #[test]
    fn smart_select_current_selects_only_selectable_target() {
        let mut state = SelectorState::new(
            vec![
                project("/a/blocked", 500, 50, RiskLevel::High, true, false, false),
                project("/a/safe", 100, 30, RiskLevel::Low, false, false, false),
            ],
            SelectorOptions::default(),
        );

        state.deselect_all_visible();
        assert!(!state.has_any_selected());

        let safe_cursor = state
            .visible_indices
            .iter()
            .position(|idx| state.projects[*idx].cleanable_dir.ends_with("safe"))
            .unwrap();
        state.cursor = safe_cursor;

        state.smart_select_current();
        assert!(state.has_any_selected());

        state.deselect_all_visible();
        let blocked_cursor = state
            .visible_indices
            .iter()
            .position(|idx| state.projects[*idx].cleanable_dir.ends_with("blocked"))
            .unwrap();
        state.cursor = blocked_cursor;
        state.smart_select_current();
        assert!(!state.has_any_selected());
    }
}
