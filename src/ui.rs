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
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame, Terminal,
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

pub async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App, cli: &Cli) -> Result<()> {
    let mut tick_timer = interval(Duration::from_secs(1)); // Refresh data every second
    let mut discover_timer = interval(Duration::from_secs(60)); // Check for new servers every 60s

    // Initial fetch if servers were found initially
    if !app.server_addresses.is_empty() {
        let initial_results = fetch_metrics(&app.server_addresses).await;
        app.update_metrics(initial_results);
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?; // Draw UI

        // Combine timers and input polling
        tokio::select! {
            _ = tick_timer.tick() => {
                // Fetch metrics for current servers periodically
                if !app.server_addresses.is_empty() {
                    let results = fetch_metrics(&app.server_addresses).await;
                    app.update_metrics(results);
                }
            },
            _ = discover_timer.tick() => {
                 // Discover servers periodically
                match find_metrics_servers(cli.logs.as_deref()) {
                    Ok(found_servers) => {
                        // Simple update strategy: replace list and add new entries to map
                        // A more sophisticated approach might handle removed servers explicitly
                        let mut updated = false;
                        for server in &found_servers {
                            if !app.metrics.contains_key(server) {
                                app.metrics.insert(server.clone(), Err("Discovered - Fetching...".to_string()));
                                updated = true; // Mark that we added a new server
                            }
                        }
                        // Only update addresses if new ones were found or list differs
                        if updated || app.server_addresses != found_servers {
                             app.server_addresses = found_servers;
                             app.server_addresses.sort(); // Keep it sorted
                             app.server_addresses.dedup();
                             // Optional: Trigger an immediate fetch for newly discovered servers if needed
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
        .split(f.size());

    let title = Paragraph::new("Autonomi Node Dashboard | Press 'q' to quit");
    f.render_widget(title, chunks[0]);

    render_metrics_table(f, app, chunks[1]);

    let status_text = format!(
        "Last update: {}s ago | {} servers",
        app.last_update.elapsed().as_secs(),
        app.server_addresses.len()
    );
    let status = Paragraph::new(status_text); //.alignment(Alignment::Right); // Requires ratatui::layout::Alignment
    f.render_widget(status, chunks[2]);
}

fn render_metrics_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = [
        "Server", "Uptime", "Mem (MB)", "CPU (%)", "Peers", "RT Peers", "Net Size",
        "BW In (B)", "BW Out (B)", "Records", "PUT Err", "Rewards", "Conn Err In", "Conn Err Out", "Kad Err", "Status"
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    // Sort addresses for consistent display order
    // app.server_addresses.sort(); // Ensure addresses are sorted before rendering

    let rows = app.server_addresses.iter().map(|addr| {
        let metrics_result = app.metrics.get(addr);
        let (cells, row_style) = match metrics_result {
            Some(Ok(metrics)) => (create_metrics_cells(addr, metrics), Style::default()),
            Some(Err(e)) => (create_error_cells(addr, e), Style::default().fg(Color::Red)),
            None => (create_error_cells(addr, "Missing data"), Style::default().fg(Color::DarkGray)), // Should ideally not happen
        };
        Row::new(cells).style(row_style)
    });

    // Define constraints for each column - ensure this matches the number of headers/cells
    let constraints = [
        Constraint::Length(25), // Server Address
        Constraint::Length(10), // Uptime
        Constraint::Length(10), // Mem
        Constraint::Length(8),  // CPU
        Constraint::Length(8),  // Peers
        Constraint::Length(8),  // RT Peers
        Constraint::Length(10), // Net Size
        Constraint::Length(12), // BW In
        Constraint::Length(12), // BW Out
        Constraint::Length(10), // Records
        Constraint::Length(8),  // PUT Err
        Constraint::Length(10), // Rewards
        Constraint::Length(12), // Conn Err In
        Constraint::Length(12), // Conn Err Out
        Constraint::Length(8),  // Kad Err
        Constraint::Min(15),    // Status (flexible width)
    ];

    let table = Table::new(rows, &constraints) // Pass constraints reference
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Metrics"))
        .widths(&constraints); // Apply widths based on constraints

    // Use TableState for potential future scrolling
    f.render_stateful_widget(table, area, &mut app.table_state);
}

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


// Helper to create table cells for a row with valid metrics data
fn create_metrics_cells<'a>(addr: &'a str, metrics: &'a NodeMetrics) -> Vec<Cell<'a>> {
    vec![
        Cell::from(addr.to_string()), // Keep address as String for lifetime 'a
        Cell::from(format_uptime(metrics.uptime_seconds)),
        Cell::from(format_float(metrics.memory_used_mb, 1)),
        Cell::from(format_float(metrics.cpu_usage_percentage, 1)),
        Cell::from(format_option(metrics.connected_peers)),
        Cell::from(format_option(metrics.peers_in_routing_table)),
        Cell::from(format_option(metrics.estimated_network_size)),
        Cell::from(format_option(metrics.bandwidth_inbound_bytes)),
        Cell::from(format_option(metrics.bandwidth_outbound_bytes)),
        Cell::from(format_option(metrics.records_stored)),
        Cell::from(format_option(metrics.put_record_errors)),
        Cell::from(format_option(metrics.reward_wallet_balance)),
        Cell::from(format_option(metrics.incoming_connection_errors)),
        Cell::from(format_option(metrics.outgoing_connection_errors)),
        Cell::from(format_option(metrics.kad_get_closest_peers_errors)),
        Cell::from("OK").style(Style::default().fg(Color::Green)), // Status column
    ]
}

// Helper to create table cells for a row indicating an error state
fn create_error_cells<'a>(addr: &'a str, error_msg: &'a str) -> Vec<Cell<'a>> {
    let mut cells = vec![Cell::from(addr.to_string())];
    // Add placeholder cells for the metrics columns
    cells.extend(vec![Cell::from("-"); 14]); // 14 metric columns
    // Add the error message in the final 'Status' column
    cells.push(Cell::from(error_msg.to_string()).style(Style::default().fg(Color::Red)));
    cells
}