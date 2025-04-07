use crate::metrics::{NodeMetrics, parse_metrics};
use glob::glob;
use std::{
    collections::{HashMap, VecDeque},
    fs,            // Add fs for directory sizing
    io,            // Add io for error handling
    path::PathBuf, // Add PathBuf
    time::Instant,
};

// Number of data points to keep for sparklines
pub const SPARKLINE_HISTORY_LENGTH: usize = 60;
// Storage per node in bytes (35 GB)
pub const STORAGE_PER_NODE_BYTES: u64 = 35 * 1_000_000_000;

/// Holds the application state.
pub struct App {
    pub nodes: Vec<(String, String)>, // Stores (node_name, node_url)
    // Store parsed metrics or error string directly, keyed by node_url
    pub node_metrics: HashMap<String, Result<NodeMetrics, String>>,
    pub previous_metrics: HashMap<String, NodeMetrics>, // Store previous metrics for speed calculation
    pub last_update: Instant,
    pub previous_update_time: Instant, // Store the time of the previous update
    pub speed_in_history: HashMap<String, VecDeque<u64>>, // History for Speed In sparkline
    pub speed_out_history: HashMap<String, VecDeque<u64>>, // History for Speed Out sparkline
    // Calculated totals
    pub total_speed_in_history: VecDeque<u64>, // History for total speed in
    pub total_speed_out_history: VecDeque<u64>, // History for total speed out
    pub total_cpu_usage: f64,
    pub total_allocated_storage: u64,
    pub total_used_storage_bytes: Option<u64>, // Store calculated used storage (Option for errors)
    // Add fields for pre-calculated summary data
    pub summary_total_in_speed: f64,
    pub summary_total_out_speed: f64,
    pub summary_total_data_in_bytes: u64,
    pub summary_total_data_out_bytes: u64,
    pub summary_total_records: u64,
    pub summary_total_rewards: u64,
    pub summary_total_live_peers: u64,
    // Config & Discovered Paths
    pub node_record_store_paths: HashMap<String, PathBuf>, // Map node name to its RECORD STORE path
    pub status_message: Option<String>, // For displaying messages/errors in the footer
    pub scroll_offset: usize,           // NEW: Track the scroll position for the node list
}

impl App {
    /// Creates a new App instance with initial node list and storage path glob.
    pub fn new(nodes: Vec<(String, String)>, node_path_glob_str: String) -> App {
        let mut metrics_map = HashMap::new();
        let now = Instant::now();
        let speed_in_history = HashMap::new();
        let speed_out_history = HashMap::new();
        for (_name, url) in &nodes {
            metrics_map.insert(url.clone(), Err("Fetching...".to_string()));
        }

        // Discover record store paths from the glob pattern
        let mut node_record_store_paths = HashMap::new();
        match glob(&node_path_glob_str) {
            Ok(paths) => {
                for entry in paths {
                    match entry {
                        Ok(node_dir) => {
                            if node_dir.is_dir() {
                                // Look directly for record_store
                                let record_store_path = node_dir.join("record_store");
                                // Check if the record_store subdirectory exists and is a directory
                                if record_store_path.is_dir() {
                                    if let Some(node_name) =
                                        node_dir.file_name().and_then(|n| n.to_str())
                                    {
                                        // Store the record_store path directly
                                        node_record_store_paths
                                            .insert(node_name.to_string(), record_store_path);
                                    }
                                }
                            }
                        }
                        Err(_e) => { /* Optionally log elsewhere */ }
                    }
                }
            }
            Err(_e) => { /* Optionally log elsewhere */ }
        }

        App {
            nodes,
            node_metrics: metrics_map,
            previous_metrics: HashMap::new(),
            last_update: now,
            speed_in_history,
            speed_out_history,
            previous_update_time: now,
            // Initialize totals
            total_speed_in_history: VecDeque::with_capacity(SPARKLINE_HISTORY_LENGTH),
            total_speed_out_history: VecDeque::with_capacity(SPARKLINE_HISTORY_LENGTH),
            total_cpu_usage: 0.0,
            total_allocated_storage: 0, // Will be calculated later based on discovered nodes
            total_used_storage_bytes: None, // Initialize as None
            // Initialize summary fields
            summary_total_in_speed: 0.0,
            summary_total_out_speed: 0.0,
            summary_total_data_in_bytes: 0,
            summary_total_data_out_bytes: 0,
            summary_total_records: 0,
            summary_total_rewards: 0,
            summary_total_live_peers: 0,
            // Store config & discovered paths
            node_record_store_paths,
            status_message: None, // Initialize status message
            scroll_offset: 0,     // Initialize scroll offset
        }
    }

    /// Updates metrics, calculates speeds, totals, and used storage.
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
        self.node_metrics = new_metrics_map;
        self.last_update = update_start_time;

        // --- Calculate Totals ---
        let mut current_total_speed_in: f64 = 0.0;
        let mut current_total_speed_out: f64 = 0.0;
        let mut current_total_cpu: f64 = 0.0;
        // Initialize accumulators for other summary fields
        let mut current_total_data_in: u64 = 0;
        let mut current_total_data_out: u64 = 0;
        let mut current_total_records: u64 = 0;
        let mut current_total_rewards: u64 = 0;
        let mut current_total_live_peers: u64 = 0;

        for metrics in self.node_metrics.values().flatten() {
            // Use flatten()
            if let Some(cpu) = metrics.cpu_usage_percentage {
                current_total_cpu += cpu;
            }
            // Sum speeds for total history
            current_total_speed_in += metrics.speed_in_bps.unwrap_or(0.0);
            current_total_speed_out += metrics.speed_out_bps.unwrap_or(0.0);

            // Sum other summary fields
            current_total_data_in += metrics.bandwidth_inbound_bytes.unwrap_or(0);
            current_total_data_out += metrics.bandwidth_outbound_bytes.unwrap_or(0);
            current_total_records += metrics.records_stored.unwrap_or(0);
            current_total_rewards += metrics.reward_wallet_balance.unwrap_or(0);
            current_total_live_peers += metrics.connected_peers.unwrap_or(0);
        }
        self.total_cpu_usage = current_total_cpu;
        // Calculate allocated storage based on the number of discovered nodes with record stores
        self.total_allocated_storage =
            self.node_record_store_paths.len() as u64 * STORAGE_PER_NODE_BYTES;
        // Store calculated summary totals
        self.summary_total_in_speed = current_total_speed_in;
        self.summary_total_out_speed = current_total_speed_out;
        self.summary_total_data_in_bytes = current_total_data_in;
        self.summary_total_data_out_bytes = current_total_data_out;
        self.summary_total_records = current_total_records;
        self.summary_total_rewards = current_total_rewards;
        self.summary_total_live_peers = current_total_live_peers;

        // Update total speed history
        let total_in_val = current_total_speed_in.max(0.0) as u64;
        let total_out_val = current_total_speed_out.max(0.0) as u64;

        self.total_speed_in_history.push_back(total_in_val);
        self.total_speed_out_history.push_back(total_out_val);

        if self.total_speed_in_history.len() > SPARKLINE_HISTORY_LENGTH {
            self.total_speed_in_history.pop_front();
        }
        if self.total_speed_out_history.len() > SPARKLINE_HISTORY_LENGTH {
            self.total_speed_out_history.pop_front();
        }

        // --- Calculate Total Used Storage ---
        let mut current_total_used: u64 = 0;
        let calculation_possible = true;
        // Iterate over discovered record store paths
        for record_store_path in self.node_record_store_paths.values() {
            // The path IS the record_store path, so check it directly
            if record_store_path.is_dir() {
                // Check should pass if it was added correctly
                match calculate_dir_size(record_store_path) {
                    // Calculate size of record_store_path
                    Ok(size) => current_total_used += size,
                    Err(_e) => { /* Optionally log elsewhere */ }
                }
            } else {
                // This case should ideally not happen if App::new logic is correct
            }
        }

        if calculation_possible {
            self.total_used_storage_bytes = Some(current_total_used);
        } else {
            self.total_used_storage_bytes = None;
        }
    }
}

/// Recursively calculate the total size of a directory.
/// Includes basic error handling for permissions etc.
fn calculate_dir_size(path: &PathBuf) -> io::Result<u64> {
    let mut total_size = 0;
    let metadata = fs::metadata(path)?; // Propagate initial metadata error

    if metadata.is_dir() {
        for entry_result in fs::read_dir(path)? {
            let entry = entry_result?; // Handle read_dir entry error
            let entry_path = entry.path();
            let entry_metadata = match fs::symlink_metadata(&entry_path) {
                Ok(md) => md,
                Err(_e) => continue, // Skip files/dirs we can't get metadata for
            };

            if entry_metadata.is_dir() {
                // Recursively call, adding result if successful, propagating error otherwise
                // If a subdirectory fails, maybe we should skip it instead of failing the whole calculation?
                // Let's try skipping it:
                match calculate_dir_size(&entry_path) {
                    Ok(size) => total_size += size,
                    Err(_e) => { /* Optionally log elsewhere */ }
                }
            } else if entry_metadata.is_file() {
                total_size += entry_metadata.len();
            }
            // Ignore symlinks, sockets, etc. for size calculation
        }
    } else if metadata.is_file() {
        // If the initial path is a file, just return its size
        total_size = metadata.len();
    }

    Ok(total_size)
}
