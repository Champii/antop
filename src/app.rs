use crate::metrics::{NodeMetrics, parse_metrics}; // Import necessary items from metrics module
use ratatui::widgets::TableState; // Import TableState for the struct field
use std::{collections::HashMap, time::Instant};

/// Holds the application state.
pub struct App {
    pub servers: Vec<(String, String)>, // Stores (server_name, server_url)
    // Store parsed metrics or error string directly, keyed by server_url
    pub metrics: HashMap<String, Result<NodeMetrics, String>>,
    pub previous_metrics: HashMap<String, NodeMetrics>, // Store previous metrics for speed calculation
    pub last_update: Instant,
    pub previous_update_time: Instant, // Store the time of the previous update
    pub table_state: TableState,       // To potentially handle scrolling later
}

impl App {
    /// Creates a new App instance with initial server list.
    pub fn new(servers: Vec<(String, String)>) -> App {
        let mut metrics_map = HashMap::new();
        let now = Instant::now();
        // Initialize metrics map with URLs as keys
        for (_name, url) in &servers {
            metrics_map.insert(url.clone(), Err("Fetching...".to_string()));
        }
        App {
            servers, // Assign the passed-in Vec<(String, String)>
            metrics: metrics_map,
            previous_metrics: HashMap::new(), // Initialize empty previous metrics
            last_update: now,                 // Initialize last_update time
            previous_update_time: now,        // Initialize previous update time
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
                    // Store the potentially updated metrics for the next cycle's "previous" state
                    next_previous_metrics.insert(addr.clone(), current_metrics.clone());
                    // Store the result for the current display state
                    new_metrics_map.insert(addr, Ok(current_metrics));
                }
                Err(e) => {
                    // Store the error string if fetching/parsing failed
                    new_metrics_map.insert(addr, Err(e));
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
