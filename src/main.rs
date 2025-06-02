mod scanner;
use color_eyre::eyre::Result;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{self, Color, Modifier, Style, Stylize},
    text::Text,
    widgets::{
        Block, BorderType, Borders, Cell, Gauge, HighlightSpacing, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState,
    },
    DefaultTerminal, Frame,
};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};
use style::palette::tailwind;
use unicode_width::UnicodeWidthStr;

use self::scanner::{HomebrewScanner, ScanningState};

const PALETTES: [tailwind::Palette; 4] = [
    tailwind::BLUE,
    tailwind::EMERALD,
    tailwind::INDIGO,
    tailwind::RED,
];
const INFO_TEXT: [&str; 3] = [
    "(Esc) quit | (↑) move up | (↓) move down | (←) move left | (→) move right",
    "(Shift + →) next color | (Shift + ←) previous color | (Space) Start Scan",
    "(Enter) Select Package | (d) Delete Selected | (r) Refresh",
];

const ITEM_HEIGHT: usize = 4;

fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::new().run(terminal);
    ratatui::restore();
    app_result
}

struct TableColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_row_style_fg: Color,
    selected_column_style_fg: Color,
    selected_cell_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
    footer_border_color: Color,
}

impl TableColors {
    const fn new(color: &tailwind::Palette) -> Self {
        Self {
            buffer_bg: tailwind::SLATE.c950,
            header_bg: color.c900,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_row_style_fg: color.c400,
            selected_column_style_fg: color.c400,
            selected_cell_style_fg: color.c600,
            normal_row_color: tailwind::SLATE.c950,
            alt_row_color: tailwind::SLATE.c900,
            footer_border_color: color.c400,
        }
    }
}

#[derive(Debug, Clone)]
struct Package {
    name: String,
    package_type: PackageType,
    last_accessed: Option<SystemTime>,
    last_accessed_path: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
enum PackageType {
    Formula,
    Cask,
}

impl Package {
    fn get_display_fields(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            match self.package_type {
                PackageType::Formula => "Formula".to_string(),
                PackageType::Cask => "Cask".to_string(),
            },
            self.format_last_accessed(),
            self.last_accessed_path
                .as_deref()
                .unwrap_or("no path")
                .to_string(),
        ]
    }

    fn format_last_accessed(&self) -> String {
        match self.last_accessed {
            Some(time) => {
                match time.elapsed() {
                    Ok(duration) => {
                        let secs = duration.as_secs();

                        if secs < 60 {
                            "Just now".to_string()
                        } else if secs < 3600 {
                            let mins = secs / 60;
                            format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
                        } else if secs < 86400 {
                            let hours = secs / 3600;
                            format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
                        } else if secs < 2592000 {
                            // 30 days
                            let days = secs / 86400;
                            format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
                        } else if secs < 31536000 {
                            // 365 days
                            let months = secs / 2592000;
                            format!("{} month{} ago", months, if months == 1 { "" } else { "s" })
                        } else {
                            let years = secs / 31536000;
                            format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
                        }
                    }
                    Err(_) => {
                        // If we can't calculate elapsed time, show the actual date
                        use std::time::UNIX_EPOCH;
                        if let Ok(duration_since_epoch) = time.duration_since(UNIX_EPOCH) {
                            let timestamp = duration_since_epoch.as_secs();
                            // Simple date formatting (you might want to use chrono crate for better formatting)
                            format!("Timestamp: {}", timestamp)
                        } else {
                            "Unknown date".to_string()
                        }
                    }
                }
            }
            None => "Never accessed".to_string(),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn package_type(&self) -> &str {
        match self.package_type {
            PackageType::Formula => "Formula",
            PackageType::Cask => "Cask",
        }
    }

    fn last_accessed_path(&self) -> &str {
        self.last_accessed_path.as_deref().unwrap_or("")
    }

    fn last_accessed(&self) -> String {
        self.last_accessed
            .map(|time| format!("{:?}", time))
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

#[derive(Debug, Clone)]
enum AppState {
    Table,
    Scanning,
    ScanComplete,
    PackageSelected(usize),
    ConfirmDelete(usize),
    Deleting(usize),
}

struct App {
    state: TableState,
    items: Vec<Package>,
    longest_item_lens: (u16, u16, u16, u16),
    scroll_state: ScrollbarState,
    colors: TableColors,
    color_index: usize,
    app_state: AppState,
    scanner: Option<HomebrewScanner>,
    scan_handle: Option<thread::JoinHandle<()>>,
    delete_output_receiver: Option<mpsc::Receiver<String>>,
    delete_result_receiver: Option<mpsc::Receiver<Result<(), String>>>,
    delete_output: Vec<String>,
    delete_message: Option<String>,
    delete_success: bool,
}

impl App {
    fn new() -> Self {
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: (20, 10, 15, 20),
            scroll_state: ScrollbarState::new(0),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 0,
            items: Vec::new(),
            app_state: AppState::Table,
            scanner: None,
            scan_handle: None,
            delete_output_receiver: None,
            delete_result_receiver: None,
            delete_output: Vec::new(),
            delete_message: None,
            delete_success: false,
        }
    }

    fn start_scanning(&mut self) {
        self.app_state = AppState::Scanning;
        self.items.clear();

        let scanner = HomebrewScanner::new();
        let handle = scanner.start_scan();

        self.scanner = Some(scanner);
        self.scan_handle = Some(handle);
    }

    fn update_scan(&mut self) {
        if let Some(ref scanner) = self.scanner {
            let scanning_state = scanner.get_state();

            if scanning_state.scan_complete {
                self.items = scanner.get_packages();
                self.sort_packages_by_usage();
                self.app_state = AppState::ScanComplete;
                self.longest_item_lens = constraint_len_calculator(&self.items);
                self.scroll_state = ScrollbarState::new(if self.items.is_empty() {
                    0
                } else {
                    (self.items.len() - 1) * ITEM_HEIGHT
                });
                if !self.items.is_empty() {
                    self.state.select(Some(0));
                }
            }
        }
    }

    fn select_package(&mut self) {
        if let Some(selected_index) = self.state.selected() {
            if selected_index < self.items.len() {
                self.app_state = AppState::PackageSelected(selected_index);
            }
        }
    }

    fn confirm_delete(&mut self, package_index: usize) {
        self.app_state = AppState::ConfirmDelete(package_index);
    }

    fn delete_selected_package(&mut self) {
        if let Some(selected_index) = self.state.selected() {
            if selected_index < self.items.len() {
                self.confirm_delete(selected_index);
            }
        }
    }

    fn execute_delete(&mut self, package_index: usize) {
        if package_index < self.items.len() {
            self.app_state = AppState::Deleting(package_index);
            let package = self.items[package_index].clone();

            // Clear previous output
            self.delete_output.clear();

            // Create channels for output and result
            let (output_sender, output_receiver) = mpsc::channel();
            let (result_sender, result_receiver) = mpsc::channel();

            self.delete_output_receiver = Some(output_receiver);
            self.delete_result_receiver = Some(result_receiver);

            // Execute delete in background thread
            thread::spawn(move || {
                let result = HomebrewScanner::delete_package_with_output(&package, output_sender);
                let _ = result_sender.send(result);
            });
        }
    }

    fn check_delete_progress(&mut self) {
        // Check for new output lines
        if let Some(ref receiver) = self.delete_output_receiver {
            while let Ok(line) = receiver.try_recv() {
                self.delete_output.push(line);
                // Keep only the last 20 lines to prevent memory buildup
                if self.delete_output.len() > 20 {
                    self.delete_output.remove(0);
                }
            }
        }

        // Check if deletion completed
        if let Some(ref receiver) = self.delete_result_receiver {
            if let Ok(result) = receiver.try_recv() {
                // Clear receivers
                self.delete_output_receiver = None;
                self.delete_result_receiver = None;

                if let AppState::Deleting(package_index) = self.app_state {
                    let package_name = self
                        .items
                        .get(package_index)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());

                    match result {
                        Ok(()) => {
                            let message =
                                format!("Successfully deleted package '{}'", package_name);
                            self.handle_delete_result(package_index, true, message);
                        }
                        Err(e) => {
                            let message = format!("Failed to delete '{}': {}", package_name, e);
                            self.handle_delete_result(package_index, false, message);
                        }
                    }
                }
            }
        }
    }

    fn handle_delete_result(&mut self, package_index: usize, success: bool, message: String) {
        if success {
            // Remove the package from the list
            if package_index < self.items.len() {
                self.items.remove(package_index);

                self.sort_packages_by_usage();

                // Update table state
                if self.items.is_empty() {
                    self.state.select(None);
                } else if package_index >= self.items.len() {
                    self.state.select(Some(self.items.len() - 1));
                } else {
                    self.state.select(Some(package_index));
                }

                // Recalculate constraints and scroll state
                self.longest_item_lens = constraint_len_calculator(&self.items);
                self.scroll_state = ScrollbarState::new(if self.items.is_empty() {
                    0
                } else {
                    (self.items.len() - 1) * ITEM_HEIGHT
                });
            }
            self.delete_success = true;
        } else {
            self.delete_success = false;
        }

        self.delete_message = Some(message);
        self.app_state = AppState::Table;
    }

    fn sort_packages_by_usage(&mut self) {
        // Simple sort: Only by last accessed time, oldest first
        self.items.sort_by(|a, b| {
            match (&a.last_accessed, &b.last_accessed) {
                (None, None) => std::cmp::Ordering::Equal, // Both never used, keep original order
                (None, Some(_)) => std::cmp::Ordering::Less, // Never used comes first
                (Some(_), None) => std::cmp::Ordering::Greater, // Used comes after never used
                (Some(a_time), Some(b_time)) => a_time.cmp(b_time), // Oldest access time first
            }
        });

        // Reset selection to top after sorting
        if !self.items.is_empty() {
            self.state.select(Some(0));
            self.scroll_state = self.scroll_state.position(0);
        }
    }

    fn get_scanning_state(&self) -> Option<ScanningState> {
        self.scanner.as_ref().map(|s| s.get_state())
    }

    pub fn next_row(&mut self) {
        if !matches!(self.app_state, AppState::Table) || self.items.is_empty() {
            return;
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };

        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    pub fn previous_row(&mut self) {
        if !matches!(self.app_state, AppState::Table) || self.items.is_empty() {
            return;
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    pub fn next_column(&mut self) {
        if matches!(self.app_state, AppState::Table) {
            self.state.select_next_column();
        }
    }

    pub fn previous_column(&mut self) {
        if matches!(self.app_state, AppState::Table) {
            self.state.select_previous_column();
        }
    }

    pub fn next_color(&mut self) {
        self.color_index = (self.color_index + 1) % PALETTES.len();
    }

    pub fn previous_color(&mut self) {
        let count = PALETTES.len();
        self.color_index = (self.color_index + count - 1) % count;
    }

    pub fn set_colors(&mut self) {
        self.colors = TableColors::new(&PALETTES[self.color_index]);
    }

    pub fn toggle_pause(&mut self) {
        if let Some(ref scanner) = self.scanner {
            scanner.toggle_pause();
        }
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            // Update scanning progress
            if matches!(self.app_state, AppState::Scanning) {
                self.update_scan();
            }

            if matches!(self.app_state, AppState::Deleting(_)) {
                self.check_delete_progress();
            }

            // Handle events with timeout for responsive UI
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        let shift_pressed = key.modifiers.contains(KeyModifiers::SHIFT);
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char(' ') => match self.app_state {
                                AppState::Table => self.start_scanning(),
                                AppState::Scanning => self.toggle_pause(),
                                AppState::ScanComplete => self.app_state = AppState::Table,
                                AppState::PackageSelected(_) => self.app_state = AppState::Table,
                                AppState::ConfirmDelete(_) => self.app_state = AppState::Table,
                                AppState::Deleting(_) => {}
                            },
                            KeyCode::Enter => match self.app_state {
                                AppState::Table => self.select_package(),
                                AppState::ScanComplete => self.app_state = AppState::Table,
                                AppState::PackageSelected(_) => self.app_state = AppState::Table,
                                AppState::ConfirmDelete(idx) => self.execute_delete(idx),
                                _ => {}
                            },
                            KeyCode::Char('d') | KeyCode::Delete => match self.app_state {
                                AppState::Table => self.delete_selected_package(),
                                AppState::PackageSelected(idx) => self.confirm_delete(idx),
                                _ => {}
                            },
                            KeyCode::Char('r') => {
                                if matches!(self.app_state, AppState::Table) {
                                    self.start_scanning();
                                }
                            }
                            KeyCode::Char('y') => {
                                if let AppState::ConfirmDelete(idx) = self.app_state {
                                    self.execute_delete(idx);
                                }
                            }
                            KeyCode::Char('n') => {
                                if matches!(self.app_state, AppState::ConfirmDelete(_)) {
                                    self.app_state = AppState::Table;
                                }
                            }
                            KeyCode::Char('j') | KeyCode::Down => self.next_row(),
                            KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
                            KeyCode::Char('l') | KeyCode::Right if shift_pressed => {
                                self.next_color()
                            }
                            KeyCode::Char('h') | KeyCode::Left if shift_pressed => {
                                self.previous_color();
                            }
                            KeyCode::Char('l') | KeyCode::Right => self.next_column(),
                            KeyCode::Char('h') | KeyCode::Left => self.previous_column(),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        self.set_colors();

        match self.app_state {
            AppState::Scanning => self.render_scanning_ui(frame),
            AppState::ScanComplete => self.render_scan_complete_ui(frame),
            AppState::PackageSelected(idx) => self.render_package_details(frame, idx),
            AppState::ConfirmDelete(idx) => self.render_confirm_delete(frame, idx),
            AppState::Deleting(idx) => self.render_deleting(frame, idx),
            AppState::Table => {
                let vertical = &Layout::vertical([Constraint::Min(5), Constraint::Length(6)]);
                let rects = vertical.split(frame.area());

                self.render_table(frame, rects[0]);
                if !self.items.is_empty() {
                    self.render_scrollbar(frame, rects[0]);
                }
                self.render_footer(frame, rects[1]);
            }
        }
    }
    fn render_scanning_ui(&self, frame: &mut Frame) {
        let scanning_state = self.get_scanning_state().unwrap_or_else(ScanningState::new);

        let scanning_block = Block::default()
            .title("🔍 Homebrew Package Scanner")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.colors.footer_border_color))
            .style(Style::default().bg(self.colors.buffer_bg));

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(1), // Status
                Constraint::Length(1), // Empty space
                Constraint::Length(3), // Progress bar
                Constraint::Length(1), // Empty space
                Constraint::Length(1), // Packages found
                Constraint::Length(1), // Current scanning
                Constraint::Length(1), // Elapsed time
                Constraint::Length(1), // Error message (if any)
                Constraint::Length(1), // Empty space
                Constraint::Length(1), // Controls
            ])
            .split(scanning_block.inner(frame.area()));

        frame.render_widget(scanning_block, frame.area());

        // Status text
        let status_text = if let Some(ref error) = scanning_state.error_message {
            format!("Error: {}", error)
        } else if scanning_state.is_paused {
            "Status: Scanning paused...".to_string()
        } else {
            "Status: Scanning installed packages...".to_string()
        };

        let status_color = if scanning_state.error_message.is_some() {
            Color::Red
        } else {
            self.colors.row_fg
        };

        let status = Paragraph::new(status_text).style(Style::default().fg(status_color));
        frame.render_widget(status, chunks[0]);

        // Progress bar
        let progress = Gauge::default()
            .block(Block::default().title("Progress").borders(Borders::ALL))
            .gauge_style(Style::default().fg(self.colors.footer_border_color))
            .percent(scanning_state.progress_percentage())
            .label(format!(
                "{}% ({}/{})",
                scanning_state.progress_percentage(),
                scanning_state.packages_scanned,
                scanning_state.total_packages
            ));
        frame.render_widget(progress, chunks[2]);

        // Package count
        let found = Paragraph::new(format!(
            "📦 Packages Found: {}",
            scanning_state.packages_found
        ))
        .style(Style::default().fg(Color::Green));
        frame.render_widget(found, chunks[4]);

        // Current scanning
        let current = Paragraph::new(format!("📁 Current: {}", scanning_state.current_path))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(current, chunks[5]);

        // Elapsed time
        let elapsed = Paragraph::new(format!("⏱️  Elapsed: {}", scanning_state.format_elapsed()))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(elapsed, chunks[6]);

        // Error message (if any)
        if let Some(ref error) = scanning_state.error_message {
            let error_msg = Paragraph::new(format!("❌ Error: {}", error))
                .style(Style::default().fg(Color::Red));
            frame.render_widget(error_msg, chunks[7]);
        }

        // Controls
        let controls_text = if scanning_state.error_message.is_some() {
            "[Space] Retry  [ESC] Cancel"
        } else if scanning_state.is_paused {
            "[Space] Resume  [ESC] Cancel"
        } else {
            "[Space] Pause  [ESC] Cancel"
        };
        let controls = Paragraph::new(controls_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(controls, chunks[9]);
    }

    fn render_scan_complete_ui(&self, frame: &mut Frame) {
        let scanning_state = self.get_scanning_state().unwrap_or_else(ScanningState::new);

        let complete_block = Block::default()
            .title("✅ Scan Complete!")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .style(Style::default().bg(self.colors.buffer_bg));

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(2), // Summary
                Constraint::Length(1), // Empty space
                Constraint::Length(1), // Packages found
                Constraint::Length(1), // Time taken
                Constraint::Length(1), // Empty space
                Constraint::Length(1), // Controls
            ])
            .split(complete_block.inner(frame.area()));

        frame.render_widget(complete_block, frame.area());

        // Summary
        let summary = Paragraph::new(
            "Scanning completed successfully!\nPress Enter or Space to view results.",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(self.colors.row_fg));
        frame.render_widget(summary, chunks[0]);

        // Package count
        let found = Paragraph::new(format!(
            "📦 Total Packages Found: {}",
            scanning_state.packages_found
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Green));
        frame.render_widget(found, chunks[2]);

        // Time taken
        let time_taken = Paragraph::new(format!(
            "⏱️  Total Time: {}",
            scanning_state.format_elapsed()
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
        frame.render_widget(time_taken, chunks[3]);

        // Controls
        let controls = Paragraph::new("[Enter/Space] View Results  [ESC] Quit")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(controls, chunks[5]);
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        if self.items.is_empty() {
            let empty_msg = Paragraph::new("No packages found. Press Space to start scanning.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray))
                .block(
                    Block::default()
                        .title("Homebrew Packages")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(self.colors.footer_border_color)),
                );
            frame.render_widget(empty_msg, area);
            return;
        }

        let header_style = Style::default()
            .fg(self.colors.header_fg)
            .bg(self.colors.header_bg);

        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_row_style_fg);

        let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);

        let selected_cell_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_cell_style_fg);

        let header = [
            "Package Name",
            "Type",
            "Last Accessed",
            "Last Accessed Path",
        ]
        .into_iter()
        .map(Cell::from)
        .collect::<Row>()
        .style(header_style)
        .height(1);

        let rows = self.items.iter().enumerate().map(|(i, package)| {
            let color = match i % 2 {
                0 => self.colors.normal_row_color,
                _ => self.colors.alt_row_color,
            };
            let item = package.get_display_fields();
            item.into_iter()
                .map(|content| Cell::from(Text::from(format!("\n {content} \n"))))
                .collect::<Row>()
                .style(Style::new().fg(self.colors.row_fg).bg(color))
                .height(4)
        });

        let bar = " █ ";

        let t = Table::new(
            rows,
            [
                Constraint::Length(self.longest_item_lens.0 + 10),
                Constraint::Min(self.longest_item_lens.1 + 3),
                Constraint::Min(self.longest_item_lens.2),
                Constraint::Min(self.longest_item_lens.3),
            ],
        )
        .header(header)
        .row_highlight_style(selected_row_style)
        .column_highlight_style(selected_col_style)
        .cell_highlight_style(selected_cell_style)
        .highlight_symbol(Text::from(vec![
            "".into(),
            bar.into(),
            bar.into(),
            "".into(),
        ]))
        .bg(self.colors.buffer_bg)
        .highlight_spacing(HighlightSpacing::Always);

        frame.render_stateful_widget(t, area, &mut self.state);
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.scroll_state,
        );
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let info_footer = Paragraph::new(Text::from_iter(INFO_TEXT))
            .style(
                Style::new()
                    .fg(self.colors.row_fg)
                    .bg(self.colors.buffer_bg),
            )
            .centered()
            .block(
                Block::bordered()
                    .border_type(BorderType::Double)
                    .border_style(Style::new().fg(self.colors.footer_border_color)),
            );
        frame.render_widget(info_footer, area);
    }

    fn render_package_details(&self, frame: &mut Frame, package_index: usize) {
        if package_index >= self.items.len() {
            return;
        }

        let package = &self.items[package_index];

        let details_block = Block::default()
            .title(format!("📦 Package Details: {}", package.name))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.colors.footer_border_color))
            .style(Style::default().bg(self.colors.buffer_bg));

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(2), // Name and type
                Constraint::Length(2), // Last accessed
                Constraint::Length(2), // Path
                Constraint::Length(1), // Empty space
                Constraint::Length(1), // Controls
            ])
            .split(details_block.inner(frame.area()));

        frame.render_widget(details_block, frame.area());

        // Package name and type
        let name_type = Paragraph::new(format!(
            "Name: {}\nType: {}",
            package.name,
            package.package_type()
        ))
        .style(Style::default().fg(self.colors.row_fg));
        frame.render_widget(name_type, chunks[0]);

        // Last accessed
        let accessed = Paragraph::new(format!("Last Accessed: {}", package.format_last_accessed()))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(accessed, chunks[1]);

        // Path
        let path = Paragraph::new(format!(
            "Path: {}",
            package.last_accessed_path.as_deref().unwrap_or("Unknown")
        ))
        .style(Style::default().fg(Color::Cyan));
        frame.render_widget(path, chunks[2]);

        // Controls
        let controls = Paragraph::new("[Enter/Space] Back  [d] Delete  [ESC] Quit")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(controls, chunks[4]);
    }

    fn render_confirm_delete(&self, frame: &mut Frame, package_index: usize) {
        if package_index >= self.items.len() {
            return;
        }

        let package = &self.items[package_index];

        let confirm_block = Block::default()
            .title("⚠️  Confirm Delete")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .style(Style::default().bg(self.colors.buffer_bg));

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3), // Warning message
                Constraint::Length(2), // Package info
                Constraint::Length(1), // Empty space
                Constraint::Length(1), // Controls
            ])
            .split(confirm_block.inner(frame.area()));

        frame.render_widget(confirm_block, frame.area());

        // Warning message
        let warning = Paragraph::new(format!(
            "Are you sure you want to delete '{}'?\n\nThis action cannot be undone!",
            package.name
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red));
        frame.render_widget(warning, chunks[0]);

        // Package info
        let info = Paragraph::new(format!(
            "Type: {}\nPath: {}",
            package.package_type(),
            package.last_accessed_path.as_deref().unwrap_or("Unknown")
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(self.colors.row_fg));
        frame.render_widget(info, chunks[1]);

        // Controls
        let controls =
            Paragraph::new("[y] Yes, Delete  [n] No, Cancel  [Enter] Delete  [Space] Cancel")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
        frame.render_widget(controls, chunks[3]);
    }

    fn render_deleting(&self, frame: &mut Frame, package_index: usize) {
        if package_index >= self.items.len() {
            return;
        }

        let package = &self.items[package_index];

        let deleting_block = Block::default()
            .title("🗑️  Uninstalling Package")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(self.colors.buffer_bg));

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // Package info
                Constraint::Length(1), // Empty line
                Constraint::Min(5),    // Command output
                Constraint::Length(1), // Controls
            ])
            .split(deleting_block.inner(frame.area()));

        frame.render_widget(deleting_block, frame.area());

        // Package info
        let package_info = Paragraph::new(format!(
            "Uninstalling: {} ({})",
            package.name,
            package.package_type()
        ))
        .style(Style::default().fg(Color::Yellow));
        frame.render_widget(package_info, chunks[0]);

        // Command output
        let output_text = if self.delete_output.is_empty() {
            "Starting uninstall process...".to_string()
        } else {
            self.delete_output.join("\n")
        };

        let output_block = Block::default()
            .title("Command Output")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let output_paragraph = Paragraph::new(output_text)
            .block(output_block)
            .style(Style::default().fg(Color::Green))
            .scroll((
                if self.delete_output.len() > 10 {
                    self.delete_output.len().saturating_sub(10) as u16
                } else {
                    0
                },
                0,
            ));

        frame.render_widget(output_paragraph, chunks[2]);

        // Controls
        let controls = Paragraph::new("[c] Stop Watching  [ESC] Force Quit")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(controls, chunks[3]);
    }
}

fn constraint_len_calculator(items: &[Package]) -> (u16, u16, u16, u16) {
    if items.is_empty() {
        return (20, 10, 15, 20);
    }

    let name_len = items
        .iter()
        .map(Package::name)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    let type_len = items
        .iter()
        .map(Package::package_type)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    let last_accessed_path_len = items
        .iter()
        .map(Package::last_accessed_path)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    let last_accessed_time_len = items
        .iter()
        .map(|package| package.last_accessed())
        .map(|s| s.width())
        .max()
        .unwrap_or(0);

    (
        name_len as u16,
        type_len as u16,
        last_accessed_path_len as u16,
        last_accessed_time_len as u16,
    )
}
