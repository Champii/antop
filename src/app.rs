use crate::metrics::{NodeMetrics, parse_metrics}; // Import necessary items from metrics module
use ratatui::widgets::TableState; // Import TableState for the struct field
use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
}; // Added VecDeque

// Number of data points to keep for sparklines
const SPARKLINE_HISTORY_LENGTH: usize = 60;

/// Holds the application state.
pub struct App {
    pub servers: Vec<(String, String)>, // Stores (server_name, server_url)
    // Store parsed metrics or error string directly, keyed by server_url
    pub metrics: HashMap<String, Result<NodeMetrics, String>>,
    pub previous_metrics: HashMap<String, NodeMetrics>, // Store previous metrics for speed calculation
    pub last_update: Instant,
    pub previous_update_time: Instant, // Store the time of the previous update
    pub speed_in_history: HashMap<String, VecDeque<u64>>, // History for Speed In sparkline
    pub speed_out_history: HashMap<String, VecDeque<u64>>, // History for Speed Out sparkline
    pub table_state: TableState,       // To potentially handle scrolling later
}

impl App {
    /// Creates a new App instance with initial server list.
    pub fn new(servers: Vec<(String, String)>) -> App {
        let mut metrics_map = HashMap::new();
        let now = Instant::now();
        let speed_in_history = HashMap::new(); // Initialize history maps
        let speed_out_history = HashMap::new();
        // Initialize metrics map with URLs as keys
        for (_name, url) in &servers {
            metrics_map.insert(url.clone(), Err("Fetching...".to_string()));
        }
        App {
            servers, // Assign the passed-in Vec<(String, String)>
            metrics: metrics_map,
            previous_metrics: HashMap::new(), // Initialize empty previous metrics
            last_update: now,                 // Initialize last_update time
            speed_in_history,                 // Add history maps to struct initialization
            speed_out_history,
            previous_update_time: now, // Initialize previous update time
            table_state: TableState::default(), // Initialize table state
        }
    }

    /// Updates metrics based on fetch results and calculates speeds.
    /// Takes results from fetch_metrics: Vec<(address, Result<raw_data, error_string>)>
    pub fn update_metrics(&mut self, results: Vec<(String, Result<String, String>)>) {
        let update_start_time = Instant::now();
        let delta_time = update_start_time
            .duration_since(self.previous_update_time)
            .as_secs_f64();

        let mut new_metrics_map = HashMap::new();
        let mut next_previous_metrics = HashMap::new();

        for (addr, result) in results {
            // Get or create history deques for this address
            let history_in = self
                .speed_in_history
                .entry(addr.clone())
                .or_insert_with(|| VecDeque::with_capacity(SPARKLINE_HISTORY_LENGTH));
            let history_out = self
                .speed_out_history
                .entry(addr.clone())
                .or_insert_with(|| VecDeque::with_capacity(SPARKLINE_HISTORY_LENGTH));

            match result {
                Ok(raw_data) => {
                    // Parse raw data into NodeMetrics
                    let mut current_metrics = parse_metrics(&raw_data);

                    // Calculate speed if possible
                    if let Some(prev_metrics) = self.previous_metrics.get(&addr) {
                        if delta_time > 0.0 {
                            // Calculate inbound speed
                            if let (Some(current_in), Some(prev_in)) = (
                                current_metrics.bandwidth_inbound_bytes,
                                prev_metrics.bandwidth_inbound_bytes,
                            ) {
                                if current_in >= prev_in {
                                    // Handle counter resets or initial states
                                    let delta_bytes = current_in - prev_in;
                                    current_metrics.speed_in_bps =
                                        Some(delta_bytes as f64 / delta_time);
                                } else {
                                    current_metrics.speed_in_bps = Some(0.0); // Reset detected or initial state
                                }
                            }

                            // Calculate outbound speed
                            if let (Some(current_out), Some(prev_out)) = (
                                current_metrics.bandwidth_outbound_bytes,
                                prev_metrics.bandwidth_outbound_bytes,
                            ) {
                                if current_out >= prev_out {
                                    // Handle counter resets
                                    let delta_bytes = current_out - prev_out;
                                    current_metrics.speed_out_bps =
                                        Some(delta_bytes as f64 / delta_time);
                                } else {
                                    current_metrics.speed_out_bps = Some(0.0); // Reset detected
                                }
                            }
                        }
                    }

                    // Add current speeds to history (use 0 if None or negative)
                    let speed_in_val = current_metrics.speed_in_bps.unwrap_or(0.0).max(0.0) as u64;
                    let speed_out_val =
                        current_metrics.speed_out_bps.unwrap_or(0.0).max(0.0) as u64;

                    history_in.push_back(speed_in_val);
                    history_out.push_back(speed_out_val);

                    // Trim history if it exceeds the desired length
                    if history_in.len() > SPARKLINE_HISTORY_LENGTH {
                        history_in.pop_front();
                    }
                    if history_out.len() > SPARKLINE_HISTORY_LENGTH {
                        history_out.pop_front();
                    }

                    // Store the potentially updated metrics for the next cycle's "previous" state
                    next_previous_metrics.insert(addr.clone(), current_metrics.clone());
                    // Store the result for the current display state
                    new_metrics_map.insert(addr, Ok(current_metrics));
                }
                Err(e) => {
                    // Fetching failed, store error and add 0 to speed history
                    new_metrics_map.insert(addr.clone(), Err(e)); // Use addr.clone() here too
                    history_in.push_back(0);
                    history_out.push_back(0);

                    // Trim history
                    if history_in.len() > SPARKLINE_HISTORY_LENGTH {
                        history_in.pop_front();
                    }
                    if history_out.len() > SPARKLINE_HISTORY_LENGTH {
                        history_out.pop_front();
                    }
                }
            }
        }

        // Update the application state
        self.previous_metrics = next_previous_metrics;
        self.previous_update_time = self.last_update; // The 'last_update' before this cycle becomes the 'previous' for the next
        self.metrics = new_metrics_map;
        self.last_update = update_start_time; // Record the time this update cycle started
    }
}
