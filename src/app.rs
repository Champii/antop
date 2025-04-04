use crate::metrics::{parse_metrics, NodeMetrics}; // Import necessary items from metrics module
use ratatui::widgets::TableState; // Import TableState for the struct field
use std::{collections::HashMap, time::Instant};

/// Holds the application state.
pub struct App {
    pub servers: Vec<(String, String)>, // Stores (server_name, server_url)
    // Store parsed metrics or error string directly, keyed by server_url
    pub metrics: HashMap<String, Result<NodeMetrics, String>>,
    pub last_update: Instant,
    pub table_state: TableState, // To potentially handle scrolling later
}

impl App {
    /// Creates a new App instance with initial server list.
    pub fn new(servers: Vec<(String, String)>) -> App {
        let mut metrics_map = HashMap::new();
        // Initialize metrics map with URLs as keys
        for (_name, url) in &servers {
            metrics_map.insert(url.clone(), Err("Fetching...".to_string()));
        }
        App {
            servers, // Assign the passed-in Vec<(String, String)>
            metrics: metrics_map,
            last_update: Instant::now(), // Initialize last_update time
            table_state: TableState::default(), // Initialize table state
        }
    }

    /// Updates metrics based on fetch results.
    /// Takes results from fetch_metrics: Vec<(address, Result<raw_data, error_string>)>
    pub fn update_metrics(&mut self, results: Vec<(String, Result<String, String>)>) {
        for (addr, result) in results {
            match result {
                Ok(raw_data) => {
                    // Parse raw data into NodeMetrics
                    let parsed_metrics = parse_metrics(&raw_data);
                    self.metrics.insert(addr, Ok(parsed_metrics));
                }
                Err(e) => {
                    // Store the error string if fetching/parsing failed
                    self.metrics.insert(addr, Err(e));
                }
            }
        }
        // Update the timestamp after processing all results
        self.last_update = Instant::now();
    }
}