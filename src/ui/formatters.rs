use crate::metrics::NodeMetrics;
use humansize::{DECIMAL, format_size};
use std::path::Path;

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
pub fn create_list_item_cells(root_path: &str, metrics: &NodeMetrics) -> Vec<String> {
    let put_err = metrics.put_record_errors.unwrap_or(0);
    let conn_in_err = metrics.incoming_connection_errors.unwrap_or(0);
    let conn_out_err = metrics.outgoing_connection_errors.unwrap_or(0);
    let kad_err = metrics.kad_get_closest_peers_errors.unwrap_or(0);
    let total_errors = put_err + conn_in_err + conn_out_err + kad_err;

    // Extract the last component (directory name)
    let node_name = Path::new(root_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(root_path); // Fallback to full path if extraction fails

    vec![
        node_name.to_string(), // Let widget handle alignment
        format!("{}", format_uptime(metrics.uptime_seconds)), // Uptime
        format!("{}MB", format_float(metrics.memory_used_mb, 1)), // Mem MB
        format!("{}%", format_float(metrics.cpu_usage_percentage, 2)), // CPU %
        format!("{}", format_option(metrics.connected_peers)), // Peers (Live)
        format!("{}", format_option(metrics.peers_in_routing_table)), // Routing Table Size
        format!("{}", format_option(metrics.records_stored)), // Records
        format!("{}", format_option(metrics.reward_wallet_balance)), // Reward
        format!("{}", total_errors), // Err
                               // Status is handled separately in render_custom_node_rows
    ]
}

// Helper to create placeholder cells for error/unknown states
pub fn create_placeholder_cells(root_path: &str) -> Vec<String> {
    // Extract the last component (directory name)
    let node_name = Path::new(root_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(root_path); // Fallback to full path if extraction fails

    vec![
        node_name.to_string(),  // Let widget handle alignment
        format!("{:>11}", "-"), // Uptime (Right aligned, width 11)
        format!("{:>6}", "-"),  // Mem MB (Right aligned, width 6)
        format!("{:>5}", "-"),  // CPU % (Right aligned, width 5)
        format!("{:>5}", "-"),  // Peers (Live) (Right aligned, width 5)
        format!("{:>7}", "-"),  // Routing Table Size (Right aligned, width 7)
        format!("{:>7}", "-"),  // Records (Right aligned, width 7)
        format!("{:>8}", "-"),  // Reward (Right aligned, width 8)
        format!("{:>3}", "-"),  // Err (Right aligned, width 3)
    ]
}

/// Formats a Duration into a human-readable string (ms, s, min, hr).
pub fn format_duration_human(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();

    if secs < 1.0 {
        format!("{}ms", duration.as_millis())
    } else if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        format!("{:.0}min", secs / 60.0)
    } else {
        format!("{:.0}hr", secs / 3600.0)
    }
}
