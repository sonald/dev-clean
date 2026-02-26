use crate::scanner::{Category, RiskLevel};
use crate::utils::format_size;
use crate::{Cleaner, Config, ProjectInfo, Scanner};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum SortKey {
    Size,
    Age,
    Risk,
}

impl SortKey {
    fn next(self) -> Self {
        match self {
            Self::Size => Self::Age,
            Self::Age => Self::Risk,
            Self::Risk => Self::Size,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Age => "age",
            Self::Risk => "risk",
        }
    }
}

struct AppState {
    projects: Vec<ProjectInfo>,
    visible_indices: Vec<usize>,
    selected: Vec<bool>,
    list_state: ListState,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    query: String,
    category_filter: Option<Category>,
    risk_filter: Option<RiskLevel>,
    sort_key: SortKey,
    show_help: bool,
}

impl AppState {
    fn new(
        mut projects: Vec<ProjectInfo>,
        include_recent: bool,
        include_protected: bool,
        recent_days: i64,
    ) -> Self {
        for project in &mut projects {
            project.recent = project.days_since_modified() < recent_days;
        }
        let mut app = Self {
            selected: vec![false; projects.len()],
            projects,
            visible_indices: Vec::new(),
            list_state: ListState::default(),
            include_recent,
            include_protected,
            recent_days,
            query: String::new(),
            category_filter: None,
            risk_filter: None,
            sort_key: SortKey::Size,
            show_help: false,
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
            if !self.include_protected && p.protected {
                continue;
            }
            if !self.include_recent && p.recent {
                continue;
            }
            if let Some(category) = self.category_filter {
                if p.category != category {
                    continue;
                }
            }
            if let Some(risk) = self.risk_filter {
                if p.risk_level != risk {
                    continue;
                }
            }
            if !self.query.is_empty() {
                let q = self.query.to_ascii_lowercase();
                let path = p.cleanable_dir.display().to_string().to_ascii_lowercase();
                let name = p.project_type_display_name().to_ascii_lowercase();
                if !path.contains(&q) && !name.contains(&q) {
                    continue;
                }
            }
            self.visible_indices.push(idx);
        }

        self.visible_indices.sort_by(|a, b| match self.sort_key {
            SortKey::Size => self.projects[*b]
                .size
                .cmp(&self.projects[*a].size)
                .then_with(|| {
                    self.projects[*b]
                        .days_since_modified()
                        .cmp(&self.projects[*a].days_since_modified())
                }),
            SortKey::Age => self.projects[*b]
                .days_since_modified()
                .cmp(&self.projects[*a].days_since_modified())
                .then_with(|| self.projects[*b].size.cmp(&self.projects[*a].size)),
            SortKey::Risk => self.projects[*a]
                .risk_level
                .cmp(&self.projects[*b].risk_level)
                .then_with(|| self.projects[*b].size.cmp(&self.projects[*a].size)),
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

    fn visible_total_size(&self) -> u64 {
        self.visible_indices
            .iter()
            .map(|idx| self.projects[*idx].size)
            .sum()
    }

    fn get_selected_projects(&self) -> Vec<ProjectInfo> {
        self.projects
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.selected[*idx])
            .map(|(_, p)| p.clone())
            .collect()
    }

    fn selected_project(&self) -> Option<&ProjectInfo> {
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

    fn cycle_category(&mut self) {
        self.category_filter = match self.category_filter {
            None => Some(Category::Cache),
            Some(Category::Cache) => Some(Category::Build),
            Some(Category::Build) => Some(Category::Deps),
            Some(Category::Deps) => None,
            Some(Category::Unknown) => None,
        };
        self.recompute_visible();
    }

    fn cycle_risk(&mut self) {
        self.risk_filter = match self.risk_filter {
            None => Some(RiskLevel::Low),
            Some(RiskLevel::Low) => Some(RiskLevel::Medium),
            Some(RiskLevel::Medium) => Some(RiskLevel::High),
            Some(RiskLevel::High) => None,
        };
        self.recompute_visible();
    }
}

fn default_selectable(project: &ProjectInfo, recent_days: i64) -> bool {
    !project.in_use && !project.protected && project.days_since_modified() >= recent_days
}

pub fn run_tui(path: PathBuf) -> Result<()> {
    let config = Config::load_or_default(Config::default_path())?;
    run_tui_with_config(path, &config)
}

pub fn run_tui_with_config(path: PathBuf, config: &Config) -> Result<()> {
    let mut scanner = Scanner::new(&path)
        .exclude_dirs(&config.exclude_dirs)
        .custom_patterns(&config.custom_patterns);

    if let Some(depth) = config.default_depth {
        scanner = scanner.max_depth(depth);
    }
    if let Some(min_size_mb) = config.min_size_mb {
        scanner = scanner.min_size(min_size_mb * 1024 * 1024);
    }
    if let Some(max_age_days) = config.max_age_days {
        scanner = scanner.max_age_days(max_age_days);
    }

    let projects = scanner.scan()?;
    run_tui_projects(projects, false, false, 7)
}

pub fn run_tui_projects(
    projects: Vec<ProjectInfo>,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
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
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: AppState) -> Result<()> {
    loop {
        terminal.draw(|f| render_ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if app.show_help {
                app.show_help = false;
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Char(' ') => app.toggle_current_selection(),
                KeyCode::Char('a') => app.select_all_visible(),
                KeyCode::Char('d') => app.deselect_all_visible(),
                KeyCode::Char('c') => app.cycle_category(),
                KeyCode::Char('r') => app.cycle_risk(),
                KeyCode::Char('s') => {
                    app.sort_key = app.sort_key.next();
                    app.recompute_visible();
                }
                KeyCode::Char('R') => {
                    app.include_recent = !app.include_recent;
                    app.recompute_visible();
                }
                KeyCode::Char('P') => {
                    app.include_protected = !app.include_protected;
                    app.recompute_visible();
                }
                KeyCode::Backspace => {
                    app.query.pop();
                    app.recompute_visible();
                }
                KeyCode::Char('?') | KeyCode::Char('h') => app.show_help = true,
                KeyCode::Enter => {
                    let selected = app.get_selected_projects();
                    if selected.is_empty() {
                        continue;
                    }
                    disable_raw_mode()?;
                    let cleaner = Cleaner::new().verbose(true);
                    let result = cleaner.clean_multiple(&selected)?;
                    println!("\nCleaning completed!");
                    println!("  Cleaned: {}", result.cleaned_count);
                    println!(
                        "  Skipped: {} ({})",
                        result.skipped_count,
                        format_size(result.bytes_skipped)
                    );
                    println!("  Failed: {}", result.failed_count);
                    println!("  Space freed: {}", result.size_freed_human());
                    return Ok(());
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
                }
                _ => {}
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
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(f.size());

    draw_header(f, chunks[0], app);
    draw_body(f, chunks[1], app);
    draw_footer(f, chunks[2], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &AppState) {
    let category = app
        .category_filter
        .map(|c| c.as_str().to_string())
        .unwrap_or_else(|| "all".to_string());
    let risk = app
        .risk_filter
        .map(|r| r.as_str().to_string())
        .unwrap_or_else(|| "all".to_string());
    let text = vec![
        Line::from(Span::styled(
            "Dev Cleaner - TUI v2",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "Visible: {} | Total visible size: {} | Selected: {} ({})",
            app.visible_indices.len(),
            format_size(app.visible_total_size()),
            app.selected_count(),
            format_size(app.selected_size())
        )),
        Line::from(format!(
            "query=`{}` category={} risk={} sort={} include_recent={} include_protected={}",
            app.query,
            category,
            risk,
            app.sort_key.as_str(),
            app.include_recent,
            app.include_protected
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
            let mut tags = Vec::new();
            if p.in_use {
                tags.push("IN_USE");
            }
            if p.protected {
                tags.push("PROTECTED");
            }
            if p.recent {
                tags.push("RECENT");
            }
            let tags = if tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", tags.join(","))
            };
            let line = format!(
                "{} {:<10} {:>9} {:<12} {}{}",
                selected_marker,
                p.project_type_display_name(),
                format_size(p.size),
                format!("[{}/{}]", p.category, p.risk_level),
                p.cleanable_dir.display(),
                tags
            );
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Targets"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_detail_panel(f: &mut Frame, area: Rect, app: &AppState) {
    let text = if let Some(p) = app.selected_project() {
        vec![
            Line::from(Span::styled(
                p.cleanable_dir.display().to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("Project: {}", p.project_type_display_name())),
            Line::from(format!("Size: {}", format_size(p.size))),
            Line::from(format!("Age: {} days", p.days_since_modified())),
            Line::from(format!("Category: {}", p.category)),
            Line::from(format!("Risk: {}", p.risk_level)),
            Line::from(format!("Confidence: {}", p.confidence)),
            Line::from(format!("In use: {}", p.in_use)),
            Line::from(format!("Protected: {}", p.protected)),
            Line::from(format!("Recent: {}", p.recent)),
            Line::from(format!(
                "Rule: {}",
                p.matched_rule
                    .as_ref()
                    .map(|r| format!("{:?}:{}", r.source, r.pattern))
                    .unwrap_or_else(|| "-".to_string())
            )),
            Line::from(format!(
                "Protected by: {}",
                p.protected_by.clone().unwrap_or_else(|| "-".to_string())
            )),
        ]
    } else {
        vec![Line::from("No visible targets")]
    };

    let panel = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Details"));
    f.render_widget(panel, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &AppState) {
    let help = vec![
        Line::from("↑/↓/j/k move | space toggle | enter clean | q quit | ? help"),
        Line::from("c category | r risk | s sort | R recent toggle | P protected toggle"),
        Line::from("type to search | backspace clear"),
        Line::from(format!(
            "Selected: {} ({})",
            app.selected_count(),
            format_size(app.selected_size())
        )),
    ];
    let footer =
        Paragraph::new(help).block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(footer, area);
}

fn draw_help(f: &mut Frame) {
    let help_text = vec![
        Line::from("Help - TUI v2"),
        Line::from(""),
        Line::from("Navigation: ↑/↓/j/k"),
        Line::from("Selection: space toggle, a select all visible, d deselect visible"),
        Line::from("Filters: c category, r risk, R include recent, P include protected"),
        Line::from("Sort: s cycle size/age/risk"),
        Line::from("Search: type to append query, Backspace to delete"),
        Line::from("Actions: Enter clean selected, q/Esc quit"),
        Line::from(""),
        Line::from("Press any key to close"),
    ];

    let paragraph =
        Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(paragraph, f.size());
}
