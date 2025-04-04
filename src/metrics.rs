use std::str::FromStr;

/// Structure to hold parsed metrics from an antnode.
#[derive(Debug, Default, Clone)]
pub struct NodeMetrics {
    pub uptime_seconds: Option<u64>,
    pub memory_used_mb: Option<f64>,
    pub cpu_usage_percentage: Option<f64>,
    pub connected_peers: Option<u64>,
    pub peers_in_routing_table: Option<u64>,
    pub estimated_network_size: Option<u64>,
    pub bandwidth_inbound_bytes: Option<u64>,
    pub bandwidth_outbound_bytes: Option<u64>,
    pub records_stored: Option<u64>,
    pub put_record_errors: Option<u64>,
    pub reward_wallet_balance: Option<u64>, // Assuming integer units
    pub incoming_connection_errors: Option<u64>,
    pub outgoing_connection_errors: Option<u64>,
    pub kad_get_closest_peers_errors: Option<u64>,
    pub speed_in_bps: Option<f64>,
    pub speed_out_bps: Option<f64>,
}

/// Parses the raw metrics text into a NodeMetrics struct.
pub fn parse_metrics(metrics_data: &str) -> NodeMetrics {
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
            "ant_networking_process_memory_used_mb" => {
                metrics.memory_used_mb = parse_value::<f64>(value_str)
            }
            "ant_networking_process_cpu_usage_percentage" => {
                metrics.cpu_usage_percentage = parse_value::<f64>(value_str)
            }
            "ant_networking_connected_peers" => {
                metrics.connected_peers = parse_value::<u64>(value_str)
            }
            "ant_networking_peers_in_routing_table" => {
                metrics.peers_in_routing_table = parse_value::<u64>(value_str)
            }
            "ant_networking_estimated_network_size" => {
                metrics.estimated_network_size = parse_value::<u64>(value_str)
            }
            "ant_networking_records_stored" => {
                metrics.records_stored = parse_value::<u64>(value_str)
            }
            "ant_node_put_record_err_total" => {
                metrics.put_record_errors = parse_value::<u64>(value_str)
            }
            "ant_node_current_reward_wallet_balance" => {
                metrics.reward_wallet_balance = parse_value::<u64>(value_str)
            }
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
    if incoming_connection_errors_sum > 0
        || metrics_data.contains("libp2p_swarm_connections_incoming_error_total")
    {
        metrics.incoming_connection_errors = Some(incoming_connection_errors_sum);
    }
    if outgoing_connection_errors_sum > 0
        || metrics_data.contains("libp2p_swarm_outgoing_connection_error_total")
    {
        metrics.outgoing_connection_errors = Some(outgoing_connection_errors_sum);
    }
    if kad_get_closest_peers_errors_sum > 0
        || metrics_data.contains("libp2p_kad_query_result_get_closest_peers_error_total")
    {
        metrics.kad_get_closest_peers_errors = Some(kad_get_closest_peers_errors_sum);
    }

    metrics
}
