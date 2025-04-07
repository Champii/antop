pub mod formatters;
pub mod widgets;

// --- Imports (Combined and adjusted from src/ui.rs) ---
use self::widgets::{render_header, render_node_row};
use crate::{app::App, cli::Cli, discovery::find_metrics_nodes, fetch::fetch_metrics};
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
    _cli: &Cli,
    effective_log_path: &str,
) -> Result<()> {
    let mut tick_timer = interval(Duration::from_secs(1)); // Refresh data every second
    let mut discover_timer = interval(Duration::from_secs(60)); // Check for new nodes every 60s

    if !app.nodes.is_empty() {
        let urls: Vec<String> = app.nodes.iter().map(|(_, url)| url.clone()).collect();
        let initial_results = fetch_metrics(&urls).await;
        app.update_metrics(initial_results);
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        tokio::select! {
            _ = tick_timer.tick() => {
                if !app.nodes.is_empty() {
                    let urls: Vec<String> = app.nodes.iter().map(|(_, url)| url.clone()).collect();
                    let results = fetch_metrics(&urls).await;
                    app.update_metrics(results);
                }
            },
            _ = discover_timer.tick() => {
                let log_path_buf = std::path::PathBuf::from(effective_log_path);
                match find_metrics_nodes(log_path_buf).await {
                    Ok(mut found_nodes) => {
                        found_nodes.sort_by(|a, b| a.0.cmp(&b.0));
                        found_nodes.dedup_by(|a, b| a.1 == b.1);

                        for (name, _url) in &found_nodes {
                            if !app.node_metrics.contains_key(name) {
                                app.node_metrics.insert(name.clone(), Err("Discovered - Fetching...".to_string()));
                            }
                        }

                        let lists_differ = app.nodes != found_nodes;

                        if lists_differ {
                            app.nodes = found_nodes;
                        }
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Error re-discovering nodes: {}", e));
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
                        app.status_message = Some(format!("Input polling error: {}", e));
                    }
                    Err(e) => {
                         app.status_message = Some(format!("Input task spawn error: {}", e));
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
                Constraint::Length(2), // Top Title
                Constraint::Length(2), // Summary Gauges (Height reduced to 2 as Peers is now separate)
                Constraint::Min(0),    // Node Table
                Constraint::Length(1), // Bottom Status / Error
            ]
            .as_ref(),
        )
        .split(f.area());

    let title = Paragraph::new("Autonomi Node Dashboard").style(Style::default().fg(Color::White));
    f.render_widget(title, main_chunks[0]);

    // Render summary gauges in the new chunk
    widgets::render_summary_gauges(f, app, main_chunks[1]);

    // Render node table in the adjusted chunk
    render_custom_node_rows(f, app, main_chunks[2]);

    // Determine status text: Show error/message if present, otherwise default status
    let status_content = if let Some(msg) = &app.status_message {
        Paragraph::new(msg.clone()).style(Style::default().fg(Color::Red)) // Style errors in Red
    } else {
        let default_status = format!(
            "Last update: {}s ago | {} nodes | Press 'q' to quit",
            app.last_update.elapsed().as_secs(),
            app.nodes.len()
        );
        Paragraph::new(default_status).alignment(Alignment::Right)
    };

    // Render status/error in the last chunk
    f.render_widget(status_content, main_chunks[3]);

    // Clear the status message after displaying it once (optional, remove if messages should persist)
    // app.status_message = None;
}

/// Renders the main content area containing the node list (header + rows).
/// Delegates rendering the header and individual rows to helper functions.
// This function is also internal to the ui module, called by ui
fn render_custom_node_rows(f: &mut Frame, app: &mut App, area: Rect) {
    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let num_nodes = app.nodes.len();
    if num_nodes == 0 {
        let no_nodes_text = Paragraph::new("No nodes discovered yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_nodes_text, inner_area);
        return;
    }

    let mut constraints = vec![Constraint::Length(1)];
    constraints.extend(std::iter::repeat_n(Constraint::Length(1), num_nodes));

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    render_header(f, vertical_chunks[0]);

    for (i, (name, url)) in app.nodes.iter().enumerate() {
        if i + 1 >= vertical_chunks.len() {
            continue;
        }
        let row_area = vertical_chunks[i + 1];
        render_node_row(f, app, row_area, name, url);
    }
}
