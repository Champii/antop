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
    text::Text,
    widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table}, // Added Sparkline
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

    render_metrics_table(f, app, chunks[1]);

    let status_text = format!(
        "Last update: {}s ago | {} servers",
        app.last_update.elapsed().as_secs(),
        app.servers.len()
    );
    let status = Paragraph::new(status_text); //.alignment(Alignment::Right); // Requires ratatui::layout::Alignment
    f.render_widget(status, chunks[2]);
}

// Helper function to calculate column offsets based on constraints
fn get_column_offsets(constraints: &[Constraint]) -> Vec<u16> {
    let mut offsets = Vec::with_capacity(constraints.len() + 1);
    let mut current_offset = 0;
    offsets.push(current_offset);
    for constraint in constraints {
        // This is a simplification; assumes fixed Length constraints for calculating
        // sparkline positions. More robust calculation might be needed for complex layouts.
        let width = match *constraint {
            Constraint::Length(w) | Constraint::Min(w) | Constraint::Max(w) => w, // Approximate width
            _ => 10, // Default guess for Percent/Ratio etc.
        };
        current_offset += width;
        offsets.push(current_offset);
    }
    offsets
}

fn render_metrics_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = [
        "Server",
        "Uptime",
        "Mem",
        "CPU",
        "Peers",
        "RT Peers",
        "BW In",
        "BW Out",
        "Speed In",
        "Speed Out",
        "Records",
        "Rewards",
        "Err",    // New Error Sum column
        "Status", // Restored Status column
    ]
    .iter()
    .map(|h| {
        Cell::from(
            Text::from(*h)
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::Yellow)),
        )
    });
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    // Iterate over (name, url) pairs
    let rows = app.servers.iter().map(|(name, url)| {
        let metrics_result = app.metrics.get(url); // Use URL as key for metrics
        let (cells, row_style) = match metrics_result {
            Some(Ok(metrics)) => (create_metrics_cells(name, metrics), Style::default()), // Pass name
            Some(Err(e)) => (
                // Display "Stopped" even on fetch error, not the error message itself
                create_error_cells(name, "Stopped"),
                Style::default().fg(Color::Yellow),
            ),
            None => (
                create_error_cells(name, "Stopped"), // More descriptive text
                Style::default().fg(Color::Yellow),  // Yellow for Stopped/Missing row
            ), // Pass name
        };
        Row::new(cells).style(row_style)
    });

    // Define constraints for each column - ensure this matches the number of headers/cells
    let constraints = [
        Constraint::Length(20), // Server Name
        Constraint::Length(10), // Uptime
        Constraint::Length(10), // Mem
        Constraint::Length(8),  // CPU
        Constraint::Length(8),  // Peers
        Constraint::Length(8),  // RT Peers
        Constraint::Length(12), // BW In
        Constraint::Length(12), // BW Out
        Constraint::Length(25), // Speed In
        Constraint::Length(25), // Speed Out
        Constraint::Length(10), // Records
        Constraint::Length(10), // Rewards
        Constraint::Length(8),  // Err (New)
        Constraint::Min(10),    // Status (Restored, flexible width)
    ];

    let table_block = Block::default().borders(Borders::ALL).title("Metrics");
    let table_inner_area = table_block.inner(area); // Area inside the block borders

    let table = Table::new(rows, &constraints) // Pass constraints reference
        .header(header)
        .block(table_block) // Use the defined block
        .widths(&constraints)
        .flex(ratatui::layout::Flex::Center);

    // Use TableState for potential future scrolling
    f.render_stateful_widget(table, area, &mut app.table_state);

    // --- Render Sparklines Over the Table ---
    // This needs to happen *after* the table is rendered.

    let header_height = 2; // Calculated from header row definition (height 1 + margin 1)
    let col_offsets = get_column_offsets(&constraints);
    let speed_in_col_index = 8; // Index of "Speed In"
    let speed_out_col_index = 9; // Index of "Speed Out"

    // Ensure we have offsets for the required columns
    if col_offsets.len() > speed_out_col_index + 1 {
        let speed_in_x = table_inner_area.x + col_offsets[speed_in_col_index];
        let speed_in_width = col_offsets[speed_in_col_index + 1] - col_offsets[speed_in_col_index];
        let speed_out_x = table_inner_area.x + col_offsets[speed_out_col_index];
        let speed_out_width =
            col_offsets[speed_out_col_index + 1] - col_offsets[speed_out_col_index];

        for (row_index, (_name, url)) in app.servers.iter().enumerate() {
            let row_y = table_inner_area.y + header_height + row_index as u16;

            // Check if row is within the visible table area
            if row_y >= table_inner_area.bottom() {
                break; // Stop drawing if rows go beyond the visible area
            }

            // Get history data, provide empty slice if not found
            let history_in = app
                .speed_in_history
                .get(url)
                .map(|d| d.as_slices().0)
                .unwrap_or(&[]);
            let history_out = app
                .speed_out_history
                .get(url)
                .map(|d| d.as_slices().0)
                .unwrap_or(&[]);

            // Create Sparkline widgets
            let sparkline_in = Sparkline::default()
                .data(history_in)
                .style(Style::default().fg(Color::LightCyan));
            let sparkline_out = Sparkline::default()
                .data(history_out)
                .style(Style::default().fg(Color::LightMagenta));

            // Define the Rect for each sparkline
            let sparkline_in_area = Rect::new(speed_in_x, row_y, speed_in_width, 1);
            let sparkline_out_area = Rect::new(speed_out_x, row_y, speed_out_width, 1);

            // Render the sparklines directly onto the frame in the calculated areas
            f.render_widget(sparkline_in, sparkline_in_area);
            f.render_widget(sparkline_out, sparkline_out_area);
        }
    }
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

// Helper to create table cells for a row with valid metrics data
fn create_metrics_cells<'a>(name: &'a str, metrics: &'a NodeMetrics) -> Vec<Cell<'a>> {
    // Accept name instead of addr

    // Calculate total errors for the "Err" column
    let put_err = metrics.put_record_errors.unwrap_or(0);
    let conn_in_err = metrics.incoming_connection_errors.unwrap_or(0);
    let conn_out_err = metrics.outgoing_connection_errors.unwrap_or(0);
    let kad_err = metrics.kad_get_closest_peers_errors.unwrap_or(0);
    let total_errors = put_err + conn_in_err + conn_out_err + kad_err;

    // Determine Status text and style - always "Running" if metrics are present
    let status_text = "Running";
    let status_style = Style::default().fg(Color::Green);

    vec![
        right_aligned_cell(name.to_string()), // Display server name
        right_aligned_cell(format_uptime(metrics.uptime_seconds)),
        right_aligned_cell(format_float(metrics.memory_used_mb, 1)),
        right_aligned_cell(format_float(metrics.cpu_usage_percentage, 1)),
        right_aligned_cell(format_option(metrics.connected_peers)),
        right_aligned_cell(format_option(metrics.peers_in_routing_table)),
        right_aligned_cell(format_option_u64_bytes(metrics.bandwidth_inbound_bytes)),
        right_aligned_cell(format_option_u64_bytes(metrics.bandwidth_outbound_bytes)),
        right_aligned_cell(format_speed_bps(metrics.speed_in_bps)), // New Speed In cell
        right_aligned_cell(format_speed_bps(metrics.speed_out_bps)), // New Speed Out cell
        right_aligned_cell(format_option(metrics.records_stored)),
        // Removed: right_aligned_cell(format_option(metrics.put_record_errors)),
        right_aligned_cell(format_option(metrics.reward_wallet_balance)),
        // Removed: right_aligned_cell(format_option(metrics.incoming_connection_errors)),
        // Removed: right_aligned_cell(format_option(metrics.outgoing_connection_errors)),
        // Removed: right_aligned_cell(format_option(metrics.kad_get_closest_peers_errors)),
        right_aligned_cell(total_errors.to_string()), // New "Err" column showing sum
        right_aligned_cell(status_text.to_string()).style(status_style), // Restored "Status" column
    ]
}

// Helper to create table cells for a row indicating an error state (e.g., fetch failed)
fn create_error_cells<'a>(name: &'a str, error_msg: &'a str) -> Vec<Cell<'a>> {
    // Accept name instead of addr
    let mut cells = vec![right_aligned_cell(name.to_string())]; // Display server name (right-aligned)
    // Add placeholder cells for the metrics columns (11 columns: Uptime to Rewards)
    cells.extend(vec![right_aligned_cell("-".to_string()); 11]);
    // Add placeholder for the "Err" column
    cells.push(right_aligned_cell("-".to_string()));
    // Add the error message in the "Status" column
    // Use Yellow for Stopped/Error fetching status cell
    cells.push(right_aligned_cell(error_msg.to_string()).style(Style::default().fg(Color::Yellow)));
    cells
}

fn right_aligned_cell(text: String) -> Cell<'static> {
    Cell::from(Text::from(text).alignment(Alignment::Right))
}
