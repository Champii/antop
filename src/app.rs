use crate::metrics::{NodeMetrics, parse_metrics};
// use ratatui::widgets::{ListState, TableState}; // Removed, unused
use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

// Number of data points to keep for sparklines
pub const SPARKLINE_HISTORY_LENGTH: usize = 60;
// Storage per node in bytes (35 GB)
pub const STORAGE_PER_NODE_BYTES: u64 = 35 * 1_000_000_000;

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
    // Calculated totals
    pub total_cpu_usage: f64,
    pub total_allocated_storage: u64,
    // pub table_state: TableState, // Removed, unused
    // pub list_state: ListState, // Removed, unused
}

impl App {
    /// Creates a new App instance with initial server list.
    pub fn new(servers: Vec<(String, String)>) -> App {
        let mut metrics_map = HashMap::new();
        let now = Instant::now();
        let speed_in_history = HashMap::new();
        let speed_out_history = HashMap::new();
        for (_name, url) in &servers {
            metrics_map.insert(url.clone(), Err("Fetching...".to_string()));
        }
        App {
            servers,
            metrics: metrics_map,
            previous_metrics: HashMap::new(),
            last_update: now,
            speed_in_history,
            speed_out_history,
            previous_update_time: now,
            // Initialize totals
            total_cpu_usage: 0.0,
            total_allocated_storage: 0,
            // table_state: TableState::default(), // Removed, unused
            // list_state: ListState::default(), // Removed, unused
        }
    }

    /// Updates metrics based on fetch results and calculates speeds and totals.
    /// Takes results from fetch_metrics: Vec<(address, Result<raw_data, error_string>)>
    pub fn update_metrics(&mut self, results: Vec<(String, Result<String, String>)>) {
        let update_start_time = Instant::now();
        let delta_time = update_start_time
            .duration_since(self.previous_update_time)
            .as_secs_f64();

        let mut new_metrics_map = HashMap::new();
        let mut next_previous_metrics = HashMap::new();

        for (addr, result) in results {
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
                    let mut current_metrics = parse_metrics(&raw_data);

                    if let Some(prev_metrics) = self.previous_metrics.get(&addr) {
                        if delta_time > 0.0 {
                            if let (Some(current_in), Some(prev_in)) = (
                                current_metrics.bandwidth_inbound_bytes,
                                prev_metrics.bandwidth_inbound_bytes,
                            ) {
                                if current_in >= prev_in {
                                    let delta_bytes = current_in - prev_in;
                                    current_metrics.speed_in_bps =
                                        Some(delta_bytes as f64 / delta_time);
                                } else {
                                    current_metrics.speed_in_bps = Some(0.0);
                                }
                            }

                            if let (Some(current_out), Some(prev_out)) = (
                                current_metrics.bandwidth_outbound_bytes,
                                prev_metrics.bandwidth_outbound_bytes,
                            ) {
                                if current_out >= prev_out {
                                    let delta_bytes = current_out - prev_out;
                                    current_metrics.speed_out_bps =
                                        Some(delta_bytes as f64 / delta_time);
                                } else {
                                    current_metrics.speed_out_bps = Some(0.0);
                                }
                            }
                        }
                    }

                    let speed_in_val = current_metrics.speed_in_bps.unwrap_or(0.0).max(0.0) as u64;
                    let speed_out_val =
                        current_metrics.speed_out_bps.unwrap_or(0.0).max(0.0) as u64;

                    history_in.push_back(speed_in_val);
                    history_out.push_back(speed_out_val);

                    if history_in.len() > SPARKLINE_HISTORY_LENGTH {
                        history_in.pop_front();
                    }
                    if history_out.len() > SPARKLINE_HISTORY_LENGTH {
                        history_out.pop_front();
                    }
                    current_metrics.chart_data_in = Some(
                        history_in
                            .iter()
                            .enumerate()
                            .map(|(i, &val)| (i as f64, val as f64))
                            .collect(),
                    );
                    current_metrics.chart_data_out = Some(
                        history_out
                            .iter()
                            .enumerate()
                            .map(|(i, &val)| (i as f64, val as f64))
                            .collect(),
                    );

                    next_previous_metrics.insert(addr.clone(), current_metrics.clone());
                    new_metrics_map.insert(addr.clone(), Ok(current_metrics.clone())); // Clone to avoid move
                }
                Err(e) => {
                    new_metrics_map.insert(addr.clone(), Err(e));
                    history_in.push_back(0);
                    history_out.push_back(0);

                    if history_in.len() > SPARKLINE_HISTORY_LENGTH {
                        history_in.pop_front();
                    }
                    if history_out.len() > SPARKLINE_HISTORY_LENGTH {
                        history_out.pop_front();
                    }
                }
            }
        }

        self.previous_metrics = next_previous_metrics;
        self.previous_update_time = self.last_update;
        self.metrics = new_metrics_map;
        self.last_update = update_start_time;

        // --- Calculate Totals ---
        let mut current_total_cpu: f64 = 0.0;
        for result in self.metrics.values() {
            if let Ok(metrics) = result {
                if let Some(cpu) = metrics.cpu_usage_percentage {
                    current_total_cpu += cpu;
                }
            }
        }
        self.total_cpu_usage = current_total_cpu;
        self.total_allocated_storage = self.servers.len() as u64 * STORAGE_PER_NODE_BYTES;
    }
}
