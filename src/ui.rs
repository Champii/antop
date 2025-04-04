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
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect}, // Added Margin
    style::{Color, Style},
    symbols,
    text::{Line, Span, Text}, // Added Line for Axis titles
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph}, // Added Chart widgets
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
    // Main vertical layout: Title, Content Area, Status
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1), // Title/Instructions
                Constraint::Min(0),    // Content Area (List + Chart)
                Constraint::Length(1), // Status/Last Update
            ]
            .as_ref(),
        )
        .split(f.area());

    // The middle content area (main_chunks[1]) will now be used entirely for the custom layout.

    let title = Paragraph::new(
        "Autonomi Node Dashboard | Press 'q' to quit | Use Up/Down keys to select node for chart",
    );
    f.render_widget(title, main_chunks[0]); // Use main_chunks[0]

    // Render the custom node rows in the main content area
    render_custom_node_rows(f, app, main_chunks[1]); // Use main_chunks[1]

    let status_text = format!(
        "Last update: {}s ago | {} servers",
        app.last_update.elapsed().as_secs(),
        app.servers.len()
    );
    let status = Paragraph::new(status_text).alignment(Alignment::Right); // Align status to the right
    f.render_widget(status, main_chunks[2]); // Use main_chunks[2]
}

fn render_custom_node_rows(f: &mut Frame, app: &mut App, area: Rect) {
    let outer_block = Block::default().borders(Borders::ALL).title("Nodes");
    f.render_widget(outer_block, area); // Render the block first to draw borders

    let inner_area = area.inner(Margin {
        // Use Margin directly, remove '&' and full path
        vertical: 1,
        horizontal: 1,
    }); // Area inside the block borders

    let num_servers = app.servers.len();
    if num_servers == 0 {
        // Handle case with no servers gracefully
        let no_servers_text = Paragraph::new("No servers discovered yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_servers_text, inner_area);
        return;
    }

    // Define constraints for each row. Each row gets equal height for now.
    // We might need a more dynamic height later if charts have variable height.
    let row_constraints = std::iter::repeat(Constraint::Ratio(1, num_servers as u32))
        .take(num_servers)
        .collect::<Vec<_>>();

    // Create a vertical layout for the rows within the inner area
    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner_area);

    // Iterate through servers and render each row
    for (i, (name, url)) in app.servers.iter().enumerate() {
        let row_area = row_chunks[i];

        // Split the row area horizontally: Text | Chart Placeholder
        let row_content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70), // Text area
                Constraint::Percentage(30), // Chart placeholder area
            ])
            .split(row_area);

        let text_area = row_content_chunks[0];
        let chart_placeholder_area = row_content_chunks[1];

        // Get metrics and determine style
        let metrics_result = app.metrics.get(url);
        let (text, style) = match metrics_result {
            Some(Ok(metrics)) => (
                create_list_item_text(name, metrics), // Use existing helper
                Style::default().fg(Color::Green),
            ),
            Some(Err(_)) => (
                format!("{:<20} | Status: Stopped", name),
                Style::default().fg(Color::Yellow),
            ),
            None => (
                format!("{:<20} | Status: Unknown", name),
                Style::default().fg(Color::DarkGray),
            ),
        };

        // Render the text part
        let text_paragraph = Paragraph::new(text).style(style);
        f.render_widget(text_paragraph, text_area);

        // --- Render the Speed In Chart (using available pre-calculated data) ---
        let chart_data = match metrics_result {
            Some(Ok(metrics)) => metrics.chart_data_in.as_deref(), // Get Option<&[(f64, f64)]>
            _ => None, // No data if metrics failed or are None
        };

        if let Some(data) = chart_data {
            if data.len() >= 2 {
                // Only render if we have at least 2 points for a line
                let datasets = vec![
                    Dataset::default()
                        .name("Speed In")
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Style::default().fg(Color::Green)) // Use green for inbound
                        .data(data),
                ];

                // Determine X-axis bounds based on the number of data points
                // Data is already (index, value), so index is 0 to len-1
                let x_bounds = [0.0, (data.len() - 1).max(1) as f64];

                // Determine Y-axis bounds dynamically
                let y_max = data.iter().map(|&(_, y)| y).fold(f64::NAN, f64::max);
                let y_bounds = [0.0, y_max.max(1.0)]; // Ensure at least 1.0 height, handle NaN

                let chart = Chart::new(datasets)
                    .block(
                        Block::default()
                            .title(Span::styled(
                                "Speed In (Bps)",
                                Style::default().fg(Color::White),
                            ))
                            .borders(Borders::LEFT),
                    )
                    .x_axis(
                        Axis::default()
                            // .title(Line::from("Time ->").style(Style::default().fg(Color::Gray))) // Title often clutters small charts
                            .style(Style::default().fg(Color::DarkGray))
                            .bounds(x_bounds),
                    )
                    .y_axis(
                        Axis::default()
                            .title(Line::from(" Bps").style(Style::default().fg(Color::Gray))) // Add space for alignment
                            .style(Style::default().fg(Color::DarkGray))
                            .bounds(y_bounds) // Use calculated Y bounds
                            .labels(vec![
                                Span::from("0"),                     // Start label
                                Span::from(format!("{:.0}", y_max)), // Max label
                            ]),
                    );

                f.render_widget(chart, chart_placeholder_area);
            } else {
                // Not enough data points, render placeholder
                let placeholder = Block::default().borders(Borders::LEFT).title(Span::styled(
                    "Speed In (no data)",
                    Style::default().fg(Color::DarkGray),
                ));
                f.render_widget(placeholder, chart_placeholder_area);
            }
        } else {
            // No metrics available (error or initial state), render placeholder
            let placeholder = Block::default().borders(Borders::LEFT).title(Span::styled(
                "Speed In (N/A)",
                Style::default().fg(Color::DarkGray),
            ));
            f.render_widget(placeholder, chart_placeholder_area);
        }
    }
} // End of render_custom_node_rows

// --- Chart Rendering Removed ---
// The render_speed_chart function has been removed as per the task requirements.
// The custom layout in render_custom_node_rows now provides space for charts.

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
    // Shorten the list item format slightly to fit better with the chart layout
    format!(
        "{:<18} | Up: {:<10} | Mem: {:<5} | CPU: {:<4}% | Peers: {:<2}/{:<2} | BW In: {:<8} | BW Out: {:<8} | Rec: {:<5} | Rew: {:<7} | Err: {:<3} | Running",
        name,
        format_uptime(metrics.uptime_seconds),
        format_float(metrics.memory_used_mb, 0), // Less precision for mem
        format_float(metrics.cpu_usage_percentage, 0), // Less precision for cpu
        format_option(metrics.connected_peers),
        format_option(metrics.peers_in_routing_table),
        format_option_u64_bytes(metrics.bandwidth_inbound_bytes),
        format_option_u64_bytes(metrics.bandwidth_outbound_bytes),
        // Speed is now shown in the chart
        format_option(metrics.records_stored),
        format_option(metrics.reward_wallet_balance),
        total_errors,
    )
}
