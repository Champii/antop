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
    text::{Line, Span}, // Added Line for Axis titles
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
    // let outer_block = Block::default().borders(Borders::ALL).title("Nodes");
    // f.render_widget(outer_block, area); // Render the block first

    let inner_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    }); // Area inside borders

    let num_servers = app.servers.len();
    if num_servers == 0 {
        let no_servers_text = Paragraph::new("No servers discovered yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_servers_text, inner_area);
        return;
    }

    // Layout: Header row + one row per server
    let mut constraints = vec![Constraint::Length(1)]; // Header row
    constraints.extend(std::iter::repeat(Constraint::Length(1)).take(num_servers)); // Data rows

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    // Render Header Titles into Columns
    let header_titles = [
        "Node", "Uptime", "Mem", "CPU", "Peers", "BW In", "BW Out", "Recs", "Rwds", "Err", "Status",
    ];
    let header_style = Style::default().fg(Color::Yellow);

    // Define column widths based on header comment (line 33) + Status
    // These must match the constraints used for data rows (lines 243-255)
    let column_constraints = [
        Constraint::Length(18), // Node
        Constraint::Length(12), // Uptime
        Constraint::Length(6),  // Mem MB
        Constraint::Length(5),  // CPU %
        Constraint::Length(7),  // Peers
        Constraint::Length(10), // BW In
        Constraint::Length(10), // BW Out
        Constraint::Length(7),  // Records
        Constraint::Length(8),  // Reward
        Constraint::Length(6),  // Err
        Constraint::Length(10), // Status
        Constraint::Min(0),     // Spacer to fill text_area
    ];

    // Split the header row area (vertical_chunks[0]) horizontally like the data rows (70% text, 30% chart placeholder)
    // This ensures the header titles align with the data columns below.
    let header_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Text area for header titles
            Constraint::Percentage(30), // Empty space corresponding to chart area
        ])
        .split(vertical_chunks[0]); // Split the entire header row area

    let header_text_area = header_row_chunks[0]; // The 70% area where titles will go

    // Split the header text area into columns using the defined constraints
    let header_column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(column_constraints.as_ref())
        .split(header_text_area);

    // Render each header title in its respective column chunk
    for (i, title) in header_titles.iter().enumerate() {
        // Ensure we only render titles into the actual title columns, not the spacer
        if i < header_column_chunks.len() - 1 {
            let title_paragraph = Paragraph::new(*title)
                .style(header_style)
                .alignment(Alignment::Left); // Align titles to the left within their columns
            f.render_widget(title_paragraph, header_column_chunks[i]);
        }
    }

    // Render Data Rows (starting from index 1)
    for (i, (name, url)) in app.servers.iter().enumerate() {
        if i + 1 >= vertical_chunks.len() {
            continue;
        } // Bounds check
        let row_area = vertical_chunks[i + 1]; // Use chunks starting from 1

        // Split row: Text | Chart Placeholder (remains the same)
        let row_content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70), // Text area
                Constraint::Percentage(30), // Chart placeholder area
            ])
            .split(row_area);

        let text_area = row_content_chunks[0];
        let chart_placeholder_area = row_content_chunks[1];

        // Get metrics and determine style (remains the same)
        let metrics_result = app.metrics.get(url);
        // Define column widths based on header comment (line 33) + Status
        let column_constraints = [
            Constraint::Length(18), // Node
            Constraint::Length(12), // Uptime
            Constraint::Length(6),  // Mem MB
            Constraint::Length(5),  // CPU %
            Constraint::Length(7),  // Peers
            Constraint::Length(10), // BW In
            Constraint::Length(10), // BW Out
            Constraint::Length(7),  // Records
            Constraint::Length(8),  // Reward
            Constraint::Length(6),  // Err
            Constraint::Length(10), // Status
            Constraint::Min(0),     // Spacer to fill text_area
        ];
        let column_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints.as_ref())
            .split(text_area);

        // Get metrics data vector and determine style + status
        let (data_cells, style, status_text) = match metrics_result {
            Some(Ok(metrics)) => (
                create_list_item_cells(name, metrics), // Use new helper returning Vec<String>
                Style::default().fg(Color::Green),
                "Running".to_string(),
            ),
            Some(Err(_)) => {
                // Create placeholder cells for error state
                let cells = vec![
                    format!("{:<18}", name), // Node
                    format!("{:<12}", "-"),  // Uptime
                    format!("{:<6}", "-"),   // Mem MB
                    format!("{:<5}", "-"),   // CPU %
                    format!("{:<7}", "-"),   // Peers
                    format!("{:<10}", "-"),  // BW In
                    format!("{:<10}", "-"),  // BW Out
                    format!("{:<7}", "-"),   // Records
                    format!("{:<8}", "-"),   // Reward
                    format!("{:<4}", "-"),   // Err
                ];
                (
                    cells,
                    Style::default().fg(Color::Yellow),
                    "Error".to_string(),
                ) // Changed status text
            }
            None => {
                // Create placeholder cells for unknown state
                let cells = vec![
                    format!("{:<18}", name), // Node
                    format!("{:<12}", "-"),  // Uptime
                    format!("{:<6}", "-"),   // Mem MB
                    format!("{:<5}", "-"),   // CPU %
                    format!("{:<7}", "-"),   // Peers
                    format!("{:<10}", "-"),  // BW In
                    format!("{:<10}", "-"),  // BW Out
                    format!("{:<7}", "-"),   // Records
                    format!("{:<8}", "-"),   // Reward
                    format!("{:<4}", "-"),   // Err
                ];
                (
                    cells,
                    Style::default().fg(Color::DarkGray),
                    "Unknown".to_string(),
                )
            }
        };

        // Render each data cell in its column chunk
        for (idx, cell_text) in data_cells.iter().enumerate() {
            if idx < column_chunks.len() - 1 {
                // Ensure we don't overwrite status column index
                let cell_paragraph = Paragraph::new(cell_text.clone()).style(style);
                f.render_widget(cell_paragraph, column_chunks[idx]);
            }
        }

        // Render status in the dedicated status column chunk (index 10)
        let status_paragraph = Paragraph::new(format!("{:<8}", status_text)).style(style); // Pad status
        f.render_widget(status_paragraph, column_chunks[10]);

        // Render the Speed In Chart (remains the same)
        let chart_data = match metrics_result {
            Some(Ok(metrics)) => metrics.chart_data_in.as_deref(),
            _ => None,
        };

        if let Some(data) = chart_data {
            if data.len() >= 2 {
                let datasets = vec![
                    Dataset::default()
                        .name("Speed In")
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Style::default().fg(Color::Cyan)) // Changed color for visibility
                        .data(data),
                ];
                let x_bounds = [0.0, (data.len() - 1).max(1) as f64];
                let y_max = data.iter().map(|&(_, y)| y).fold(f64::NAN, f64::max);
                let y_bounds = [0.0, y_max.max(1.0)];
                let chart = Chart::new(datasets)
                    .x_axis(
                        Axis::default()
                            .style(Style::default().fg(Color::DarkGray))
                            .bounds(x_bounds),
                    )
                    .y_axis(
                        Axis::default()
                            .title(Line::from(" Bps").style(Style::default().fg(Color::Gray)))
                            .style(Style::default().fg(Color::DarkGray))
                            .bounds(y_bounds)
                            .labels(vec![Span::from("0"), Span::from(format!("{:.0}", y_max))]),
                    );
                f.render_widget(chart, chart_placeholder_area);
            } else {
                let placeholder = Block::default().borders(Borders::LEFT).title(Span::styled(
                    "Speed In (no data)",
                    Style::default().fg(Color::DarkGray),
                ));
                f.render_widget(placeholder, chart_placeholder_area);
            }
        } else {
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

// Helper to format peer counts concisely
fn format_peers(connected: Option<u32>, total: Option<u32>) -> String {
    match (connected, total) {
        (Some(c), Some(t)) => format!("{}/{}", c, t),
        (Some(c), None) => format!("{}/-", c),
        (None, Some(t)) => format!("-/{}", t),
        (None, None) => "-".to_string(),
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

// Helper to create a vector of formatted data cell strings for a list item
fn create_list_item_cells(name: &str, metrics: &NodeMetrics) -> Vec<String> {
    // Calculate total errors
    let put_err = metrics.put_record_errors.unwrap_or(0);
    let conn_in_err = metrics.incoming_connection_errors.unwrap_or(0);
    let conn_out_err = metrics.outgoing_connection_errors.unwrap_or(0);
    let kad_err = metrics.kad_get_closest_peers_errors.unwrap_or(0);
    let total_errors = put_err + conn_in_err + conn_out_err + kad_err;

    // Format values according to HEADER widths and push to vector
    vec![
        format!("{:<18}", name),                                         // Node
        format!("{:<12}", format_uptime(metrics.uptime_seconds)),        // Uptime
        format!("{:<6}", format_float(metrics.memory_used_mb, 0)),       // Mem MB
        format!("{:<5}", format_float(metrics.cpu_usage_percentage, 0)), // CPU %
        format!(
            "{:<7}",
            format_peers(
                metrics.connected_peers.map(|v| v as u32),
                metrics.peers_in_routing_table.map(|v| v as u32)
            )
        ), // Peers
        format!("{:<10}", format_speed_bps(metrics.speed_in_bps)),       // BW In
        format!("{:<10}", format_speed_bps(metrics.speed_out_bps)),      // BW Out
        format!("{:<7}", format_option(metrics.records_stored)),         // Records
        format!("{:<8}", format_option(metrics.reward_wallet_balance)),  // Reward
        format!("{:<4}", total_errors),                                  // Err
                                                                         // Status is handled separately in render_custom_node_rows
    ]
}
