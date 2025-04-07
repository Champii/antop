use crate::metrics::{NodeMetrics, parse_metrics};
use glob::glob;
use std::{
    collections::{HashMap, VecDeque},
    fs,                        // Add fs for directory sizing
    io,                        // Add io for error handling
    path::PathBuf,             // Add PathBuf
    time::{Duration, Instant}, // Import Duration
};

// Number of data points to keep for sparklines
pub const SPARKLINE_HISTORY_LENGTH: usize = 60;
// Storage per node in bytes (35 GB)
pub const STORAGE_PER_NODE_BYTES: u64 = 35 * 1_000_000_000;
// Tick rate bounds
const MIN_TICK_RATE: Duration = Duration::from_millis(100);
const MAX_TICK_RATE: Duration = Duration::from_secs(3600); // 1 hour

// Discrete tick rate levels
const TICK_LEVELS: [Duration; 13] = [
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),   // 1m
    Duration::from_secs(300),  // 5m
    Duration::from_secs(600),  // 10m
    Duration::from_secs(1800), // 30m
    Duration::from_secs(3600), // 1h
];

/// Holds the application state.
pub struct App {
    // --- Core Node Data ---
    pub nodes: Vec<String>, // Stores discovered node *directory paths*
    pub node_urls: HashMap<String, String>, // Maps node directory path to metrics URL
    // Store parsed metrics or error string, keyed by *metrics URL*
    pub node_metrics: HashMap<String, Result<NodeMetrics, String>>,
    // Map node directory path to its RECORD STORE path
    pub node_record_store_paths: HashMap<String, PathBuf>,

    // --- Metrics History & Calculation ---
    pub previous_metrics: HashMap<String, NodeMetrics>, // Keyed by metrics URL
    pub last_update: Instant,
    pub previous_update_time: Instant, // Store the time of the previous update
    pub speed_in_history: HashMap<String, VecDeque<u64>>, // Keyed by metrics URL
    pub speed_out_history: HashMap<String, VecDeque<u64>>, // Keyed by metrics URL

    // --- Calculated Totals & Summaries ---
    pub total_speed_in_history: VecDeque<u64>,
    pub total_speed_out_history: VecDeque<u64>,
    pub total_cpu_usage: f64,
    pub total_allocated_storage: u64,
    pub total_used_storage_bytes: Option<u64>,
    pub summary_total_in_speed: f64,
    pub summary_total_out_speed: f64,
    pub summary_total_data_in_bytes: u64,
    pub summary_total_data_out_bytes: u64,
    pub summary_total_records: u64,
    pub summary_total_rewards: u64,
    pub summary_total_live_peers: u64,

    // --- UI State & Config ---
    pub status_message: Option<String>,
    pub scroll_offset: usize, // Track the scroll position for the node list
    pub tick_rate: Duration,  // Current update interval
}

impl App {
    /// Creates a new App instance.
    /// `discovered_node_dirs`: List of full directory paths found matching the path glob.
    /// `initial_node_urls`: List of (directory_path, metrics_url) found initially from logs.
    /// `_node_path_glob_str`: Original glob pattern string (currently unused here but kept for potential future use).
    pub fn new(
        discovered_node_dirs: Vec<String>,
        initial_node_urls: Vec<(String, String)>,
        _node_path_glob_str: String, // Keep param for signature consistency
    ) -> App {
        let mut node_urls_map = HashMap::new();
        let mut metrics_map = HashMap::new();
        let now = Instant::now();
        let speed_in_history = HashMap::new();
        let speed_out_history = HashMap::new();

        // Populate maps based on initially discovered URLs
        for (dir_path, url) in &initial_node_urls {
            node_urls_map.insert(dir_path.clone(), url.clone());
            // Initialize metrics status for nodes with URLs
            metrics_map.insert(url.clone(), Err("Fetching...".to_string()));
        }

        // Discover record store paths based on ALL discovered directories
        let mut node_record_store_paths = HashMap::new();
        for node_dir_str in &discovered_node_dirs {
            let node_dir = PathBuf::from(node_dir_str);
            if node_dir.is_dir() {
                // Should always be true based on find_node_directories
                let record_store_path = node_dir.join("record_store");
                if record_store_path.is_dir() {
                    // Use the full directory path as the key
                    node_record_store_paths.insert(node_dir_str.clone(), record_store_path);
                }
                // Optionally: Add logic here to handle cases where record_store is missing
                // E.g., insert a specific marker or None, or just skip.
                // Current logic implicitly skips nodes without a record_store dir.
            }
        }

        App {
            nodes: discovered_node_dirs, // Store all discovered directory paths
            node_urls: node_urls_map,    // Store mapping for nodes with found URLs
            node_metrics: metrics_map,   // Initialize metrics only for those with URLs
            previous_metrics: HashMap::new(),
            last_update: now,
            speed_in_history,
            speed_out_history,
            previous_update_time: now,
            total_speed_in_history: VecDeque::with_capacity(SPARKLINE_HISTORY_LENGTH),
            total_speed_out_history: VecDeque::with_capacity(SPARKLINE_HISTORY_LENGTH),
            total_cpu_usage: 0.0,
            // Calculate allocated storage based on nodes *with record stores*
            total_allocated_storage: node_record_store_paths.len() as u64 * STORAGE_PER_NODE_BYTES,
            total_used_storage_bytes: None, // Initialize as None, calculated in update_metrics
            summary_total_in_speed: 0.0,
            summary_total_out_speed: 0.0,
            summary_total_data_in_bytes: 0,
            summary_total_data_out_bytes: 0,
            summary_total_records: 0,
            summary_total_rewards: 0,
            summary_total_live_peers: 0,
            node_record_store_paths, // Use the map populated above
            status_message: None,
            scroll_offset: 0,
            tick_rate: TICK_LEVELS[3], // Default tick rate (1 second)
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

    /// Adjusts the application's tick rate (update interval) through discrete levels.
    /// `increase`: true to increase interval (slower updates), false to decrease (faster updates).
    pub fn adjust_tick_rate(&mut self, increase: bool) {
        // Find the current index in the TICK_LEVELS array
        let current_index = TICK_LEVELS.iter().position(|&d| d == self.tick_rate);

        let new_index = match current_index {
            Some(index) => {
                if increase {
                    (index + 1).min(TICK_LEVELS.len() - 1)
                } else {
                    index.saturating_sub(1)
                }
            }
            None => {
                // If current tick_rate isn't exactly in levels, find the closest
                if increase {
                    // Find first level *greater than* current
                    TICK_LEVELS
                        .iter()
                        .position(|&d| d > self.tick_rate)
                        .unwrap_or(TICK_LEVELS.len() - 1) // Default to max if none greater
                } else {
                    // Find last level *less than* current
                    TICK_LEVELS
                        .iter()
                        .rposition(|&d| d < self.tick_rate)
                        .unwrap_or(0) // Default to min if none smaller
                }
            }
        };

        self.tick_rate = TICK_LEVELS[new_index];

        // Optional: Add a status message (can be done in ui/run_app instead)
        // self.status_message = Some(format!("Update interval set to: {:.1?}s", self.tick_rate.as_secs_f64()));
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
