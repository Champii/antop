use crate::metrics::NodeMetrics;
use humansize::{DECIMAL, format_size};

// Helper to format Option<T> for display
pub fn format_option<T: std::fmt::Display>(opt: Option<T>) -> String {
    match opt {
        Some(val) => val.to_string(),
        None => "-".to_string(),
    }
}

// Helper to format uptime into d HH:MM:SS or HH:MM:SS
pub fn format_uptime(seconds: Option<u64>) -> String {
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
pub fn format_float(opt: Option<f64>, precision: usize) -> String {
    match opt {
        Some(val) => format!("{:.prec$}", val, prec = precision),
        None => "-".to_string(),
    }
}

// Helper to format peer counts concisely
pub fn format_peers(connected: Option<u32>, total: Option<u32>) -> String {
    match (connected, total) {
        (Some(c), Some(t)) => format!("{}/{}", c, t),
        (Some(c), None) => format!("{}/-", c),
        (None, Some(t)) => format!("-/{}", t),
        (None, None) => "-".to_string(),
    }
}

// Helper to format Option<u64> bytes into human-readable size (KB, MB, GB)
pub fn format_option_u64_bytes(opt: Option<u64>) -> String {
    match opt {
        Some(val) => humansize::format_size(val, humansize::DECIMAL), // Use humansize formatting
        None => "-".to_string(),
    }
}

// Helper to format Option<f64> speed in Bps to human-readable KB/s, MB/s etc.
pub fn format_speed_bps(speed_bps: Option<f64>) -> String {
    match speed_bps {
        Some(bps) if bps >= 0.0 => {
            // Use humansize for formatting, append "/s"
            format!("{}/s", format_size(bps as u64, DECIMAL))
        }
        _ => "-".to_string(), // Handle None or negative values (e.g., initial state)
    }
}

// Helper to create a vector of formatted data cell strings for a list item
pub fn create_list_item_cells(name: &str, metrics: &NodeMetrics) -> Vec<String> {
    // Calculate total errors
    let put_err = metrics.put_record_errors.unwrap_or(0);
    let conn_in_err = metrics.incoming_connection_errors.unwrap_or(0);
    let conn_out_err = metrics.outgoing_connection_errors.unwrap_or(0);
    let kad_err = metrics.kad_get_closest_peers_errors.unwrap_or(0);
    let total_errors = put_err + conn_in_err + conn_out_err + kad_err;

    // Format values according to HEADER widths and push to vector
    vec![
        format!("{:<18}", name),                                          // Node
        format!("{:<12}", format_uptime(metrics.uptime_seconds)),         // Uptime
        format!("{:<6}MB", format_float(metrics.memory_used_mb, 1)),      // Mem MB
        format!("{:<5}%", format_float(metrics.cpu_usage_percentage, 1)), // CPU %
        format!(
            "{:<7}",
            format_peers(
                metrics.connected_peers.map(|v| v as u32),
                metrics.peers_in_routing_table.map(|v| v as u32)
            )
        ), // Peers
        // Use total bytes (bandwidth_..._bytes) and format_option_u64_bytes instead of current speed
        format!(
            "{}",
            format_option_u64_bytes(metrics.bandwidth_inbound_bytes)
        ), // Total In
        format!(
            "{}",
            format_option_u64_bytes(metrics.bandwidth_outbound_bytes)
        ), // Total Out
        format!("{:<7}", format_option(metrics.records_stored)), // Records
        format!("{:<8}", format_option(metrics.reward_wallet_balance)), // Reward
        format!("{:<4}", total_errors),                          // Err
                                                                 // Status is handled separately in render_custom_node_rows
    ]
}

// Helper to create placeholder cells for error/unknown states
pub fn create_placeholder_cells(name: &str) -> Vec<String> {
    vec![
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
    ]
}
