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
    symbols,            // Added for chart markers
    text::{Span, Text}, // Removed unused 'Line'
    widgets::{
        Axis,
        Block,
        Borders,
        Chart,
        Dataset,
        GraphType,
        LegendPosition,
        List,
        ListItem, // Removed unused 'ListState'
        Paragraph,
    }, // Added Chart, Dataset, Axis, GraphType, LegendPosition
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

    // Split the middle content area horizontally: List on left, Chart on right
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(65), // List takes 65%
                Constraint::Percentage(35), // Chart takes 35%
            ]
            .as_ref(),
        )
        .split(main_chunks[1]); // Split the middle chunk (index 1)

    let title = Paragraph::new(
        "Autonomi Node Dashboard | Press 'q' to quit | Use Up/Down keys to select node for chart",
    );
    f.render_widget(title, main_chunks[0]); // Use main_chunks[0]

    // Render the list in the left content area
    render_metrics_list(f, app, content_chunks[0]); // Use content_chunks[0]

    // Render the speed chart in the right content area
    render_speed_chart(f, app, content_chunks[1]); // Use content_chunks[1]

    let status_text = format!(
        "Last update: {}s ago | {} servers",
        app.last_update.elapsed().as_secs(),
        app.servers.len()
    );
    let status = Paragraph::new(status_text).alignment(Alignment::Right); // Align status to the right
    f.render_widget(status, main_chunks[2]); // Use main_chunks[2]
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

    // Handle list navigation *before* creating the List widget to avoid borrow errors
    let num_items = items.len(); // Get length before moving items
    if let Some(selected) = app.list_state.selected() {
        if selected >= num_items {
            // If selection is out of bounds (e.g. list shrunk), reset selection
            app.list_state
                .select(if num_items > 0 { Some(0) } else { None }); // Select first or none
        }
    } else if num_items > 0 {
        app.list_state.select(Some(0)); // Select first item if nothing is selected
    }

    // Create the List widget
    let list = List::new(items) // Now 'items' is moved here
        .block(list_block)
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White)) // Basic highlight style
        .highlight_symbol("> "); // Symbol for selected item

    // Render the list using ListState
    f.render_stateful_widget(list, area, &mut app.list_state);
} // End of render_metrics_list

// --- Speed Chart Rendering ---

fn render_speed_chart(f: &mut Frame, app: &mut App, area: Rect) {
    let selected_index = app.list_state.selected();

    // Initialize chart variables before the conditional logic
    let mut chart_title = "Speed History: Select a node".to_string();
    // This Vec needs a lifetime annotation because Dataset borrows data.
    // We'll create the actual data and datasets inside the 'if let' block.
    let mut datasets_to_render: Vec<Dataset<'_>> = vec![];
    let mut max_speed = 0.0; // Initialize max_speed

    if let Some(index) = selected_index {
        if let Some((name, url)) = app.servers.get(index) {
            // Data exists for the selected server, access metrics via URL
            if let Some(Ok(metrics)) = app.metrics.get(url) {
                chart_title = format!("Speed History: {}", name);
                // Use the pre-calculated chart data from app state
                if let (Some(chart_data_in), Some(chart_data_out)) =
                    (&metrics.chart_data_in, &metrics.chart_data_out)
                {
                    // Calculate max speed from the chart data stored in app state
                    max_speed = chart_data_in
                        .iter()
                        .chain(chart_data_out.iter())
                        .map(|&(_, y)| y)
                        .fold(0.0, f64::max);

                    // Create datasets borrowing data directly from app state (metrics)
                    // The borrow checker is happy because `metrics` (and thus the chart data)
                    // lives as long as `app`, which is longer than the chart rendering.
                    datasets_to_render = vec![
                        Dataset::default()
                            .name("In (B/s)")
                            .marker(symbols::Marker::Braille)
                            .graph_type(GraphType::Line)
                            .style(Style::default().fg(Color::Cyan))
                            .data(chart_data_in), // Borrow from app state
                        Dataset::default()
                            .name("Out (B/s)")
                            .marker(symbols::Marker::Braille)
                            .graph_type(GraphType::Line)
                            .style(Style::default().fg(Color::Magenta))
                            .data(chart_data_out), // Borrow from app state
                    ];
                } else {
                    // Handle case where chart data hasn't been populated yet in metrics
                    chart_title = format!("Speed History: {} (Calculating...)", name);
                    // datasets_to_render remains empty
                }
            } else {
                // Handle case where metrics are Error or None for the selected server's URL
                chart_title = format!("Speed History: {} (No Data)", name);
                // datasets_to_render remains empty
            }
        } else {
            // Index out of bounds, keep defaults but set error title
            chart_title = "Speed History: Error".to_string();
            // datasets_to_render remains empty, max_speed remains 0.0
        }
    } // else: No server selected, keep defaults:
    // chart_title = "Speed History: Select a node"
    // datasets_to_render remains empty, max_speed remains 0.0

    // The tuple is now correctly destructured above, so these lines are redundant
    // let chart_title = datasets.0; // Removed
    // let chart_datasets = datasets.1; // Removed - use 'datasets' directly now
    // let max_speed = datasets.2; // Removed

    // Determine Y-axis bounds and labels based on max_speed (already destructured)
    // Determine Y-axis bounds and labels based on max_speed (already calculated)
    let (y_max, y_label_suffix) = if datasets_to_render.is_empty() || max_speed <= 1.0_f64 {
        (1.0_f64, " B/s") // Avoid 0 bound, show B/s if no data or very low speed
    } else {
        let max_val = max_speed; // Use max_speed directly (already f64)
        if max_val < 1_000.0_f64 {
            (max_val.ceil(), " B/s") // ceil() works on f64
        } else if max_val < 1_000_000.0_f64 {
            ((max_val / 1_000.0_f64).ceil() * 1_000.0_f64, " KB/s")
        } else if max_val < 1_000_000_000.0_f64 {
            (
                (max_val / 1_000_000.0_f64).ceil() * 1_000_000.0_f64,
                " MB/s",
            )
        } else {
            (
                (max_val / 1_000_000_000.0_f64).ceil() * 1_000_000_000.0_f64,
                " GB/s",
            )
        }
    };

    let y_labels: Vec<Span> = (0..=3) // Generate 4 labels (0%, 33%, 66%, 100% of max)
        .map(|i| {
            let val = y_max * (i as f64 / 3.0);
            let formatted_val = if y_label_suffix == " B/s" {
                format!("{:.0}", val)
            } else if y_label_suffix == " KB/s" {
                format!("{:.1}", val / 1_000.0)
            } else if y_label_suffix == " MB/s" {
                format!("{:.1}", val / 1_000_000.0)
            } else {
                // GB/s
                format!("{:.1}", val / 1_000_000_000.0)
            };
            Span::raw(format!("{}{}", formatted_val, y_label_suffix))
        })
        .collect();

    let chart = Chart::new(datasets_to_render) // Use the datasets created/assigned above
        .block(
            Block::default()
                .title(Span::styled(
                    chart_title, // Use chart_title directly
                    Style::default().fg(Color::White),
                ))
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Time (updates)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, crate::app::SPARKLINE_HISTORY_LENGTH as f64]), // X-axis from 0 to history length
        )
        .y_axis(
            Axis::default()
                .title(format!("Speed ({})", y_label_suffix.trim()))
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, y_max]) // Y-axis from 0 to calculated max
                .labels(y_labels), // Use generated labels
        )
        .legend_position(Some(LegendPosition::TopRight)); // Show legend (Remove non-existent legend_style)

    f.render_widget(chart, area);
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
