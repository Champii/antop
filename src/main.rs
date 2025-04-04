use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::future::join_all;
use glob::glob;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};
use regex::Regex;
use reqwest;
use std::{
    collections::HashMap,
    fs,
    io::{self, Stdout},
    path::PathBuf,
    str::FromStr,
    time::{Duration, Instant},
};
use tokio::time::interval;

// --- Existing Metrics Struct and Parsing Logic ---

/// Structure to hold parsed metrics from an antnode.
#[derive(Debug, Default, Clone)]
struct NodeMetrics {
    uptime_seconds: Option<u64>,
    memory_used_mb: Option<f64>,
    cpu_usage_percentage: Option<f64>,
    connected_peers: Option<u64>,
    peers_in_routing_table: Option<u64>,
    estimated_network_size: Option<u64>,
    bandwidth_inbound_bytes: Option<u64>,
    bandwidth_outbound_bytes: Option<u64>,
    records_stored: Option<u64>,
    put_record_errors: Option<u64>,
    reward_wallet_balance: Option<u64>, // Assuming integer units
    incoming_connection_errors: Option<u64>,
    outgoing_connection_errors: Option<u64>,
    kad_get_closest_peers_errors: Option<u64>,
}

/// Parses the raw metrics text into a NodeMetrics struct.
fn parse_metrics(metrics_data: &str) -> NodeMetrics {
    let mut metrics = NodeMetrics::default();
    let mut outgoing_connection_errors_sum: u64 = 0;
    let mut incoming_connection_errors_sum: u64 = 0;
    let mut kad_get_closest_peers_errors_sum: u64 = 0;

    for line in metrics_data.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue; // Skip comments and empty lines
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue; // Skip lines without at least a name and value
        }

        let metric_name = parts[0];
        let value_str = parts[parts.len() - 1]; // Value is usually the last part

        // Generic helper function to parse value
        fn parse_value<T: FromStr>(s: &str) -> Option<T> {
            s.parse::<T>().ok()
        }

        match metric_name {
            "ant_node_uptime" => metrics.uptime_seconds = parse_value::<u64>(value_str),
            "ant_networking_process_memory_used_mb" => metrics.memory_used_mb = parse_value::<f64>(value_str),
            "ant_networking_process_cpu_usage_percentage" => metrics.cpu_usage_percentage = parse_value::<f64>(value_str),
            "ant_networking_connected_peers" => metrics.connected_peers = parse_value::<u64>(value_str),
            "ant_networking_peers_in_routing_table" => metrics.peers_in_routing_table = parse_value::<u64>(value_str),
            "ant_networking_estimated_network_size" => metrics.estimated_network_size = parse_value::<u64>(value_str),
            "ant_networking_records_stored" => metrics.records_stored = parse_value::<u64>(value_str),
            "ant_node_put_record_err_total" => metrics.put_record_errors = parse_value::<u64>(value_str),
            "ant_node_current_reward_wallet_balance" => metrics.reward_wallet_balance = parse_value::<u64>(value_str),
            // Handle metrics with labels
            name if name.starts_with("libp2p_bandwidth_bytes_total") => {
                if line.contains(r#"direction="Inbound""#) {
                    metrics.bandwidth_inbound_bytes = parse_value::<u64>(value_str);
                } else if line.contains(r#"direction="Outbound""#) {
                    metrics.bandwidth_outbound_bytes = parse_value::<u64>(value_str);
                }
            }
            name if name.starts_with("libp2p_swarm_connections_incoming_error_total") => {
                if let Some(val) = parse_value::<u64>(value_str) {
                    incoming_connection_errors_sum += val;
                }
            }
            name if name.starts_with("libp2p_swarm_outgoing_connection_error_total") => {
                if let Some(val) = parse_value::<u64>(value_str) {
                    outgoing_connection_errors_sum += val;
                }
            }
            name if name.starts_with("libp2p_kad_query_result_get_closest_peers_error_total") => {
                 if let Some(val) = parse_value::<u64>(value_str) {
                    kad_get_closest_peers_errors_sum += val;
                }
            }
            _ => {} // Ignore other metrics
        }
    }

    // Assign summed errors if they were found or the metric name exists at all
    if incoming_connection_errors_sum > 0 || metrics_data.contains("libp2p_swarm_connections_incoming_error_total") {
         metrics.incoming_connection_errors = Some(incoming_connection_errors_sum);
    }
     if outgoing_connection_errors_sum > 0 || metrics_data.contains("libp2p_swarm_outgoing_connection_error_total") {
        metrics.outgoing_connection_errors = Some(outgoing_connection_errors_sum);
    }
    if kad_get_closest_peers_errors_sum > 0 || metrics_data.contains("libp2p_kad_query_result_get_closest_peers_error_total") {
        metrics.kad_get_closest_peers_errors = Some(kad_get_closest_peers_errors_sum);
    }

    metrics
}

// --- Existing Server Discovery and Fetching Logic ---

/// Finds antnode log files and extracts the last reported metrics server address from each.
fn find_metrics_servers() -> Result<Vec<String>> {
    let home_dir = dirs::home_dir().context("Failed to get home directory")?;
    let pattern_str = home_dir
        .join(".local/share/autonomi/node/*/logs/antnode.log")
        .to_string_lossy()
        .to_string();

    let re = Regex::new(r"Metrics server on (\S+)")?;
    let mut addresses = Vec::new();

    // Use glob to find matching log files
    for entry in glob(&pattern_str).context("Failed to read glob pattern")? {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    match process_log_file(&path, &re) {
                        Ok(Some(address)) => {
                            addresses.push(address);
                        }
                        Ok(None) => {} // Ignore logs without the address
                        Err(e) => {
                            eprintln!("Error processing log file {:?}: {}", path, e); // Log error but continue
                        }
                    }
                }
            }
            Err(e) => eprintln!("Error processing glob entry: {}", e), // Log error but continue
        }
    }

    addresses.sort();
    addresses.dedup();
    Ok(addresses)
}

/// Reads a single log file and extracts the last metrics server address.
fn process_log_file(path: &PathBuf, re: &Regex) -> Result<Option<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read log file: {:?}", path))?;
    let mut last_match: Option<String> = None;
    for line in content.lines() {
        if let Some(caps) = re.captures(line) {
            if let Some(address) = caps.get(1) {
                last_match = Some(address.as_str().to_string());
            }
        }
    }
    Ok(last_match)
}

/// Fetches metrics data from a list of server addresses concurrently.
async fn fetch_metrics(
    addresses: &[String],
) -> Vec<(String, Result<String, String>)> { // Return String error for simplicity in App state
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2)) // Shorter timeout for TUI responsiveness
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let futures = addresses.iter().map(|addr| {
        let client = client.clone();
        let addr = addr.clone();
        async move {
            let url = format!("{}/metrics", addr);
            let result = client.get(&url).send().await;

            match result {
                Ok(response) => match response.error_for_status() {
                    Ok(successful_response) => match successful_response.text().await {
                        Ok(text) => (addr, Ok(text)),
                        Err(e) => (addr, Err(format!("Read body error: {}", e))),
                    },
                    Err(status_error) => (addr, Err(format!("HTTP error: {}", status_error))),
                },
                Err(network_error) => (addr, Err(format!("Network error: {}", network_error))),
            }
        }
    });

    join_all(futures).await
}

// --- TUI Application State and Logic ---

/// Holds the application state.
struct App {
    server_addresses: Vec<String>,
    metrics: HashMap<String, Result<NodeMetrics, String>>, // Store parsed metrics or error string
    last_update: Instant,
    table_state: TableState, // To potentially handle scrolling later
}

impl App {
    fn new(servers: Vec<String>) -> App {
        let mut metrics_map = HashMap::new();
        for server in &servers {
            metrics_map.insert(server.clone(), Err("Fetching...".to_string())); // Initial state
        }
        App {
            server_addresses: servers,
            metrics: metrics_map,
            last_update: Instant::now(),
            table_state: TableState::default(),
        }
    }

    /// Update metrics based on fetch results.
    fn update_metrics(&mut self, results: Vec<(String, Result<String, String>)>) {
        for (addr, result) in results {
            match result {
                Ok(raw_data) => {
                    let parsed_metrics = parse_metrics(&raw_data);
                    self.metrics.insert(addr, Ok(parsed_metrics));
                }
                Err(e) => {
                    self.metrics.insert(addr, Err(e)); // Store the error string
                }
            }
        }
        self.last_update = Instant::now();
    }
}

// --- TUI Setup and Restore ---

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("Failed to create terminal")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
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

#[tokio::main]
async fn main() -> Result<()> {
    // Initial server discovery
    let initial_servers = match find_metrics_servers() {
        Ok(servers) => {
            if servers.is_empty() {
                eprintln!("Warning: No metrics servers found in logs. Waiting for discovery or manual config.");
                // Proceed with empty list, maybe fetch later or show message
                Vec::new()
            } else {
                println!("Found {} metrics servers.", servers.len()); // Log to console before TUI starts
                servers
            }
        }
        Err(e) => {
            eprintln!("Error finding metrics servers: {}. Exiting.", e);
            return Err(e);
        }
    };

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Create app and run the main loop
    let app = App::new(initial_servers);
    let res = run_app(&mut terminal, app).await;

    // Restore terminal
    restore_terminal(&mut terminal)?;

    if let Err(err) = res {
        println!("Error running app: {:?}", err);
        return Err(err);
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    let mut tick_timer = interval(Duration::from_secs(1));
    let mut discover_timer = interval(Duration::from_secs(60)); // Check for new servers every 60s

    // Initial fetch
    if !app.server_addresses.is_empty() {
        let initial_results = fetch_metrics(&app.server_addresses).await;
        app.update_metrics(initial_results);
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?; // Draw UI

        // Combine timers and input polling
        tokio::select! {
            _ = tick_timer.tick() => {
                // Fetch metrics for current servers
                if !app.server_addresses.is_empty() {
                    let results = fetch_metrics(&app.server_addresses).await;
                    app.update_metrics(results);
                }
            },
            _ = discover_timer.tick() => {
                 // Discover servers periodically
                match find_metrics_servers() {
                    Ok(found_servers) => {
                        // Simple update: replace list and add new entries to map
                        // More sophisticated logic could handle removed servers
                        for server in &found_servers {
                            if !app.metrics.contains_key(server) {
                                app.metrics.insert(server.clone(), Err("Discovered - Fetching...".to_string()));
                            }
                        }
                        app.server_addresses = found_servers;
                        app.server_addresses.sort(); // Keep it sorted
                        app.server_addresses.dedup();
                    }
                    Err(e) => {
                        // Log error, maybe display in UI later
                        eprintln!("Error re-discovering metrics servers: {}", e);
                    }
                }
            },
            // Poll for keyboard events with a small timeout
            result = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(100))) => {
                if let Ok(true) = result? { // Check if poll returned Ok(true) indicating an event
                    if let Event::Key(key) = event::read()? {
                        if key.code == KeyCode::Char('q') {
                            return Ok(()); // Quit
                        }
                        // Add other key handlers here if needed (e.g., scrolling)
                    }
                }
                // If poll returned Ok(false) or Err, just continue the loop
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

    let rows = app.server_addresses.iter().map(|addr| {
        let metrics_result = app.metrics.get(addr);
        let (cells, row_style) = match metrics_result {
            Some(Ok(metrics)) => (create_metrics_cells(addr, metrics), Style::default()),
            Some(Err(e)) => (create_error_cells(addr, e), Style::default().fg(Color::Red)),
            None => (create_error_cells(addr, "Missing data"), Style::default().fg(Color::DarkGray)), // Should not happen with current logic
        };
        Row::new(cells).style(row_style)
    });

    // Define constraints for each column
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
        Constraint::Min(15),    // Status (flexible)
    ];

    let table = Table::new(rows, constraints)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Metrics"))
        .widths(&constraints); // Ensure constraints are applied

    // Use TableState for potential future scrolling
    f.render_stateful_widget(table, area, &mut app.table_state);
}

// Helper to format Option<T> for display
fn format_option<T: std::fmt::Display>(opt: Option<T>) -> String {
    match opt {
        Some(val) => val.to_string(),
        None => "-".to_string(),
    }
}

// Helper to format uptime
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

// Helper to format Option<f64> with precision
fn format_float(opt: Option<f64>, precision: usize) -> String {
    match opt {
        Some(val) => format!("{:.prec$}", val, prec = precision),
        None => "-".to_string(),
    }
}


// Helper to create cells for a row with valid metrics
fn create_metrics_cells<'a>(addr: &'a str, metrics: &'a NodeMetrics) -> Vec<Cell<'a>> {
    vec![
        Cell::from(addr.to_string()),
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
        Cell::from("OK"), // Status column
    ]
}

// Helper to create cells for a row with an error
fn create_error_cells<'a>(addr: &'a str, error_msg: &'a str) -> Vec<Cell<'a>> {
    // Create a vector with the address, the error message spanning most columns, and empty cells
    let mut cells = vec![Cell::from(addr.to_string())];
    // Add empty cells for metric columns
    cells.extend(vec![Cell::from("-"); 14]); // 14 metric columns
    // Add the error message in the last column
    cells.push(Cell::from(error_msg.to_string()));
    cells
}
