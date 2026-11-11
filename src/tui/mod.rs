use crate::clean_progress::{AuditedCleanObserver, TerminalCleanObserver};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dev_cleaner_core::audit::AuditLogger;
use dev_cleaner_core::evaluation::EvaluatedProject;
use dev_cleaner_core::utils::format_size;
use dev_cleaner_core::{CleanResult, Cleaner, Config, ProjectInfo};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;

#[derive(Clone, Copy)]
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

struct AppState {
    projects: Vec<EvaluatedProject>,
    visible_indices: Vec<usize>,
    selected: Vec<bool>,
    list_state: ListState,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    query: String,
    sort_key: SortKey,
    show_help: bool,
    input_mode: InputMode,
    filter_cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppOutcome {
    Continue,
    Quit,
    CleanSelected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Normal,
    Search,
    FilterPanel,
}

impl AppState {
    fn new(
        projects: Vec<ProjectInfo>,
        include_recent: bool,
        include_protected: bool,
        recent_days: i64,
    ) -> Self {
        let projects = projects
            .into_iter()
            .map(|project| {
                let recent = project.days_since_modified() < recent_days;
                let mut evaluated = EvaluatedProject::from(project);
                evaluated.safety.recent = recent;
                evaluated
            })
            .collect::<Vec<_>>();
        let mut app = Self {
            selected: vec![false; projects.len()],
            projects,
            visible_indices: Vec::new(),
            list_state: ListState::default(),
            include_recent,
            include_protected,
            recent_days,
            query: String::new(),
            sort_key: SortKey::Size,
            show_help: false,
            input_mode: InputMode::Normal,
            filter_cursor: 0,
        };
        app.recompute_visible();
        for &idx in &app.visible_indices {
            app.selected[idx] = default_selectable(&app.projects[idx], app.recent_days);
        }
        if !app.visible_indices.is_empty() {
            app.list_state.select(Some(0));
        }
        app
    }

    fn recompute_visible(&mut self) {
        self.visible_indices.clear();
        for (idx, p) in self.projects.iter().enumerate() {
            if !self.include_protected && p.safety.protected {
                continue;
            }
            if !self.include_recent && p.safety.recent {
                continue;
            }
            if !self.query.is_empty() {
                let q = self.query.to_ascii_lowercase();
                let path = p
                    .info
                    .cleanable_dir
                    .display()
                    .to_string()
                    .to_ascii_lowercase();
                let name = p.info.project_type_display_name().to_ascii_lowercase();
                if !path.contains(&q) && !name.contains(&q) {
                    continue;
                }
            }
            self.visible_indices.push(idx);
        }

        self.visible_indices.sort_by(|a, b| match self.sort_key {
            SortKey::Size => self.projects[*b]
                .info
                .size
                .cmp(&self.projects[*a].info.size)
                .then_with(|| {
                    self.projects[*b]
                        .info
                        .days_since_modified()
                        .cmp(&self.projects[*a].info.days_since_modified())
                }),
            SortKey::Age => self.projects[*b]
                .info
                .days_since_modified()
                .cmp(&self.projects[*a].info.days_since_modified())
                .then_with(|| {
                    self.projects[*b]
                        .info
                        .size
                        .cmp(&self.projects[*a].info.size)
                }),
            SortKey::Source => source_sort_rank(&self.projects[*a])
                .cmp(&source_sort_rank(&self.projects[*b]))
                .then_with(|| {
                    self.projects[*b]
                        .info
                        .size
                        .cmp(&self.projects[*a].info.size)
                }),
        });

        let current = self.list_state.selected().unwrap_or(0);
        if self.visible_indices.is_empty() {
            self.list_state.select(None);
        } else if current >= self.visible_indices.len() {
            self.list_state.select(Some(self.visible_indices.len() - 1));
        } else {
            self.list_state.select(Some(current));
        }
    }

    /// Selected rows that are currently visible. Enter acts on these only: rows hidden by a
    /// search or filter stay selected for later but are never cleaned unseen.
    fn effective_selection(&self) -> impl Iterator<Item = &EvaluatedProject> {
        self.visible_indices
            .iter()
            .filter(|idx| self.selected[**idx])
            .map(|idx| &self.projects[*idx])
    }

    fn selected_count(&self) -> usize {
        self.effective_selection().count()
    }

    fn selected_size(&self) -> u64 {
        self.effective_selection().map(|p| p.info.size).sum()
    }

    fn visible_total_size(&self) -> u64 {
        self.visible_indices
            .iter()
            .map(|idx| self.projects[*idx].info.size)
            .sum()
    }

    fn get_selected_projects(&self) -> Vec<ProjectInfo> {
        self.effective_selection()
            .map(|p| p.to_project_info())
            .collect()
    }

    fn selected_project(&self) -> Option<&EvaluatedProject> {
        let idx = self.list_state.selected()?;
        let project_idx = *self.visible_indices.get(idx)?;
        self.projects.get(project_idx)
    }

    fn next(&mut self) {
        let Some(current) = self.list_state.selected() else {
            self.list_state.select(Some(0));
            return;
        };
        if self.visible_indices.is_empty() {
            self.list_state.select(None);
            return;
        }
        let next = if current + 1 >= self.visible_indices.len() {
            0
        } else {
            current + 1
        };
        self.list_state.select(Some(next));
    }

    fn previous(&mut self) {
        let Some(current) = self.list_state.selected() else {
            self.list_state.select(Some(0));
            return;
        };
        if self.visible_indices.is_empty() {
            self.list_state.select(None);
            return;
        }
        let prev = if current == 0 {
            self.visible_indices.len() - 1
        } else {
            current - 1
        };
        self.list_state.select(Some(prev));
    }

    fn toggle_current_selection(&mut self) {
        if let Some(cursor) = self.list_state.selected() {
            if let Some(&idx) = self.visible_indices.get(cursor) {
                // Protected and in-use rows can be shown but never cleaned from the TUI.
                let project = &self.projects[idx];
                if project.safety.protected || project.info.in_use {
                    return;
                }
                self.selected[idx] = !self.selected[idx];
            }
        }
    }

    fn select_all_visible(&mut self) {
        for idx in &self.visible_indices {
            self.selected[*idx] = default_selectable(&self.projects[*idx], self.recent_days);
        }
    }

    fn deselect_all_visible(&mut self) {
        for idx in &self.visible_indices {
            self.selected[*idx] = false;
        }
    }

    fn cycle_sort_back(&mut self) {
        self.sort_key = match self.sort_key {
            SortKey::Size => SortKey::Source,
            SortKey::Age => SortKey::Size,
            SortKey::Source => SortKey::Age,
        };
        self.recompute_visible();
    }

    fn filter_cursor_next(&mut self) {
        self.filter_cursor = (self.filter_cursor + 1) % 3;
    }

    fn filter_cursor_prev(&mut self) {
        self.filter_cursor = if self.filter_cursor == 0 {
            2
        } else {
            self.filter_cursor - 1
        };
    }

    fn update_filter_value(&mut self, forward: bool) {
        match self.filter_cursor {
            0 => {
                if forward {
                    self.sort_key = self.sort_key.next();
                    self.recompute_visible();
                } else {
                    self.cycle_sort_back();
                }
            }
            1 => {
                self.include_recent = !self.include_recent;
                self.recompute_visible();
            }
            2 => {
                self.include_protected = !self.include_protected;
                self.recompute_visible();
            }
            _ => {}
        }
    }
}

fn handle_key(app: &mut AppState, key: KeyCode) -> AppOutcome {
    if app.show_help {
        app.show_help = false;
        return AppOutcome::Continue;
    }

    if app.input_mode == InputMode::Search {
        match key {
            KeyCode::Esc | KeyCode::Enter => {
                app.input_mode = InputMode::Normal;
                AppOutcome::Continue
            }
            KeyCode::Backspace => {
                app.query.pop();
                app.recompute_visible();
                AppOutcome::Continue
            }
            KeyCode::Char(ch)
                if ch.is_ascii_alphanumeric()
                    || ch == '/'
                    || ch == '.'
                    || ch == '-'
                    || ch == '_' =>
            {
                app.query.push(ch);
                app.recompute_visible();
                AppOutcome::Continue
            }
            _ => AppOutcome::Continue,
        }
    } else if app.input_mode == InputMode::FilterPanel {
        match key {
            KeyCode::Esc | KeyCode::Char('f') => {
                app.input_mode = InputMode::Normal;
                AppOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.filter_cursor_next();
                AppOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.filter_cursor_prev();
                AppOutcome::Continue
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                app.update_filter_value(true);
                AppOutcome::Continue
            }
            KeyCode::Left | KeyCode::Char('h') => {
                app.update_filter_value(false);
                AppOutcome::Continue
            }
            _ => AppOutcome::Continue,
        }
    } else {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => AppOutcome::Quit,
            KeyCode::Down | KeyCode::Char('j') => {
                app.next();
                AppOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.previous();
                AppOutcome::Continue
            }
            KeyCode::Char(' ') => {
                app.toggle_current_selection();
                AppOutcome::Continue
            }
            KeyCode::Char('a') => {
                app.select_all_visible();
                AppOutcome::Continue
            }
            KeyCode::Char('d') => {
                app.deselect_all_visible();
                AppOutcome::Continue
            }
            KeyCode::Char('s') => {
                app.sort_key = app.sort_key.next();
                app.recompute_visible();
                AppOutcome::Continue
            }
            KeyCode::Char('R') => {
                app.include_recent = !app.include_recent;
                app.recompute_visible();
                AppOutcome::Continue
            }
            KeyCode::Char('P') => {
                app.include_protected = !app.include_protected;
                app.recompute_visible();
                AppOutcome::Continue
            }
            KeyCode::Backspace => {
                app.query.pop();
                app.recompute_visible();
                AppOutcome::Continue
            }
            KeyCode::Char('/') => {
                app.input_mode = InputMode::Search;
                AppOutcome::Continue
            }
            KeyCode::Char('f') => {
                app.input_mode = InputMode::FilterPanel;
                AppOutcome::Continue
            }
            KeyCode::Char('?') | KeyCode::Char('h') => {
                app.show_help = true;
                AppOutcome::Continue
            }
            KeyCode::Enter => {
                if app.get_selected_projects().is_empty() {
                    AppOutcome::Continue
                } else {
                    AppOutcome::CleanSelected
                }
            }
            KeyCode::Char(ch)
                if ch.is_ascii_alphanumeric()
                    || ch == '/'
                    || ch == '.'
                    || ch == '-'
                    || ch == '_' =>
            {
                app.query.push(ch);
                app.recompute_visible();
                AppOutcome::Continue
            }
            _ => AppOutcome::Continue,
        }
    }
}

fn default_selectable(project: &EvaluatedProject, recent_days: i64) -> bool {
    !project.info.in_use
        && !project.safety.protected
        && project.info.days_since_modified() >= recent_days
}

fn selection_status(project: &EvaluatedProject) -> &'static str {
    if project.info.in_use {
        "in-use"
    } else if project.safety.protected {
        "protected"
    } else if project.safety.recent {
        "recent"
    } else {
        "ready"
    }
}

fn detection_source(project: &EvaluatedProject) -> &'static str {
    let Some(rule) = &project.info.matched_rule else {
        return "unknown";
    };

    match rule.source {
        dev_cleaner_core::scanner::RuleSource::Builtin => "builtin",
        dev_cleaner_core::scanner::RuleSource::Custom => "custom",
        dev_cleaner_core::scanner::RuleSource::Gitignore => "gitignore",
        dev_cleaner_core::scanner::RuleSource::Heuristic => "heuristic",
    }
}

fn source_sort_rank(project: &EvaluatedProject) -> u8 {
    match project.info.matched_rule.as_ref().map(|r| r.source) {
        Some(dev_cleaner_core::scanner::RuleSource::Custom) => 0,
        Some(dev_cleaner_core::scanner::RuleSource::Builtin) => 1,
        Some(dev_cleaner_core::scanner::RuleSource::Heuristic) => 2,
        Some(dev_cleaner_core::scanner::RuleSource::Gitignore) => 3,
        None => 4,
    }
}

fn decision_summary(project: &EvaluatedProject, recent_days: i64) -> &'static str {
    if default_selectable(project, recent_days) {
        "Eligible to clean now"
    } else {
        "Will be skipped by default"
    }
}

fn block_reason(project: &EvaluatedProject, recent_days: i64) -> String {
    if project.info.in_use {
        return "Project appears active (lock file modified recently).".to_string();
    }
    if project.safety.protected {
        return format!(
            "Target is protected by policy {}.",
            project
                .safety
                .protected_by
                .clone()
                .unwrap_or_else(|| "(rule)".to_string())
        );
    }
    if project.safety.recent {
        return format!(
            "Target was modified within the recent window ({} days).",
            recent_days
        );
    }
    "No blocker. Safe to include in cleanup selection.".to_string()
}

pub fn run_tui_projects(
    projects: Vec<ProjectInfo>,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    config: &Config,
) -> Result<()> {
    if projects.is_empty() {
        println!("No cleanable directories found.");
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = AppState::new(projects, include_recent, include_protected, recent_days);
    let res = run_app(&mut terminal, app, &AuditLogger::from_config(config));

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Print after leaving the alternate screen, or the summary (and undo batch id) is erased.
    if let Some(result) = res? {
        println!("\nCleaning completed!");
        println!("  Cleaned: {}", result.cleaned_count);
        println!(
            "  Skipped: {} ({})",
            result.skipped_count,
            format_size(result.bytes_skipped)
        );
        println!("  Failed: {}", result.failed_count);
        println!("  Moved to trash: {}", result.size_freed_human());
        if let Some(batch_id) = &result.trash_batch_id {
            println!("  Undo: dev-cleaner undo --batch {batch_id}");
            println!("  Release space: dev-cleaner trash purge --batch {batch_id}");
        }
    }
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: AppState,
    audit: &AuditLogger,
) -> Result<Option<CleanResult>> {
    loop {
        terminal.draw(|f| render_ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match handle_key(&mut app, key.code) {
                AppOutcome::Continue => {}
                AppOutcome::Quit => return Ok(None),
                AppOutcome::CleanSelected => {
                    disable_raw_mode()?;
                    // Enter cleans without a separate prompt, so it must stay undoable.
                    let selected = app.get_selected_projects();
                    let run_id = audit.start_run_with_mode("clean", Some("trash")).ok();
                    // Ticking a recent row (shown via R / --include-recent) is the consent to clean it.
                    let cleaner = Cleaner::new().trash(true).include_recent(true);
                    let mut observer = AuditedCleanObserver {
                        terminal: TerminalCleanObserver::new(true),
                        audit,
                        run_id: run_id.clone(),
                        command: "clean",
                        action: "trash",
                    };
                    let result = cleaner.clean_multiple_with_observer(&selected, &mut observer)?;
                    if let Some(run_id) = &run_id {
                        let _ = audit.finish_run(
                            run_id,
                            "clean",
                            result.cleaned_count,
                            result.skipped_count,
                            result.failed_count,
                            result.bytes_freed,
                        );
                    }
                    return Ok(Some(result));
                }
            }
        }
    }
}

fn render_ui(f: &mut Frame, app: &mut AppState) {
    if app.show_help {
        draw_help(f);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(6),
        ])
        .split(f.size());

    draw_header(f, chunks[0], app);
    draw_body(f, chunks[1], app);
    draw_footer(f, chunks[2], app);

    if app.input_mode == InputMode::FilterPanel {
        draw_filter_panel(f, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &AppState) {
    let mode = match app.input_mode {
        InputMode::Normal => "browse",
        InputMode::Search => "search",
        InputMode::FilterPanel => "filters",
    };

    let text = vec![
        Line::from(Span::styled(
            "Dev Cleaner - Select, Review, Confirm",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("1) Select targets  2) Review right-side explanation  3) Press Enter to clean"),
        Line::from(format!(
            "Visible: {} ({}) | Selected: {} ({})",
            app.visible_indices.len(),
            format_size(app.visible_total_size()),
            app.selected_count(),
            format_size(app.selected_size())
        )),
        Line::from(format!(
            "Controls: sort={} include_recent={} include_protected={}",
            app.sort_key.as_str(),
            app.include_recent,
            app.include_protected
        )),
        Line::from(format!(
            "Search: `{}` | Mode: {}",
            if app.query.is_empty() {
                "<empty>"
            } else {
                &app.query
            },
            mode
        )),
    ];

    let paragraph =
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Info"));
    f.render_widget(paragraph, area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &mut AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);
    draw_project_list(f, cols[0], app);
    draw_detail_panel(f, cols[1], app);
}

fn draw_project_list(f: &mut Frame, area: Rect, app: &mut AppState) {
    let items: Vec<ListItem> = app
        .visible_indices
        .iter()
        .map(|idx| {
            let p = &app.projects[*idx];
            let selected_marker = if app.selected[*idx] { "[✓]" } else { "[ ]" };
            let line = format!(
                "{} {:>8} {:<10} {:<10} {:<8} {}",
                selected_marker,
                format_size(p.info.size),
                detection_source(p),
                selection_status(p),
                p.info.project_type_display_name(),
                p.info.cleanable_dir.display(),
            );
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Targets  [x] size source state type path"),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_detail_panel(f: &mut Frame, area: Rect, app: &AppState) {
    let text = if let Some(p) = app.selected_project() {
        vec![
            Line::from(Span::styled(
                p.info.cleanable_dir.display().to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "Decision: {}",
                decision_summary(p, app.recent_days),
            )),
            Line::from(format!("Reason: {}", block_reason(p, app.recent_days),)),
            Line::from(""),
            Line::from(format!("Type: {}", p.info.project_type_display_name())),
            Line::from(format!("Size: {}", format_size(p.info.size))),
            Line::from(format!("Age: {} days", p.info.days_since_modified())),
            Line::from(format!("Source: {}", detection_source(p))),
            Line::from(format!(
                "Protected by: {}",
                p.safety
                    .protected_by
                    .clone()
                    .unwrap_or_else(|| "-".to_string())
            )),
        ]
    } else {
        vec![Line::from("No visible targets")]
    };

    let panel = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Details"));
    f.render_widget(panel, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &AppState) {
    let shortcut_line = match app.input_mode {
        InputMode::Normal => {
            "j/k move | space toggle | enter move to trash | / search | f filters | ? help | q quit"
        }
        InputMode::Search => {
            "Search mode: type to filter list, Backspace delete, Enter/Esc to exit"
        }
        InputMode::FilterPanel => {
            "Filter mode: j/k choose field, h/l change value, Enter apply, Esc close"
        }
    };

    let help = vec![
        Line::from(shortcut_line),
        Line::from(format!(
            "Selected: {} ({})",
            app.selected_count(),
            format_size(app.selected_size())
        )),
        Line::from("Tip: review source + status before cleaning."),
    ];
    let footer =
        Paragraph::new(help).block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(footer, area);
}

fn draw_filter_panel(f: &mut Frame, app: &AppState) {
    let area = centered_rect(62, 13, f.size());
    f.render_widget(Clear, area);

    let fields = [
        format!("Sort: {}", app.sort_key.as_str()),
        format!("Include recent: {}", app.include_recent),
        format!("Include protected: {}", app.include_protected),
    ];

    let mut lines = vec![
        Line::from(Span::styled(
            "Filter Panel",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, field) in fields.iter().enumerate() {
        if i == app.filter_cursor {
            lines.push(Line::from(Span::styled(
                format!("> {}", field),
                Style::default().add_modifier(Modifier::BOLD),
            )));
        } else {
            lines.push(Line::from(format!("  {}", field)));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(
        "Use j/k to move, h/l or Enter to change, Esc to close.",
    ));

    let panel =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Filters"));
    f.render_widget(panel, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_help(f: &mut Frame) {
    let help_text = vec![
        Line::from("Help - Dev Cleaner TUI"),
        Line::from(""),
        Line::from("Navigation: ↑/↓/j/k"),
        Line::from("Selection: space toggle, a select all visible, d deselect visible"),
        Line::from("Search: / enters search mode, type to filter, Enter/Esc exits search"),
        Line::from("Filters: f opens panel (j/k choose, h/l change), or s/R/P quick keys"),
        Line::from("Sort: size, age, source"),
        Line::from("Actions: Enter moves selected to Dev Cleaner trash (undo: dev-cleaner undo), q/Esc quit"),
        Line::from(""),
        Line::from("Press any key to close"),
    ];

    let paragraph =
        Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(paragraph, f.size());
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use dev_cleaner_core::scanner::{Category, RiskLevel};
    use std::path::PathBuf;

    fn project(
        name: &str,
        size: u64,
        days_since_modified: i64,
        category: Category,
        risk_level: RiskLevel,
        in_use: bool,
        protected: bool,
    ) -> ProjectInfo {
        ProjectInfo {
            root: PathBuf::from("/repo"),
            project_type: dev_cleaner_core::scanner::ProjectType::Rust,
            project_name: Some(name.to_string()),
            category,
            risk_level,
            confidence: dev_cleaner_core::scanner::Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from(format!("/repo/{name}")),
            size,
            size_calculated: true,
            last_modified: chrono::Utc::now() - Duration::days(days_since_modified),
            in_use,
            protected,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn enter_cleans_only_visible_selection_and_never_protected_rows() {
        let mut app = AppState::new(
            vec![
                project(
                    "alpha",
                    10,
                    30,
                    Category::Cache,
                    RiskLevel::Low,
                    false,
                    false,
                ),
                project(
                    "beta",
                    20,
                    30,
                    Category::Cache,
                    RiskLevel::Low,
                    false,
                    false,
                ),
                project("kept", 30, 30, Category::Cache, RiskLevel::Low, false, true),
            ],
            false,
            true,
            7,
        );
        assert_eq!(app.selected_count(), 2);

        // A search hides "beta": it stays selected but Enter must not clean it.
        app.query = "alpha".to_string();
        app.recompute_visible();
        let names = |app: &AppState| {
            app.get_selected_projects()
                .into_iter()
                .filter_map(|p| p.project_name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&app), vec!["alpha"]);
        assert_eq!(app.selected_count(), 1);

        // The protected row is visible (include_protected) but cannot be ticked.
        app.query.clear();
        app.recompute_visible();
        let kept_row = app
            .visible_indices
            .iter()
            .position(|&idx| app.projects[idx].info.project_name.as_deref() == Some("kept"))
            .unwrap();
        app.list_state.select(Some(kept_row));
        app.toggle_current_selection();
        assert!(!names(&app).contains(&"kept".to_string()));
    }

    #[test]
    fn app_state_handles_filters_selection_and_navigation() {
        let mut app = AppState::new(
            vec![
                project(
                    "cache",
                    10,
                    20,
                    Category::Cache,
                    RiskLevel::Low,
                    false,
                    false,
                ),
                project(
                    "build",
                    30,
                    2,
                    Category::Build,
                    RiskLevel::Medium,
                    false,
                    false,
                ),
                project("deps", 20, 40, Category::Deps, RiskLevel::High, true, true),
            ],
            true,
            true,
            7,
        );

        assert_eq!(app.visible_indices.len(), 3);
        assert_eq!(app.selected_count(), 1);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("build")
        );

        assert_eq!(
            handle_key(&mut app, KeyCode::Char('h')),
            AppOutcome::Continue
        );
        assert!(app.show_help);
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('x')),
            AppOutcome::Continue
        );
        assert!(!app.show_help);

        assert_eq!(
            handle_key(&mut app, KeyCode::Char('b')),
            AppOutcome::Continue
        );
        assert_eq!(app.visible_indices.len(), 1);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("build")
        );

        assert_eq!(
            handle_key(&mut app, KeyCode::Backspace),
            AppOutcome::Continue
        );
        assert!(app.query.is_empty());

        app.recompute_visible();
        app.list_state.select(Some(0));
        assert_eq!(handle_key(&mut app, KeyCode::Down), AppOutcome::Continue);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("deps")
        );
        assert_eq!(handle_key(&mut app, KeyCode::Up), AppOutcome::Continue);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("build")
        );

        assert_eq!(
            handle_key(&mut app, KeyCode::Char('s')),
            AppOutcome::Continue
        );
        assert_eq!(app.sort_key.as_str(), "age");
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('s')),
            AppOutcome::Continue
        );
        assert_eq!(app.sort_key.as_str(), "source");
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('s')),
            AppOutcome::Continue
        );
        assert_eq!(app.sort_key.as_str(), "size");

        assert_eq!(
            handle_key(&mut app, KeyCode::Char('R')),
            AppOutcome::Continue
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('P')),
            AppOutcome::Continue
        );
        assert!(!app.include_recent);
        assert!(!app.include_protected);

        app.include_recent = true;
        app.include_protected = true;
        app.recompute_visible();
        app.list_state.select(Some(0));
        assert_eq!(
            handle_key(&mut app, KeyCode::Char(' ')),
            AppOutcome::Continue
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('a')),
            AppOutcome::Continue
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('d')),
            AppOutcome::Continue
        );
        assert_eq!(app.selected_count(), 0);

        let mut app = AppState::new(
            vec![
                project(
                    "cache",
                    10,
                    20,
                    Category::Cache,
                    RiskLevel::Low,
                    false,
                    false,
                ),
                project(
                    "build",
                    30,
                    2,
                    Category::Build,
                    RiskLevel::Medium,
                    false,
                    false,
                ),
                project("deps", 20, 40, Category::Deps, RiskLevel::High, true, true),
            ],
            true,
            true,
            7,
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('a')),
            AppOutcome::Continue
        );
        assert_eq!(app.selected_count(), 1);
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter),
            AppOutcome::CleanSelected
        );
    }

    #[test]
    fn app_state_handles_empty_and_wraparound_cases() {
        let mut app = AppState::new(Vec::new(), false, false, 7);
        assert!(app.selected_project().is_none());
        assert_eq!(handle_key(&mut app, KeyCode::Esc), AppOutcome::Quit);
        assert_eq!(handle_key(&mut app, KeyCode::Down), AppOutcome::Continue);
        assert_eq!(handle_key(&mut app, KeyCode::Up), AppOutcome::Continue);
        assert_eq!(handle_key(&mut app, KeyCode::Enter), AppOutcome::Continue);

        let projects = vec![
            project(
                "one",
                10,
                10,
                Category::Build,
                RiskLevel::Medium,
                false,
                false,
            ),
            project(
                "two",
                20,
                12,
                Category::Build,
                RiskLevel::Medium,
                false,
                false,
            ),
        ];
        let mut app = AppState::new(projects, true, true, 7);
        app.list_state.select(Some(0));
        assert_eq!(handle_key(&mut app, KeyCode::Up), AppOutcome::Continue);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("one")
        );
        assert_eq!(handle_key(&mut app, KeyCode::Down), AppOutcome::Continue);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("two")
        );
        assert_eq!(handle_key(&mut app, KeyCode::Down), AppOutcome::Continue);
        assert_eq!(
            app.selected_project().unwrap().info.project_name.as_deref(),
            Some("one")
        );
    }

    #[test]
    fn run_tui_projects_returns_early_for_empty_input() {
        assert!(run_tui_projects(Vec::new(), false, false, 7, &Config::default()).is_ok());
    }
}
