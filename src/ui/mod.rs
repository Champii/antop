pub mod formatters;
pub mod widgets;

// --- Imports (Combined and adjusted from src/ui.rs) ---
use self::widgets::{render_header, render_node_row};
use crate::{app::App, cli::Cli, discovery::find_metrics_servers, fetch::fetch_metrics};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use std::{
    io::{self, Stdout},
    time::Duration,
};
use tokio::time::interval;

// --- TUI Setup and Restore ---

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("Failed to create terminal")
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

// --- Main Application Loop ---

pub async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    cli: &Cli,
) -> Result<()> {
    let mut tick_timer = interval(Duration::from_secs(1)); // Refresh data every second
    let mut discover_timer = interval(Duration::from_secs(60)); // Check for new servers every 60s

    if !app.servers.is_empty() {
        let urls: Vec<String> = app.servers.iter().map(|(_, url)| url.clone()).collect();
        let initial_results = fetch_metrics(&urls).await;
        app.update_metrics(initial_results);
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        tokio::select! {
            _ = tick_timer.tick() => {
                if !app.servers.is_empty() {
                    let urls: Vec<String> = app.servers.iter().map(|(_, url)| url.clone()).collect();
                    let results = fetch_metrics(&urls).await;
                    app.update_metrics(results);
                }
            },
            _ = discover_timer.tick() => {
                match find_metrics_servers(cli.logs.as_deref()) {
                    Ok(mut found_servers) => {
                        // Sort by name, deduplicate by URL (as done in discovery.rs)
                        found_servers.sort_by(|a, b| a.0.cmp(&b.0));
                        found_servers.dedup_by(|a, b| a.1 == b.1);

                        for (name, url) in &found_servers {
                            if !app.metrics.contains_key(url) {
                                app.metrics.insert(url.clone(), Err(format!("Discovered {} - Fetching...", name)));
                            }
                        }

                        let lists_differ = app.servers != found_servers;

                        if lists_differ {
                            app.servers = found_servers;
                        }

                    }
                    Err(e) => {
                        // Log error, maybe display in UI status bar later
                        eprintln!("Error re-discovering metrics servers: {}", e);
                    }
                }
            },
            // Poll for keyboard events using tokio's spawn_blocking for crossterm event handling
            result = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(100))) => {
                 match result {
                    Ok(Ok(true)) => {
                        if let Event::Key(key) = event::read()? {
                            if key.code == KeyCode::Char('q') {
                                return Ok(());
                            }
                        }
                    }
                    Ok(Ok(false)) => {}
                    Ok(Err(e)) => {
                        eprintln!("Input polling error: {}", e);
                    }
                    Err(e) => {
                         eprintln!("Input task spawn error: {}", e);
                    }
                }
            }
        }
    }
}

// --- UI Rendering ---

// This function is now internal to the ui module, called by run_app
fn ui(f: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.area());

    let title = Paragraph::new("Autonomi Node Dashboard").style(Style::default().fg(Color::White));
    f.render_widget(title, main_chunks[0]);

    let title = Paragraph::new("Press 'q' to quit").style(Style::default().fg(Color::White));
    f.render_widget(title, main_chunks[2]);

    render_custom_node_rows(f, app, main_chunks[1]);

    let status_text = format!(
        "Last update: {}s ago | {} servers",
        app.last_update.elapsed().as_secs(),
        app.servers.len()
    );
    let status = Paragraph::new(status_text).alignment(Alignment::Right);
    f.render_widget(status, main_chunks[2]);
}

/// Renders the main content area containing the node list (header + rows).
/// Delegates rendering the header and individual rows to helper functions.
// This function is also internal to the ui module, called by ui
fn render_custom_node_rows(f: &mut Frame, app: &mut App, area: Rect) {
    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let num_servers = app.servers.len();
    if num_servers == 0 {
        let no_servers_text = Paragraph::new("No servers discovered yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_servers_text, inner_area);
        return;
    }

    let mut constraints = vec![Constraint::Length(1)];
    constraints.extend(std::iter::repeat(Constraint::Length(1)).take(num_servers));

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    render_header(f, vertical_chunks[0]);

    for (i, (name, url)) in app.servers.iter().enumerate() {
        if i + 1 >= vertical_chunks.len() {
            continue;
        }
        let row_area = vertical_chunks[i + 1];
        render_node_row(f, app, row_area, name, url);
    }
}
