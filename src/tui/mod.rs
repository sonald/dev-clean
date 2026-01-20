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

struct App {
    projects: Vec<ProjectInfo>,
    list_state: ListState,
    selected: Vec<bool>,
    total_size: u64,
    selected_size: u64,
    show_help: bool,
}

impl App {
    fn new(projects: Vec<ProjectInfo>) -> Self {
        let total_size = projects.iter().map(|p| p.size).sum();
        let selected = vec![false; projects.len()];

        let mut app = Self {
            projects,
            list_state: ListState::default(),
            selected,
            total_size,
            selected_size: 0,
            show_help: false,
        };

        if !app.projects.is_empty() {
            app.list_state.select(Some(0));
        }

        app
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.projects.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.projects.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn toggle_selection(&mut self) {
        if let Some(i) = self.list_state.selected() {
            self.selected[i] = !self.selected[i];
            self.update_selected_size();
        }
    }

    fn select_all(&mut self) {
        for sel in &mut self.selected {
            *sel = true;
        }
        self.update_selected_size();
    }

    fn deselect_all(&mut self) {
        for sel in &mut self.selected {
            *sel = false;
        }
        self.update_selected_size();
    }

    fn update_selected_size(&mut self) {
        self.selected_size = self
            .projects
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected[*i])
            .map(|(_, p)| p.size)
            .sum();
    }

    fn get_selected_projects(&self) -> Vec<ProjectInfo> {
        self.projects
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected[*i])
            .map(|(_, p)| p.clone())
            .collect()
    }

    fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

pub fn run_tui(path: PathBuf) -> Result<()> {
    // Scan for projects
    let config = Config::load_or_default(Config::default_path())?;
    run_tui_with_config(path, &config)
}

pub fn run_tui_with_config(path: PathBuf, config: &Config) -> Result<()> {
    // Scan for projects
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

    if projects.is_empty() {
        println!("No cleanable directories found.");
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let app = App::new(projects);
    let res = run_app(&mut terminal, app);

    // Restore terminal
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

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Char(' ') => app.toggle_selection(),
                KeyCode::Char('a') => app.select_all(),
                KeyCode::Char('d') => app.deselect_all(),
                KeyCode::Char('?') | KeyCode::Char('h') => app.toggle_help(),
                KeyCode::Enter => {
                    let selected = app.get_selected_projects();
                    if !selected.is_empty() {
                        // Exit TUI and perform cleaning
                        // Terminal will be dropped when function returns
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
                }
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    if app.show_help {
        draw_help(f);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(f.size());

    // Header
    draw_header(f, chunks[0], app);

    // Project list
    draw_project_list(f, chunks[1], app);

    // Footer
    draw_footer(f, chunks[2], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let text = vec![
        Line::from(Span::styled(
            "Dev Cleaner - Interactive Mode",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::raw(format!(
            "Found {} cleanable directories | Total size: {}",
            app.projects.len(),
            format_size(app.total_size)
        ))),
    ];

    let paragraph =
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Info"));

    f.render_widget(paragraph, area);
}

fn draw_project_list(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .enumerate()
        .map(|(i, project)| {
            let selected_marker = if app.selected[i] { "[✓] " } else { "[ ] " };

            let project_type = project.project_type.name();
            let in_use = if project.in_use { " [IN USE]" } else { "" };

            let content = format!(
                "{}{} - {} - {}{}",
                selected_marker,
                project_type,
                project.cleanable_dir.display(),
                format_size(project.size),
                in_use
            );

            let style = if app.selected[i] {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Projects"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let selected_count = app.selected.iter().filter(|&&s| s).count();

    let text = vec![
        Line::from(Span::raw(format!(
            "Selected: {} | Size to free: {}",
            selected_count,
            format_size(app.selected_size)
        ))),
        Line::from(""),
        Line::from(vec![
            Span::raw("Space: "),
            Span::styled("Toggle", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | a: "),
            Span::styled("Select All", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | d: "),
            Span::styled(
                "Deselect All",
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("Enter: "),
            Span::styled("Clean", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | ?: "),
            Span::styled("Help", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | q: "),
            Span::styled("Quit", Style::default().add_modifier(Modifier::BOLD)),
        ]),
    ];

    let paragraph =
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Controls"));

    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut Frame) {
    let help_text = vec![
        Line::from(Span::styled(
            "Help - Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  ↑/k      - Move up"),
        Line::from("  ↓/j      - Move down"),
        Line::from(""),
        Line::from("Selection:"),
        Line::from("  Space    - Toggle selection"),
        Line::from("  a        - Select all"),
        Line::from("  d        - Deselect all"),
        Line::from(""),
        Line::from("Actions:"),
        Line::from("  Enter    - Clean selected directories"),
        Line::from("  ?/h      - Toggle this help"),
        Line::from("  q/Esc    - Quit"),
        Line::from(""),
        Line::from("Press any key to close this help..."),
    ];

    let paragraph =
        Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));

    f.render_widget(paragraph, f.size());
}
