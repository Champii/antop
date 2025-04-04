use crate::{
    app::App,
    cli::Cli,
    discovery::find_metrics_servers,
    fetch::fetch_metrics,
    metrics::NodeMetrics, // Needed for create_metrics_cells
};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use humansize::{DECIMAL, format_size}; // Re-added for BW formatting
use ratatui::{
    Frame,
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text}, // Updated Text imports
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph}, // Replaced Table etc. with List
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

// --- Main Application Loop (moved from main.rs) ---

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

                        // let mut metrics_updated = false; // Removed unused variable
                        // Add any newly discovered servers to the metrics map
                        for (name, url) in &found_servers {
                            if !app.metrics.contains_key(url) {
                                app.metrics.insert(url.clone(), Err(format!("Discovered {} - Fetching...", name)));
                                // metrics_updated = true; // Removed unused assignment
                            }
                        }

                        // Check if the server list itself has changed (order or content)
                        let lists_differ = app.servers != found_servers;

                        if lists_differ {
                            app.servers = found_servers;
                            // Optional: Remove metrics for servers no longer present
                            // app.metrics.retain(|url, _| app.servers.iter().any(|(_, u)| u == url));
                        }

                        // Consider triggering an immediate fetch if new servers were added or list changed significantly
                        // if metrics_updated || lists_differ { ... }

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

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1), // Title/Instructions
                Constraint::Min(0),    // Table
                Constraint::Length(1), // Status/Last Update
            ]
            .as_ref(),
        )
        .split(f.area()); // Use f.area() instead of deprecated f.size()

    let title = Paragraph::new("Autonomi Node Dashboard | Press 'q' to quit");
    f.render_widget(title, chunks[0]);

    render_metrics_list(f, app, chunks[1]); // Renamed function call

    let status_text = format!(
        "Last update: {}s ago | {} servers",
        app.last_update.elapsed().as_secs(),
        app.servers.len()
    );
    let status = Paragraph::new(status_text); //.alignment(Alignment::Right); // Requires ratatui::layout::Alignment
    f.render_widget(status, chunks[2]);
}

fn render_metrics_list(f: &mut Frame, app: &mut App, area: Rect) {
    // Renamed function
    let list_block = Block::default().borders(Borders::ALL).title("Nodes");

    // Create list items from server data
    let items: Vec<ListItem> = app
        .servers
        .iter()
        .map(|(name, url)| {
            let metrics_result = app.metrics.get(url);
            let (text, style) = match metrics_result {
                Some(Ok(metrics)) => (
                    create_list_item_text(name, metrics),
                    Style::default().fg(Color::Green), // Green for Running
                ),
                Some(Err(_)) => (
                    format!("{:<20} | Status: Stopped", name), // Simplified error display
                    Style::default().fg(Color::Yellow),        // Yellow for Stopped/Error
                ),
                None => (
                    format!("{:<20} | Status: Unknown", name), // Should ideally not happen if initialized
                    Style::default().fg(Color::DarkGray),
                ),
            };
            ListItem::new(Text::styled(text, style))
        })
        .collect();

    // Create the List widget
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White)) // Basic highlight style
        .highlight_symbol("> "); // Symbol for selected item

    // Render the list using ListState
    f.render_stateful_widget(list, area, &mut app.list_state);
} // End of render_metrics_list

// --- UI Helper Functions ---

// Helper to format Option<T> for display
fn format_option<T: std::fmt::Display>(opt: Option<T>) -> String {
    match opt {
        Some(val) => val.to_string(),
        None => "-".to_string(),
    }
}

// Helper to format uptime into d HH:MM:SS or HH:MM:SS
fn format_uptime(seconds: Option<u64>) -> String {
    match seconds {
        Some(s) => {
            let days = s / (24 * 3600);
            let hours = (s % (24 * 3600)) / 3600;
            let minutes = (s % 3600) / 60;
            let secs = s % 60;
            if days > 0 {
                format!("{}d {:02}:{:02}:{:02}", days, hours, minutes, secs)
            } else {
                format!("{:02}:{:02}:{:02}", hours, minutes, secs)
            }
        }
        None => "-".to_string(),
    }
}

// Helper to format Option<f64> with specified precision
fn format_float(opt: Option<f64>, precision: usize) -> String {
    match opt {
        Some(val) => format!("{:.prec$}", val, prec = precision),
        None => "-".to_string(),
    }
}
// Helper to format Option<u64> with thousands separators
// Helper to format Option<u64> bytes into human-readable size (KB, MB, GB)
fn format_option_u64_bytes(opt: Option<u64>) -> String {
    match opt {
        Some(val) => humansize::format_size(val, humansize::DECIMAL), // Use humansize formatting
        None => "-".to_string(),
    }
}

// Helper to format Option<f64> speed in Bps to human-readable KB/s, MB/s etc.
fn format_speed_bps(speed_bps: Option<f64>) -> String {
    match speed_bps {
        Some(bps) if bps >= 0.0 => {
            // Use humansize for formatting, append "/s"
            format!("{}/s", format_size(bps as u64, DECIMAL))
        }
        _ => "-".to_string(), // Handle None or negative values (e.g., initial state)
    }
}

// Helper to create the formatted string for a list item with metrics data
fn create_list_item_text(name: &str, metrics: &NodeMetrics) -> String {
    // Calculate total errors
    let put_err = metrics.put_record_errors.unwrap_or(0);
    let conn_in_err = metrics.incoming_connection_errors.unwrap_or(0);
    let conn_out_err = metrics.outgoing_connection_errors.unwrap_or(0);
    let kad_err = metrics.kad_get_closest_peers_errors.unwrap_or(0);
    let total_errors = put_err + conn_in_err + conn_out_err + kad_err;

    // Format into a single line, adjust spacing as needed
    format!(
        "{:<20} | Up: {:<12} | Mem: {:<6} | CPU: {:<5}% | Peers: {:<3}/{:<3} | BW In/Out: {:<9}/{:<9} | Speed In/Out: {:<10}/{:<10} | Rec: {:<6} | Rew: {:<8} | Err: {:<4} | Status: Running",
        name,
        format_uptime(metrics.uptime_seconds),
        format_float(metrics.memory_used_mb, 1),
        format_float(metrics.cpu_usage_percentage, 1),
        format_option(metrics.connected_peers),
        format_option(metrics.peers_in_routing_table),
        format_option_u64_bytes(metrics.bandwidth_inbound_bytes),
        format_option_u64_bytes(metrics.bandwidth_outbound_bytes),
        format_speed_bps(metrics.speed_in_bps),
        format_speed_bps(metrics.speed_out_bps),
        format_option(metrics.records_stored),
        format_option(metrics.reward_wallet_balance),
        total_errors,
    )
}
