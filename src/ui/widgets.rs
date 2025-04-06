use super::formatters::{
    // Use super to access sibling module
    create_list_item_cells,
    create_placeholder_cells,
    format_option_u64_bytes, // Import for formatting storage
    format_speed_bps,
};
use crate::app::App; // Import App and constant
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style}, // Remove Stylize
    symbols,
    text::{Line, Span}, // Add Span
    widgets::{Axis, Chart, Dataset, Gauge, GraphType, Paragraph},
};

// --- Constants ---

const HEADER_TITLES: [&str; 9] = [
    // Reduced size from 11 to 9
    "Node", "Uptime", "Mem", "CPU", "Peers",   // Live Peers
    "Routing", // Routing Table Size
    "Recs", "Rwds", "Err",
    // "Status" moved
];
const HEADER_STYLE: Style = Style::new().fg(Color::Yellow); // Use Style::new() for const
const DATA_CELL_STYLE: Style = Style::new().fg(Color::Gray); // Use Style::new() for const

pub const COLUMN_CONSTRAINTS: [Constraint; 12] = [
    // Indices shifted after removing Total In/Out
    Constraint::Ratio(1, 18), // 0: Node
    Constraint::Ratio(1, 18), // 1: Uptime
    Constraint::Ratio(1, 18), // 2: Mem MB
    Constraint::Ratio(1, 18), // 3: CPU %
    Constraint::Ratio(1, 18), // 4: Peers (Live)
    Constraint::Ratio(1, 18), // 5: Routing
    // Total In/Out constraints removed
    Constraint::Ratio(1, 18), // 6: Records (was 8)
    Constraint::Ratio(1, 18), // 7: Reward (was 9)
    Constraint::Ratio(1, 18), // 8: Err (was 10)
    Constraint::Ratio(4, 18), // 9: Rx Chart Area (was 11)
    Constraint::Ratio(4, 18), // 10: Tx Chart Area (was 12)
    Constraint::Ratio(1, 18), // 11: Status (was 13)
]; // Ratios adjusted to sum to 1 (9*1 + 2*4 + 1*1 = 18)

// --- NEW: Summary Gauges ---

/// Renders the summary section with gauges for CPU and Storage.
pub fn render_summary_gauges(f: &mut Frame, app: &App, area: Rect) {
    // Create a block for the summary section (optional, could be removed if no border needed)
    // let summary_block = Block::default().borders(Borders::NONE);
    // f.render_widget(summary_block, area);
    // let inner_area = summary_block.inner(area); // Use area directly if no block

    // Outer layout: Gauges | Spacer | Bandwidth | Spacer | Recs/Rwds | Spacer | Peers | Spacer
    let outer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // 1: Gauges area
            Constraint::Length(2),      // 2: Spacer
            Constraint::Percentage(27), // 3: Bandwidth area (Combined Data + Speed: 12 + 15)
            Constraint::Length(2),      // 4: Spacer
            Constraint::Percentage(12), // 5: Recs/Rwds Column
            Constraint::Length(2),      // 6: Spacer
            Constraint::Percentage(10), // 7: Peers Column
            Constraint::Min(0),         // 8: Remaining empty space
        ])
        .split(area);

    let gauges_area = outer_chunks[0];
    let bandwidth_area = outer_chunks[2]; // NEW: Combined area for bandwidth info (Index 2)
    let recs_rwds_col_area = outer_chunks[4]; // Index updated
    let peers_col_area = outer_chunks[6]; // Index updated

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
            let ratio = (used_bytes as f64 / allocated_bytes as f64).clamp(0.0, 1.0);
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

    // --- Bandwidth Area --- NEW STRUCTURE
    // Read pre-calculated totals directly from app state
    let formatted_data_in = format_option_u64_bytes(Some(app.summary_total_data_in_bytes));
    let formatted_data_out = format_option_u64_bytes(Some(app.summary_total_data_out_bytes));
    let total_in_speed_str = format_speed_bps(Some(app.summary_total_in_speed));
    let total_out_speed_str = format_speed_bps(Some(app.summary_total_out_speed));

    // Get chart data
    let total_in_chart_data: Vec<(f64, f64)> = app
        .total_speed_in_history
        .iter()
        .enumerate()
        .map(|(i, &val)| (i as f64, val as f64))
        .collect();

    let total_out_chart_data: Vec<(f64, f64)> = app
        .total_speed_out_history
        .iter()
        .enumerate()
        .map(|(i, &val)| (i as f64, val as f64))
        .collect();

    // Create charts
    let in_chart = create_summary_chart(&total_in_chart_data, Color::Cyan, "Total Rx");
    let out_chart = create_summary_chart(&total_out_chart_data, Color::Magenta, "Total Tx");

    // Layout for bandwidth section (Vertical: In row, Out row)
    let bandwidth_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Row for In
            Constraint::Length(1), // Row for Out
        ])
        .split(bandwidth_area); // Use the combined bandwidth_area

    // --- In Row ---
    let in_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(5),  // Label "In: "
            Constraint::Length(10), // Total Data (Right aligned)
            Constraint::Min(10),    // Chart (Takes remaining space)
            Constraint::Length(12), // Speed (Right aligned)
        ])
        .split(bandwidth_layout[0]);

    // Render "In:" label
    let in_label = Paragraph::new("In:").alignment(Alignment::Left);
    f.render_widget(in_label, in_row_layout[0]);

    // Render Total In Data
    let in_data_para = Paragraph::new(formatted_data_in)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(in_data_para, in_row_layout[1]);

    // Render In Chart
    if let Some(chart) = in_chart {
        f.render_widget(chart, in_row_layout[2]);
    } else {
        let placeholder = Paragraph::new("-")
            .style(DATA_CELL_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(placeholder, in_row_layout[2]);
    }

    // Render In Speed
    let in_speed_para = Paragraph::new(total_in_speed_str)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(in_speed_para, in_row_layout[3]);

    // --- Out Row ---
    let out_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(5),  // Label "Out: "
            Constraint::Length(10), // Total Data (Right aligned)
            Constraint::Min(10),    // Chart (Takes remaining space)
            Constraint::Length(12), // Speed (Right aligned)
        ])
        .split(bandwidth_layout[1]);

    // Render "Out:" label
    let out_label = Paragraph::new("Out:").alignment(Alignment::Left);
    f.render_widget(out_label, out_row_layout[0]);

    // Render Total Out Data
    let out_data_para = Paragraph::new(formatted_data_out)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(out_data_para, out_row_layout[1]);

    // Render Out Chart
    if let Some(chart) = out_chart {
        f.render_widget(chart, out_row_layout[2]);
    } else {
        let placeholder = Paragraph::new("-")
            .style(DATA_CELL_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(placeholder, out_row_layout[2]);
    }

    // Render Out Speed
    let out_speed_para = Paragraph::new(total_out_speed_str)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(out_speed_para, out_row_layout[3]);

    // --- Recs/Rwds Column Rendering (Use totals calculated earlier) ---
    let recs_rwds_col_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(recs_rwds_col_area);

    let recs_text = Line::from(vec![
        Span::styled("Recs:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_records), // Use pre-calculated value
            Style::default().fg(Color::White),
        ),
    ]);
    let rwds_text = Line::from(vec![
        Span::styled("Rwds:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_rewards), // Use pre-calculated value
            Style::default().fg(Color::Yellow),
        ),
    ]);

    f.render_widget(
        Paragraph::new(recs_text).alignment(Alignment::Left),
        recs_rwds_col_layout[0],
    );
    f.render_widget(
        Paragraph::new(rwds_text).alignment(Alignment::Left),
        recs_rwds_col_layout[1],
    );

    // --- Peers Column Rendering ---
    let peers_text = Line::from(vec![
        Span::styled("Peers:", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_live_peers), // Use pre-calculated value
            Style::default().fg(Color::Blue),
        ),
    ]);

    f.render_widget(
        Paragraph::new(peers_text).alignment(Alignment::Left),
        peers_col_area,
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
    f.render_widget(rx_title_paragraph, header_column_chunks[9]); // Rx is now index 9

    let tx_title_paragraph = Paragraph::new("Tx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(tx_title_paragraph, header_column_chunks[10]); // Tx is now index 10

    let status_title_paragraph = Paragraph::new("Status")
        .style(HEADER_STYLE)
        .alignment(Alignment::Right); // Align right like other data columns
    f.render_widget(status_title_paragraph, header_column_chunks[11]); // Status is now index 11
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
    let rx_area = column_chunks[9]; // Rx chart is now index 9
    let tx_area = column_chunks[10]; // Tx chart is now index 10
    let status_area = column_chunks[11]; // Status is now index 11

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

    // Render data cells (Node to Err) into chunks 0-8 (Indices 0-8 correspond to HEADER_TITLES)
    for (idx, cell_text) in data_cells.iter().take(9).enumerate() {
        // Take only the first 9 cells (0-8)
        let chunk_index = idx;

        // Determine the style based on the column index -- REMOVED, use default
        // let cell_style = match idx { ... };

        // Render into chunks 0-8
        let alignment = if idx == 0 {
            Alignment::Left
        } else {
            Alignment::Right
        }; // Align Node left
        let cell_paragraph = Paragraph::new(cell_text.clone())
            .style(DATA_CELL_STYLE) // Use default style
            .alignment(alignment);
        f.render_widget(cell_paragraph, column_chunks[chunk_index]);
    }

    // Render status separately in the new status column chunk
    let status_paragraph = Paragraph::new(status_text) // format! is redundant for String
        .style(style) // Use the determined style (Green/Yellow/Gray)
        .alignment(Alignment::Right);
    f.render_widget(status_paragraph, status_area); // Render in the dedicated status_area (index 11)

    // --- Render Rx/Tx Columns (TotalData Chart Speed Layout) ---

    // Extract all necessary data points for this node
    let (
        chart_data_in,
        chart_data_out,
        speed_in_bps,
        speed_out_bps,
        total_in_bytes,
        total_out_bytes,
    ) = metrics_result
        .and_then(|res| res.as_ref().ok()) // Get Option<&NodeMetrics>
        .map_or(
            (None, None, None, None, None, None), // Default if no metrics
            |m| {
                (
                    m.chart_data_in.as_deref(),
                    m.chart_data_out.as_deref(),
                    m.speed_in_bps,
                    m.speed_out_bps,
                    m.bandwidth_inbound_bytes,  // Extract total bytes
                    m.bandwidth_outbound_bytes, // Extract total bytes
                )
            },
        );

    // Format the extracted data
    let formatted_total_in = format_option_u64_bytes(total_in_bytes);
    let formatted_total_out = format_option_u64_bytes(total_out_bytes);
    let formatted_speed_in = format_speed_bps(speed_in_bps);
    let formatted_speed_out = format_speed_bps(speed_out_bps);

    // --- Rx Column Rendering ---
    let rx_col_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),  // Total Data (Right aligned)
            Constraint::Min(1), // Chart (Takes remaining space) - Ensure at least 1 for placeholder
            Constraint::Length(10), // Speed (Right aligned)
        ])
        .split(rx_area); // Split the rx_area (column_chunks[9])

    // Render Total In Data
    let total_in_para = Paragraph::new(formatted_total_in)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(total_in_para, rx_col_layout[0]);

    // Render In Chart (reuse create_summary_chart helper)
    if let Some(data) = chart_data_in {
        if let Some(chart) = create_summary_chart(data, Color::Cyan, "Rx") {
            f.render_widget(chart, rx_col_layout[1]);
        } else {
            // Handle case where create_summary_chart returns None (e.g., < 2 data points)
            let placeholder = Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center);
            f.render_widget(placeholder, rx_col_layout[1]);
        }
    } else {
        // Handle case where chart_data_in itself is None
        let placeholder = Paragraph::new("-")
            .style(DATA_CELL_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(placeholder, rx_col_layout[1]);
    }

    // Render In Speed
    let speed_in_para = Paragraph::new(formatted_speed_in)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(speed_in_para, rx_col_layout[2]);

    // --- Tx Column Rendering ---
    let tx_col_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),  // Total Data (Right aligned)
            Constraint::Min(1),     // Chart (Takes remaining space)
            Constraint::Length(10), // Speed (Right aligned)
        ])
        .split(tx_area); // Split the tx_area (column_chunks[10])

    // Render Total Out Data
    let total_out_para = Paragraph::new(formatted_total_out)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(total_out_para, tx_col_layout[0]);

    // Render Out Chart (reuse create_summary_chart helper)
    if let Some(data) = chart_data_out {
        if let Some(chart) = create_summary_chart(data, Color::Magenta, "Tx") {
            f.render_widget(chart, tx_col_layout[1]);
        } else {
            let placeholder = Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center);
            f.render_widget(placeholder, tx_col_layout[1]);
        }
    } else {
        let placeholder = Paragraph::new("-")
            .style(DATA_CELL_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(placeholder, tx_col_layout[1]);
    }

    // Render Out Speed
    let speed_out_para = Paragraph::new(formatted_speed_out)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(speed_out_para, tx_col_layout[2]);
}
