use super::formatters::{
    create_list_item_cells, create_placeholder_cells, format_option_u64_bytes, format_speed_bps,
};
use crate::app::App;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, Gauge, GraphType, Paragraph},
};

// --- Constants ---

const HEADER_TITLES: [&str; 9] = [
    "Node", "Uptime", "Mem", "CPU", "Peers",   // Live Peers
    "Routing", // Routing Table Size
    "Recs", "Rwds", "Err",
];
const HEADER_STYLE: Style = Style::new().fg(Color::Yellow);
const DATA_CELL_STYLE: Style = Style::new().fg(Color::Gray);

// New constraints with spacer after Err (index 9) and after Rx (index 11)
pub const COLUMN_CONSTRAINTS: [Constraint; 14] = [
    Constraint::Ratio(1, 20), // 0: Node
    Constraint::Ratio(1, 20), // 1: Uptime
    Constraint::Ratio(1, 20), // 2: Mem MB
    Constraint::Ratio(1, 20), // 3: CPU %
    Constraint::Ratio(1, 20), // 4: Peers (Live)
    Constraint::Ratio(1, 20), // 5: Routing
    Constraint::Ratio(1, 20), // 6: Records
    Constraint::Ratio(1, 20), // 7: Reward
    Constraint::Ratio(1, 20), // 8: Err
    Constraint::Length(1),    // 9: Spacer 1
    Constraint::Ratio(3, 20), // 10: Rx Chart Area
    Constraint::Length(1),    // 11: Spacer 2
    Constraint::Ratio(3, 20), // 12: Tx Chart Area
    Constraint::Ratio(1, 20), // 13: Status
]; // Ratios adjusted: 9*1 + 2*3 + 1*1 + 2 spacers = 17 units + spacers. Total ratio base = 20

// --- NEW: Summary Gauges ---

/// Renders the summary section with gauges for CPU and Storage.
pub fn render_summary_gauges(f: &mut Frame, app: &App, area: Rect) {
    // Outer layout: Gauges | Spacer | Bandwidth | Spacer | Recs/Rwds | Spacer | Peers | Spacer
    let outer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Length(2),
            Constraint::Percentage(27),
            Constraint::Length(2),
            Constraint::Percentage(12),
            Constraint::Length(2),
            Constraint::Percentage(10),
            Constraint::Min(0),
        ])
        .split(area);

    let gauges_area = outer_chunks[0];
    let bandwidth_area = outer_chunks[2];
    let recs_rwds_col_area = outer_chunks[4];
    let peers_col_area = outer_chunks[6];

    // Inner layout: Stack gauges vertically within the gauges_area
    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(gauges_area);

    // --- CPU Gauge ---
    let cpu_percentage = app.total_cpu_usage;

    let cpu_label = Span::styled(
        format!("CPU {:.2}%", cpu_percentage),
        Style::default().fg(Color::Blue),
    );
    let cpu_gauge = Gauge::default()
        // .block(Block::default().title(Span::styled("CPU", Style::new().bold())))
        .gauge_style(Color::Black)
        .ratio(cpu_percentage / 100.0)
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

    let storage_gauge = Gauge::default()
        // .block(Block::default().title(Span::styled("Store", Style::new().bold())))
        .gauge_style(Color::Black)
        .ratio(storage_ratio)
        .label(storage_label);
    f.render_widget(storage_gauge, gauge_chunks[1]);

    // --- Bandwidth Area ---
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

    let in_chart = create_summary_chart(&total_in_chart_data, Color::Cyan, "Total Rx");
    let out_chart = create_summary_chart(&total_out_chart_data, Color::Magenta, "Total Tx");

    let bandwidth_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(bandwidth_area);

    // --- In Row ---
    let in_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(5),  // Label "In:"
            Constraint::Length(10), // Data (Bytes)
            Constraint::Length(1),  // Spacer
            Constraint::Min(9),     // Chart (Reduced Min width by 2 for spacers)
            Constraint::Length(1),  // Spacer
            Constraint::Length(12), // Speed
        ])
        .split(bandwidth_layout[0]);

    let in_label = Paragraph::new("In:").alignment(Alignment::Left);
    f.render_widget(in_label, in_row_layout[0]);

    let in_data_para = Paragraph::new(formatted_data_in)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(in_data_para, in_row_layout[1]);

    // Chart in chunk 3 (after label, data, spacer)
    if let Some(chart) = in_chart {
        f.render_widget(chart, in_row_layout[3]);
    } else {
        let placeholder = Paragraph::new("-")
            .style(DATA_CELL_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(placeholder, in_row_layout[3]);
    }

    // Speed in chunk 5 (after chart and spacer)
    let in_speed_para = Paragraph::new(total_in_speed_str)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(in_speed_para, in_row_layout[5]);

    // --- Out Row ---
    let out_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(5),  // Label "Out:"
            Constraint::Length(10), // Data (Bytes)
            Constraint::Length(1),  // Spacer
            Constraint::Min(9),     // Chart (Reduced Min width by 2 for spacers)
            Constraint::Length(1),  // Spacer
            Constraint::Length(12), // Speed
        ])
        .split(bandwidth_layout[1]);

    let out_label = Paragraph::new("Out:").alignment(Alignment::Left);
    f.render_widget(out_label, out_row_layout[0]);

    let out_data_para = Paragraph::new(formatted_data_out)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(out_data_para, out_row_layout[1]);

    // Chart in chunk 3
    if let Some(chart) = out_chart {
        f.render_widget(chart, out_row_layout[3]);
    } else {
        let placeholder = Paragraph::new("-")
            .style(DATA_CELL_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(placeholder, out_row_layout[3]);
    }

    // Speed in chunk 5
    let out_speed_para = Paragraph::new(total_out_speed_str)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(out_speed_para, out_row_layout[5]);

    // --- Recs/Rwds Column Rendering ---
    let recs_rwds_col_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(recs_rwds_col_area);

    let recs_text = Line::from(vec![
        Span::styled("Recs: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_records),
            Style::default().fg(Color::White),
        ),
    ]);
    let rwds_text = Line::from(vec![
        Span::styled("Rwds: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_rewards),
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
        Span::styled("Peers: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_live_peers),
            Style::default().fg(Color::Blue),
        ),
    ]);

    f.render_widget(
        Paragraph::new(peers_text).alignment(Alignment::Left),
        peers_col_area,
    );
}

// Helper function to create summary charts consistently
fn create_summary_chart<'a>(
    data: &'a [(f64, f64)],
    color: Color,
    name: &'a str,
) -> Option<Chart<'a>> {
    if data.len() < 2 {
        // Not enough data to draw a line
        return None;
    }

    let max_len = data.len();
    let max_y = data
        .iter()
        .map(|&(_, y)| y)
        .fold(0.0f64, |max, y| max.max(y));

    let x_bounds = [0.0, (max_len.saturating_sub(1)).max(1) as f64];
    let y_bounds = [0.0, max_y.max(1.0)];

    let dataset = Dataset::default()
        .name(name)
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(data);

    let chart = Chart::new(vec![dataset])
        // .block(Block::default().borders(Borders::NONE))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::Black))
                .bounds(x_bounds)
                .labels(vec![]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::Black))
                .bounds(y_bounds)
                .labels(vec![]),
        );

    Some(chart)
}

/// Renders the header row with column titles.
pub fn render_header(f: &mut Frame, area: Rect) {
    let header_column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the NEW constraints (14 total)
        .split(area);

    // Render original titles with spacing added manually
    for (i, title) in HEADER_TITLES.iter().enumerate() {
        let chunk_index = i;
        let is_last_data_col = i == HEADER_TITLES.len() - 1; // Check if it's the last *data* title ("Err")

        if chunk_index < header_column_chunks.len() {
            let alignment = if i == 0 {
                Alignment::Left // Node title left-aligned
            } else {
                Alignment::Right // Other titles right-aligned
            };
            // Add a space for separation after each title, unless it's the last data col
            let title_text = if !is_last_data_col {
                format!("{} ", title)
            } else {
                title.to_string()
            };
            let title_paragraph = Paragraph::new(title_text)
                .style(HEADER_STYLE)
                .alignment(alignment);
            f.render_widget(title_paragraph, header_column_chunks[chunk_index]);
        }
    }

    // Render Rx, Tx, Status titles (Indices 10, 12, 13)
    let rx_index = 10;
    let tx_index = 12;
    let status_index = 13;

    if rx_index < header_column_chunks.len() {
        let rx_title_paragraph = Paragraph::new("Rx ")
            .style(HEADER_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(rx_title_paragraph, header_column_chunks[rx_index]);
    }

    if tx_index < header_column_chunks.len() {
        let tx_title_paragraph = Paragraph::new("Tx ")
            .style(HEADER_STYLE)
            .alignment(Alignment::Center);
        f.render_widget(tx_title_paragraph, header_column_chunks[tx_index]);
    }

    if status_index < header_column_chunks.len() {
        let status_title_paragraph = Paragraph::new("Status")
            .style(HEADER_STYLE)
            .alignment(Alignment::Right);
        f.render_widget(status_title_paragraph, header_column_chunks[status_index]);
    }
}

/// Renders a single node's data row, including text cells and bandwidth charts.
pub fn render_node_row(
    f: &mut Frame,
    app: &App,
    area: Rect,
    dir_path: &str,
    url_option: Option<&String>,
) {
    let column_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the NEW constraints (14 total)
        .split(area);

    // Determine metrics, status text, and style based on URL presence and metrics map
    let (cells, status_text, status_style, metrics_option) = match url_option {
        Some(url) => {
            // URL exists, try to get metrics
            match app.node_metrics.get(url) {
                Some(Ok(metrics)) => (
                    create_list_item_cells(dir_path, metrics),
                    "Running".to_string(),
                    Style::default().fg(Color::Green),
                    Some(Ok(metrics)), // Pass the successful metrics result
                ),
                Some(Err(e)) => (
                    create_placeholder_cells(dir_path),
                    // Display the first part of the error message as status
                    e.split_whitespace().next().unwrap_or("Error").to_string(),
                    Style::default().fg(Color::Red),
                    Some(Err(e)), // Pass the error result
                ),
                None => (
                    // URL exists but no entry in metrics map yet (should be rare after init)
                    create_placeholder_cells(dir_path),
                    "Initializing".to_string(),
                    Style::default().fg(Color::Yellow),
                    None, // No metrics result available
                ),
            }
        }
        None => {
            // No URL found for this directory path
            (
                create_placeholder_cells(dir_path),
                "Stopped".to_string(),
                Style::default().fg(Color::DarkGray),
                None, // No metrics result available
            )
        }
    };

    // Place data cells (indices 0..=8)
    for (i, cell_content) in cells.iter().enumerate() {
        let chunk_index = i;
        if chunk_index < column_layout.len() {
            let alignment = if i == 0 {
                Alignment::Left
            } else {
                Alignment::Right
            };
            // Add space suffix EXCEPT for the Err column (index 8)
            let cell_text = if i != 8 {
                // Don't add space after Err column
                format!("{} ", cell_content)
            } else {
                cell_content.clone()
            };
            let cell_paragraph = Paragraph::new(cell_text)
                .style(DATA_CELL_STYLE)
                .alignment(alignment);
            f.render_widget(cell_paragraph, column_layout[chunk_index]);
        }
    }

    // --- Render Rx/Tx Columns (Indices 10, 12) --- Get data first ---
    let (
        chart_data_in,
        chart_data_out,
        speed_in_bps,
        speed_out_bps,
        total_in_bytes,
        total_out_bytes,
    ) = metrics_option // Use the metrics_option determined above
        .and_then(|res| res.ok()) // Get NodeMetrics only if the result was Ok
        .map_or((None, None, None, None, None, None), |m| {
            (
                m.chart_data_in.as_deref(),
                m.chart_data_out.as_deref(),
                m.speed_in_bps,
                m.speed_out_bps,
                m.bandwidth_inbound_bytes,
                m.bandwidth_outbound_bytes,
            )
        });

    let formatted_total_in = format_option_u64_bytes(total_in_bytes);
    let formatted_total_out = format_option_u64_bytes(total_out_bytes);
    let formatted_speed_in = format_speed_bps(speed_in_bps);
    let formatted_speed_out = format_speed_bps(speed_out_bps);

    // --- Rx Column Rendering (Index 10) ---
    let rx_col_index = 10;
    if rx_col_index < column_layout.len() {
        // Restore original internal layout for Rx
        let rx_col_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(8),  // Total Bytes
                Constraint::Min(1),     // Chart
                Constraint::Length(10), // Speed
            ])
            .split(column_layout[rx_col_index]);

        // Render widgets into correct chunks (0, 1, 2)
        let total_in_para = Paragraph::new(formatted_total_in)
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Right);
        f.render_widget(total_in_para, rx_col_layout[0]); // Bytes in chunk 0

        if let Some(data) = chart_data_in {
            if let Some(chart) = create_summary_chart(data, Color::Cyan, "Rx") {
                f.render_widget(chart, rx_col_layout[1]); // Chart in chunk 1
            } else {
                let placeholder = Paragraph::new("-")
                    .style(DATA_CELL_STYLE)
                    .alignment(Alignment::Center);
                f.render_widget(placeholder, rx_col_layout[1]); // Placeholder in chunk 1
            }
        } else {
            let placeholder = Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center);
            f.render_widget(placeholder, rx_col_layout[1]); // Placeholder in chunk 1
        }

        let speed_in_para = Paragraph::new(formatted_speed_in)
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Right);
        f.render_widget(speed_in_para, rx_col_layout[2]); // Speed in chunk 2
    }

    // --- Tx Column Rendering (Index 12) ---
    let tx_col_index = 12;
    if tx_col_index < column_layout.len() {
        // Restore original internal layout for Tx
        let tx_col_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(8),  // Total Bytes
                Constraint::Min(1),     // Chart
                Constraint::Length(10), // Speed
            ])
            .split(column_layout[tx_col_index]);

        // Render widgets into correct chunks (0, 1, 2)
        let total_out_para = Paragraph::new(formatted_total_out)
            .style(Style::default().fg(Color::Magenta))
            .alignment(Alignment::Right);
        f.render_widget(total_out_para, tx_col_layout[0]); // Bytes in chunk 0

        if let Some(data) = chart_data_out {
            if let Some(chart) = create_summary_chart(data, Color::Magenta, "Tx") {
                f.render_widget(chart, tx_col_layout[1]); // Chart in chunk 1
            } else {
                let placeholder = Paragraph::new("-")
                    .style(DATA_CELL_STYLE)
                    .alignment(Alignment::Center);
                f.render_widget(placeholder, tx_col_layout[1]); // Placeholder in chunk 1
            }
        } else {
            let placeholder = Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center);
            f.render_widget(placeholder, tx_col_layout[1]); // Placeholder in chunk 1
        }

        let speed_out_para = Paragraph::new(formatted_speed_out)
            .style(Style::default().fg(Color::Magenta))
            .alignment(Alignment::Right);
        f.render_widget(speed_out_para, tx_col_layout[2]); // Speed in chunk 2
    }

    // --- Status Column Rendering (Index 13) ---
    let status_index = 13;
    if status_index < column_layout.len() {
        let status_paragraph = Paragraph::new(status_text)
            .style(status_style)
            .alignment(Alignment::Right);
        f.render_widget(status_paragraph, column_layout[status_index]);
    }
}
