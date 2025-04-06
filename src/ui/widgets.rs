use super::formatters::{
    // Use super to access sibling module
    create_list_item_cells,
    create_placeholder_cells,
    format_option_u64_bytes, // Import for formatting storage
    format_speed_bps,
};
use crate::app::{App, STORAGE_PER_NODE_BYTES}; // Import App and constant
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize}, // Add Stylize
    symbols,
    text::{Line, Span}, // Add Span
    widgets::{
        Axis, Block, Borders, Cell, Chart, Dataset, Gauge, GraphType, Paragraph, Row, Table,
    },
};

// --- Constants ---

const HEADER_TITLES: [&str; 11] = [
    // Reduced size from 12 to 11
    "Node",
    "Uptime",
    "Mem",
    "CPU",
    "Peers",   // Live Peers
    "Routing", // Routing Table Size
    "Total In",
    "Total Out",
    "Recs",
    "Rwds",
    "Err",
    // "Status" moved
];
const HEADER_STYLE: Style = Style::new().fg(Color::Yellow); // Use Style::new() for const
const DATA_CELL_STYLE: Style = Style::new().fg(Color::Gray); // Use Style::new() for const

pub const COLUMN_CONSTRAINTS: [Constraint; 14] = [
    Constraint::Ratio(1, 20), // 0: Node
    Constraint::Ratio(1, 20), // 1: Uptime
    Constraint::Ratio(1, 20), // 2: Mem MB
    Constraint::Ratio(1, 20), // 3: CPU %
    Constraint::Ratio(1, 20), // 4: Peers (Live)
    Constraint::Ratio(1, 20), // 5: Routing
    Constraint::Ratio(1, 20), // 6: Total In
    Constraint::Ratio(1, 20), // 7: Total Out
    Constraint::Ratio(1, 20), // 8: Records
    Constraint::Ratio(1, 20), // 9: Reward
    Constraint::Ratio(1, 20), // 10: Err
    // Status constraint moved
    Constraint::Ratio(4, 20), // 11: Rx Chart Area (was 12)
    Constraint::Ratio(4, 20), // 12: Tx Chart Area (was 13)
    Constraint::Ratio(1, 20), // 13: Status (was 11)
]; // Ratios adjusted to sum to 1 (11*1 + 2*4 + 1*1 = 20)

// --- NEW: Summary Gauges ---

/// Renders the summary section with gauges for CPU and Storage.
pub fn render_summary_gauges(f: &mut Frame, app: &App, area: Rect) {
    // Create a block for the summary section (optional, could be removed if no border needed)
    // let summary_block = Block::default().borders(Borders::NONE);
    // f.render_widget(summary_block, area);
    // let inner_area = summary_block.inner(area); // Use area directly if no block

    // Outer layout: Limit width for gauges and add space for speed
    let outer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Gauges area
            Constraint::Percentage(15), // Speed area
            Constraint::Percentage(15), // Totals area (Recs/Rwds, In/Out)
            Constraint::Percentage(15), // NEW: Peers area
            Constraint::Percentage(35), // Empty space
        ])
        .split(area); // Use the full area passed to the function

    let gauges_area = outer_chunks[0];
    let speed_area = outer_chunks[1];
    let totals_area = outer_chunks[2];
    let peers_area = outer_chunks[3]; // NEW: Area for Peers total

    // Inner layout: Stack gauges vertically within the gauges_area
    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // CPU Gauge (1 line high)
            Constraint::Length(1), // Storage Gauge (1 line high)
        ])
        .split(gauges_area); // Split the restricted gauges_area

    // --- CPU Gauge ---
    let cpu_percentage = app.total_cpu_usage;
    // let max_cpu_possible = (app.servers.len() as f64 * 100.0).max(1.0);
    // let cpu_ratio = (cpu_percentage / max_cpu_possible).min(1.0).max(0.0);

    // Simplified label for smaller space
    let cpu_label = Span::styled(
        format!("CPU {:.2}%", cpu_percentage),
        Style::default().fg(Color::Blue),
    );
    let cpu_gauge = Gauge::default()
        // .block(Block::default().title(Span::styled("CPU", Style::new().bold())))
        .gauge_style(Color::Black)
        .ratio(cpu_percentage / 100.0) // Use ratio directly for better precision control
        // .percent((cpu_ratio * 100.0) as u16) // Alternative using percent
        .label(cpu_label);
    f.render_widget(cpu_gauge, gauge_chunks[0]);

    // --- Storage Gauge ---
    let allocated_bytes = app.total_allocated_storage;
    let allocated_formatted = format_option_u64_bytes(Some(allocated_bytes));

    let (storage_ratio, storage_label) = match app.total_used_storage_bytes {
        Some(used_bytes) if allocated_bytes > 0 => {
            let ratio = (used_bytes as f64 / allocated_bytes as f64)
                .min(1.0)
                .max(0.0);
            let used_formatted = format_option_u64_bytes(Some(used_bytes));
            let label = Span::styled(
                format!(
                    "{} / {} ({:.2}%)",
                    used_formatted,
                    allocated_formatted,
                    ratio * 100.0
                ),
                Style::default().fg(Color::Green),
            );
            (ratio, label)
        }
        Some(_) => {
            // Used bytes known, but allocation is 0 (no nodes?)
            (
                0.0,
                Span::styled(
                    format!("0 / {}", allocated_formatted),
                    Style::default().fg(Color::Green),
                ),
            )
        }
        None => {
            // Error calculating used bytes
            (
                0.0,
                Span::styled("Error".to_string(), Style::default().fg(Color::Red)),
            )
        }
    };

    // Simplified label
    // let storage_label = allocated_formatted;
    let storage_gauge = Gauge::default()
        // .block(Block::default().title(Span::styled("Store", Style::new().bold()))) // Shortened title
        .gauge_style(Color::Black)
        .ratio(storage_ratio) // Use the calculated ratio
        // .percent(100) // REMOVED
        .label(storage_label); // Show Used / Allocated
    f.render_widget(storage_gauge, gauge_chunks[1]);

    // --- Speed Area --- NEW
    // Calculate totals first
    let mut total_in: f64 = 0.0;
    let mut total_out: f64 = 0.0;

    for metrics_result in app.metrics.values() {
        if let Ok(metrics) = metrics_result {
            total_in += metrics.speed_in_bps.unwrap_or(0.0);
            total_out += metrics.speed_out_bps.unwrap_or(0.0);
        }
    }

    // Layout for speed section (Text + Chart for In and Out)
    let speed_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Row for In speed + chart
            Constraint::Length(1), // Row for Out speed + chart
        ])
        .split(speed_area);

    let in_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Text Label (In)
            Constraint::Percentage(50), // Chart (In)
        ])
        .split(speed_layout[0]);

    let out_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Text Label (Out)
            Constraint::Percentage(50), // Chart (Out)
        ])
        .split(speed_layout[1]);

    // Use calculated totals for text
    let total_in_speed_str = format_speed_bps(Some(total_in));
    let total_out_speed_str = format_speed_bps(Some(total_out));

    let in_text_label = Line::from(vec![
        Span::styled("In:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(total_in_speed_str, Style::default().fg(Color::Cyan)),
    ]);
    let out_text_label = Line::from(vec![
        Span::styled("Out: ", Style::default().fg(Color::DarkGray)),
        Span::styled(total_out_speed_str, Style::default().fg(Color::Magenta)),
    ]);

    let in_paragraph = Paragraph::new(in_text_label).alignment(Alignment::Right);
    let out_paragraph = Paragraph::new(out_text_label).alignment(Alignment::Right);

    // Create Datasets for Charts (converting history data)
    let total_in_data: Vec<(f64, f64)> = app
        .total_speed_in_history
        .iter()
        .enumerate()
        .map(|(i, &val)| (i as f64, val as f64))
        .collect();

    let total_out_data: Vec<(f64, f64)> = app
        .total_speed_out_history
        .iter()
        .enumerate()
        .map(|(i, &val)| (i as f64, val as f64))
        .collect();

    // Create Charts (similar config to row charts)
    let in_chart = create_summary_chart(&total_in_data, Color::Cyan, "Total Rx");
    let out_chart = create_summary_chart(&total_out_data, Color::Magenta, "Total Tx");

    // Render Text and Charts
    f.render_widget(in_paragraph, in_row_layout[0]);
    if let Some(chart) = in_chart {
        f.render_widget(chart, in_row_layout[1]);
    }
    f.render_widget(out_paragraph, out_row_layout[0]);
    if let Some(chart) = out_chart {
        f.render_widget(chart, out_row_layout[1]);
    }

    // --- Totals Area (Recs/Rwds, In/Out) ---
    let mut total_records: u64 = 0;
    let mut total_rewards: u64 = 0;
    let mut total_data_in_bytes: u64 = 0;
    let mut total_data_out_bytes: u64 = 0;
    let mut total_live_peers: u64 = 0; // Still calculate here for simplicity

    for metrics_result in app.metrics.values() {
        if let Ok(metrics) = metrics_result {
            total_records += metrics.records_stored.unwrap_or(0);
            total_rewards += metrics.reward_wallet_balance.unwrap_or(0);
            total_data_in_bytes += metrics.bandwidth_inbound_bytes.unwrap_or(0);
            total_data_out_bytes += metrics.bandwidth_outbound_bytes.unwrap_or(0);
            total_live_peers += metrics.connected_peers.unwrap_or(0); // Calculate peers total
        }
    }

    // Convert bytes to GB for display
    let total_data_in_gb = total_data_in_bytes as f64 / 1_000_000_000.0;
    let total_data_out_gb = total_data_out_bytes as f64 / 1_000_000_000.0;

    // Layout for the Totals section - Now 2 rows
    let totals_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Row 1: Recs | Rwds
            Constraint::Length(1), // Row 2: In GB | Out GB
                                   // Row 3 removed
        ])
        .split(totals_area); // Render into the 'totals_area'

    // Create combined Lines for rows 1 and 2
    let recs_rwds_text = Line::from(vec![
        Span::styled("Recs:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:<5}", total_records),
            Style::default().fg(Color::White),
        ),
        Span::raw("| "), // Re-add separator if user removed it accidentally
        Span::styled("Rwds:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:<5}", total_rewards),
            Style::default().fg(Color::Yellow),
        ),
    ]);

    let data_in_out_text = Line::from(vec![
        Span::styled("In: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:<5.2}GB", total_data_in_gb),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("| "), // Re-add separator if user removed it accidentally
        Span::styled("Out:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:<5.2}GB", total_data_out_gb),
            Style::default().fg(Color::Magenta),
        ),
    ]);

    // Peers text creation removed from here

    // Render the paragraphs for the middle totals section
    f.render_widget(
        Paragraph::new(recs_rwds_text).alignment(Alignment::Left),
        totals_layout[0],
    );
    f.render_widget(
        Paragraph::new(data_in_out_text).alignment(Alignment::Left),
        totals_layout[1],
    );
    // Peers rendering removed from here

    // --- NEW: Peers Area ---
    let peers_text = Line::from(vec![
        Span::styled("Peers:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", total_live_peers),
            Style::default().fg(Color::Blue),
        ),
    ]);
    // Render the peers text in its own area, align left for consistency? Or center? Let's try left.
    f.render_widget(
        Paragraph::new(peers_text).alignment(Alignment::Left),
        peers_area,
    );
}

// NEW Helper function to create summary charts consistently
fn create_summary_chart<'a>(
    data: &'a [(f64, f64)],
    color: Color,
    name: &'a str,
) -> Option<Chart<'a>> {
    if data.len() < 2 {
        return None; // Not enough data to draw a line
    }

    let max_len = data.len();
    let max_y = data
        .iter()
        .map(|&(_, y)| y)
        .fold(0.0f64, |max, y| max.max(y));

    // Define bounds, ensuring y starts at 0 and x covers the data range
    let x_bounds = [0.0, (max_len.saturating_sub(1)).max(1) as f64];
    let y_bounds = [0.0, max_y.max(1.0)]; // Ensure y-axis starts at 0 and has at least height 1

    let dataset = Dataset::default()
        .name(name)
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(data);

    let chart = Chart::new(vec![dataset])
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds(x_bounds)
                .labels::<Vec<Line<'_>>>(vec![]), // No labels
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds(y_bounds)
                .labels::<Vec<Line<'_>>>(vec![]), // No labels
        );

    Some(chart)
}

// --- Rendering Helpers ---

/// Renders the header row with column titles.
pub fn render_header(f: &mut Frame, area: Rect) {
    // Split the entire header area into columns using the unified constraints
    let header_column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the updated constant for all columns
        .split(area);

    // Render each header title (Node to Err) in its respective column chunk (0-10)
    for (i, title) in HEADER_TITLES.iter().enumerate() {
        let chunk_index = i; // Indices 0 to 10

        if chunk_index < header_column_chunks.len() {
            // Should always be true here
            let alignment = if i == 0 {
                Alignment::Left
            } else {
                Alignment::Right
            };
            let title_paragraph = Paragraph::new(*title)
                .style(HEADER_STYLE)
                .alignment(alignment);
            f.render_widget(title_paragraph, header_column_chunks[chunk_index]);
        }
    }

    // Render Rx, Tx, and Status titles in their new positions
    let rx_title_paragraph = Paragraph::new("Rx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(rx_title_paragraph, header_column_chunks[11]); // Rx is now index 11

    let tx_title_paragraph = Paragraph::new("Tx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(tx_title_paragraph, header_column_chunks[12]); // Tx is now index 12

    let status_title_paragraph = Paragraph::new("Status")
        .style(HEADER_STYLE)
        .alignment(Alignment::Right); // Align right like other data columns
    f.render_widget(status_title_paragraph, header_column_chunks[13]); // Status is now index 13
}

/// Renders a single bandwidth chart (Rx or Tx) and its associated speed.
fn render_bandwidth_chart_and_speed(
    f: &mut Frame,
    area: Rect,
    chart_data: Option<&[(f64, f64)]>,
    current_speed: Option<f64>,
    color: Color,
    name: &str, // For dataset name
) {
    // Calculate a centered area within the provided cell area (e.g., 80% width)
    let target_width = (area.width as f32 * 0.8).max(1.0).round() as u16; // Ensure at least 1 width
    let padding = area.width.saturating_sub(target_width) / 2;
    let centered_area = Rect {
        x: area.x + padding,
        y: area.y,
        width: target_width,
        height: area.height,
    };

    // Split the *centered* area: 70% chart, 30% speed
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(centered_area); // Use centered_area
    let chart_area = chunks[0];
    let speed_area = chunks[1];

    if let Some(data) = chart_data.filter(|d| d.len() >= 2) {
        let max_len = data.len();
        let max_y = data
            .iter()
            .map(|&(_, y)| y)
            .fold(0.0f64, |max, y| max.max(y));
        let x_bounds = [0.0, (max_len.saturating_sub(1)).max(1) as f64];
        let y_bounds = [0.0, max_y.max(1.0)]; // Ensure y-axis starts at 0 and has at least height 1

        let dataset = Dataset::default()
            .name(name)
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(color))
            .data(data);

        let chart = Chart::new(vec![dataset])
            .x_axis(
                Axis::default()
                    .style(Style::default().fg(Color::DarkGray))
                    .bounds(x_bounds)
                    .labels::<Vec<Line<'_>>>(vec![]), // No labels
            )
            .y_axis(
                Axis::default()
                    .style(Style::default().fg(Color::DarkGray))
                    .bounds(y_bounds)
                    .labels::<Vec<Line<'_>>>(vec![]), // No labels
            );
        f.render_widget(chart, chart_area);

        // Render current speed next to the chart
        let speed_text = format_speed_bps(current_speed);
        let speed_paragraph = Paragraph::new(speed_text).alignment(Alignment::Right);
        f.render_widget(speed_paragraph, speed_area);
    } else {
        // Placeholder for the entire chart + speed area if no data
        let placeholder = Paragraph::new("-")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        // Render placeholder in the centered area, not the full original area
        f.render_widget(placeholder, centered_area);
    }
}

/// Renders a single node's data row, including text cells and bandwidth charts.
pub fn render_node_row(f: &mut Frame, app: &App, area: Rect, name: &str, url: &str) {
    // Split the entire row area using the unified constraints
    let column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the updated constant for all columns
        .split(area);

    // Indices adjusted after moving Status
    let rx_area = column_chunks[11]; // Rx chart is now index 11
    let tx_area = column_chunks[12]; // Tx chart is now index 12
    let status_area = column_chunks[13]; // Status is now index 13

    // Get metrics and determine style + status
    let metrics_result = app.metrics.get(url);
    let (data_cells, style, status_text) = match metrics_result {
        Some(Ok(metrics)) => (
            create_list_item_cells(name, metrics),
            Style::default().fg(Color::Green),
            "Running".to_string(),
        ),
        Some(Err(_)) => (
            create_placeholder_cells(name),
            Style::default().fg(Color::Yellow),
            "Stopped".to_string(),
        ),
        None => (
            create_placeholder_cells(name),
            Style::default().fg(Color::DarkGray),
            "Unknown".to_string(),
        ),
    };

    // Render data cells (Node to Err) into chunks 0-10
    // Assuming data_cells still contains the status conceptually at index 11,
    // but we render it separately later.
    for (idx, cell_text) in data_cells.iter().take(11).enumerate() {
        // Take only the first 11 cells (0-10)
        let chunk_index = idx;

        // Render into chunks 0-10
        let alignment = if idx == 0 {
            Alignment::Left
        } else {
            Alignment::Right
        }; // Align Node left
        let cell_paragraph = Paragraph::new(cell_text.clone())
            .style(DATA_CELL_STYLE)
            .alignment(alignment);
        f.render_widget(cell_paragraph, column_chunks[chunk_index]);
    }

    // Render status separately in the new status column chunk
    let status_paragraph = Paragraph::new(status_text) // format! is redundant for String
        .style(style) // Use the determined style (Green/Yellow/Gray)
        .alignment(Alignment::Right);
    f.render_widget(status_paragraph, status_area); // Render in the dedicated status_area (index 13)

    // --- Render Separate Rx/Tx Charts ---
    let (chart_data_in, chart_data_out, speed_in, speed_out) = metrics_result
        .and_then(|res| res.as_ref().ok()) // Get Option<&NodeMetrics>
        .map_or((None, None, None, None), |m| {
            (
                m.chart_data_in.as_deref(),
                m.chart_data_out.as_deref(),
                m.speed_in_bps,
                m.speed_out_bps,
            )
        });

    // Render Rx Chart and Speed
    render_bandwidth_chart_and_speed(f, rx_area, chart_data_in, speed_in, Color::Cyan, "Rx");

    // Render Tx Chart and Speed
    render_bandwidth_chart_and_speed(f, tx_area, chart_data_out, speed_out, Color::Magenta, "Tx");
}
