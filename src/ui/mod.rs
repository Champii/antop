// Declare the submodules within the ui module
pub mod formatters;
pub mod widgets;

// --- Imports (Combined and adjusted from src/ui.rs) ---
// Import necessary items from crate modules
use crate::{
    app::App,
    cli::Cli,
    discovery::find_metrics_servers,
    fetch::fetch_metrics,
    // NodeMetrics is implicitly used via widgets/formatters
};
// Import local UI submodules (now using `self`)
use self::widgets::{render_header, render_node_row}; // Be specific

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

    // Initial fetch if servers were found initially
    if !app.servers.is_empty() {
        let urls: Vec<String> = app.servers.iter().map(|(_, url)| url.clone()).collect();
        let initial_results = fetch_metrics(&urls).await;
        app.update_metrics(initial_results);
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?; // Draw UI

        // Combine timers and input polling
        tokio::select! {
            _ = tick_timer.tick() => {
                // Fetch metrics for current servers periodically
                if !app.servers.is_empty() {
                    let urls: Vec<String> = app.servers.iter().map(|(_, url)| url.clone()).collect();
                    let results = fetch_metrics(&urls).await;
                    app.update_metrics(results);
                }
            },
            _ = discover_timer.tick() => {
                 // Discover servers periodically
                match find_metrics_servers(cli.logs.as_deref()) {
                    Ok(mut found_servers) => { // Now Vec<(String, String)>
                        // Sort by name, deduplicate by URL (as done in discovery.rs)
                        found_servers.sort_by(|a, b| a.0.cmp(&b.0));
                        found_servers.dedup_by(|a, b| a.1 == b.1);

                        // Add any newly discovered servers to the metrics map
                        for (name, url) in &found_servers {
                            if !app.metrics.contains_key(url) {
                                app.metrics.insert(url.clone(), Err(format!("Discovered {} - Fetching...", name)));
                            }
                        }

                        // Check if the server list itself has changed (order or content)
                        let lists_differ = app.servers != found_servers;

                        if lists_differ {
                            app.servers = found_servers;
                            // Optional: Remove metrics for servers no longer present
                            // app.metrics.retain(|url, _| app.servers.iter().any(|(_, u)| u == url));
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
                    Ok(Ok(true)) => { // Successfully polled and event is available
                        if let Event::Key(key) = event::read()? {
                            if key.code == KeyCode::Char('q') {
                                return Ok(()); // Quit signal
                            }
                            // Handle other keys like scrolling here if needed
                            // match key.code {
                            //     KeyCode::Up => app.previous(),
                            //     KeyCode::Down => app.next(),
                            //     _ => {}
                            // }
                        }
                    }
                    Ok(Ok(false)) => { /* Timeout, no event */ }
                    Ok(Err(e)) => {
                        // Handle polling error
                        eprintln!("Input polling error: {}", e);
                    }
                    Err(e) => {
                         // Handle task spawn error
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
    // Main vertical layout: Title, Content Area, Status
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(1), // Title/Instructions
                Constraint::Min(0),    // Content Area (List + Chart)
                Constraint::Length(1), // Status/Last Update
            ]
            .as_ref(),
        )
        .split(f.area());

    let title = Paragraph::new(
        "Autonomi Node Dashboard | Press 'q' to quit | Use Up/Down keys to select node for chart",
    )
    .style(Style::default().fg(Color::White));
    f.render_widget(title, main_chunks[0]);

    // Render the custom node rows in the main content area using the refactored function
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
    }); // Area inside potential borders

    let num_servers = app.servers.len();
    if num_servers == 0 {
        let no_servers_text = Paragraph::new("No servers discovered yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_servers_text, inner_area);
        return;
    }

    // Layout: Header row + one row per server
    let mut constraints = vec![Constraint::Length(1)]; // Header row height
    constraints.extend(std::iter::repeat(Constraint::Length(1)).take(num_servers)); // Data row height

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    // Render Header using the helper function from ui::widgets
    render_header(f, vertical_chunks[0]); // Uses `self::widgets::render_header`

    // Render Data Rows using the helper function from ui::widgets (starting from index 1)
    for (i, (name, url)) in app.servers.iter().enumerate() {
        if i + 1 >= vertical_chunks.len() {
            continue; // Bounds check
        }
        let row_area = vertical_chunks[i + 1]; // Get the area for this specific row
        render_node_row(f, app, row_area, name, url); // Uses `self::widgets::render_node_row`
    }
}
