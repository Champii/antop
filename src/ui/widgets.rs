use super::formatters::{
    create_list_item_cells, create_placeholder_cells, format_option_u64_bytes, format_speed_bps,
};
use crate::app::App;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
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

// New constraints with fixed width for data columns and expanding charts
pub const COLUMN_CONSTRAINTS: [Constraint; 14] = [
    Constraint::Length(20), // 0: Node
    Constraint::Length(12), // 1: Uptime
    Constraint::Length(9),  // 2: Mem MB
    Constraint::Length(8),  // 3: CPU %
    Constraint::Length(6),  // 4: Peers (Live)
    Constraint::Length(8),  // 5: Routing
    Constraint::Length(7),  // 6: Records
    Constraint::Length(7),  // 7: Reward
    Constraint::Length(6),  // 8: Err
    Constraint::Length(1),  // 9: Spacer 1
    Constraint::Min(1),     // 10: Rx Chart Area (EXPANDS)
    Constraint::Length(1),  // 11: Spacer 2
    Constraint::Min(1),     // 12: Tx Chart Area (EXPANDS)
    Constraint::Length(10), // 13: Status
];

// --- Helper Functions ---

/// Returns a color based on the CPU usage percentage.
pub fn get_cpu_color(percentage: f64) -> Color {
    if percentage >= 75.0 {
        Color::Magenta // Very High
    } else if percentage >= 50.0 {
        Color::Red // High
    } else if percentage >= 25.0 {
        Color::Rgb(255, 165, 0) // Medium-High (Orange)
    } else if percentage >= 10.0 {
        Color::Yellow // Moderate
    } else {
        Color::Green // Low
    }
}

// --- NEW: Summary Gauges ---

/// Renders the summary section with gauges for CPU and Storage.
pub fn render_summary_gauges(f: &mut Frame, app: &App, area: Rect) {
    // FINAL Layout: Gauges | Spacer | Peers | Spacer | Bandwidth (Expands) | Spacer | Recs/Rwds
    let outer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // 0: Gauges (CPU/Storage)
            Constraint::Length(2),      // 1: Spacer
            Constraint::Length(10),     // 2: Peers (Fixed width)
            Constraint::Length(2),      // 3: Spacer
            Constraint::Min(0),         // 4: Bandwidth (Expands to fill, align w/ Rx/Tx)
            Constraint::Length(2),      // 5: Spacer
            Constraint::Length(10),     // 6: Recs/Rwds (Fixed width, align w/ Status)
        ])
        .split(area);

    let gauges_area = outer_chunks[0];
    let peers_area = outer_chunks[2]; // Peers area
    let bandwidth_area = outer_chunks[4]; // Bandwidth area (Expands)
    let recs_rwds_area = outer_chunks[6]; // Recs/Rwds area

    // --- 1. Gauges Rendering (Rendered into gauges_area) ---
    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(gauges_area);

    // --- CPU Gauge ---
    let cpu_percentage = app.total_cpu_usage;
    let cpu_color = get_cpu_color(cpu_percentage);
    let cpu_label = Span::styled(
        format!("CPU {:.2}%", cpu_percentage),
        Style::default().fg(cpu_color),
    )
    .bold();
    let cpu_gauge = Gauge::default()
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
        Some(_) => (
            0.0,
            Span::styled(
                format!("0 / {}", allocated_formatted),
                Style::default().fg(Color::Green),
            ),
        ),
        None => (
            0.0,
            Span::styled("Error".to_string(), Style::default().fg(Color::Red)),
        ),
    };
    let storage_gauge = Gauge::default()
        .gauge_style(Color::Black)
        .ratio(storage_ratio)
        .label(storage_label);
    f.render_widget(storage_gauge, gauge_chunks[1]);

    // --- 2. Peers Column Rendering (Rendered into peers_area) ---
    let peers_text = Line::from(vec![
        Span::styled("Peers: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_live_peers),
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
    ]);
    f.render_widget(
        Paragraph::new(peers_text).alignment(Alignment::Left),
        peers_area,
    );

    // --- 3. Bandwidth Area Rendering (Rendered into bandwidth_area) ---
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
        .split(bandwidth_area); // Use the correct area variable

    // --- In Row ---
    let in_row_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(5),  // Label "In:"
            Constraint::Length(10), // Data (Bytes)
            Constraint::Length(1),  // Spacer
            Constraint::Min(1),     // Chart
            Constraint::Length(1),  // Spacer
            Constraint::Length(10), // Speed
        ])
        .split(bandwidth_layout[0]);

    let in_label = Paragraph::new("In:").alignment(Alignment::Left);
    f.render_widget(in_label, in_row_layout[0]);
    let in_data_para = Paragraph::new(formatted_data_in)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    f.render_widget(in_data_para, in_row_layout[1]);
    if let Some(chart) = in_chart {
        f.render_widget(chart, in_row_layout[3]);
    } else {
        f.render_widget(
            Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center),
            in_row_layout[3],
        );
    }
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
            Constraint::Min(1),     // Chart
            Constraint::Length(1),  // Spacer
            Constraint::Length(10), // Speed
        ])
        .split(bandwidth_layout[1]);

    let out_label = Paragraph::new("Out:").alignment(Alignment::Left);
    f.render_widget(out_label, out_row_layout[0]);
    let out_data_para = Paragraph::new(formatted_data_out)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(out_data_para, out_row_layout[1]);
    if let Some(chart) = out_chart {
        f.render_widget(chart, out_row_layout[3]);
    } else {
        f.render_widget(
            Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center),
            out_row_layout[3],
        );
    }
    let out_speed_para = Paragraph::new(total_out_speed_str)
        .style(Style::default().fg(Color::Magenta))
        .alignment(Alignment::Right);
    f.render_widget(out_speed_para, out_row_layout[5]);

    // --- 4. Recs/Rwds Column Rendering (Rendered into recs_rwds_area) ---
    let recs_rwds_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(recs_rwds_area);

    let recs_text = Line::from(vec![
        Span::styled("Recs: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_records),
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
    ]);
    let rwds_text = Line::from(vec![
        Span::styled("Rwds: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", app.summary_total_rewards),
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
    ]);

    f.render_widget(
        Paragraph::new(recs_text).alignment(Alignment::Left),
        recs_rwds_layout[0],
    );
    f.render_widget(
        Paragraph::new(rwds_text).alignment(Alignment::Left),
        recs_rwds_layout[1],
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
                None => {
                    // URL exists but no entry in metrics map yet (should be rare after init)
                    (
                        create_placeholder_cells(dir_path),
                        "Initializing".to_string(),
                        Style::default().fg(Color::Yellow),
                        None, // No metrics result available
                    )
                }
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

    // --- Render Rx/Tx Columns (Indices 10, 12) --- Get data first ---
    let (
        cpu_usage_percentage_opt,
        chart_data_in,
        chart_data_out,
        speed_in_bps,
        speed_out_bps,
        total_in_bytes,
        total_out_bytes,
    ) = metrics_option // Use the metrics_option determined above
        .and_then(|res| res.ok()) // Get NodeMetrics only if the result was Ok
        .map_or((None, None, None, None, None, None, None), |m| {
            (
                Some(m.cpu_usage_percentage),
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

    // --- Render Data Cells (Indices 0..=8) ---
    for (i, cell_content) in cells.iter().enumerate() {
        let chunk_index = i;
        if chunk_index < column_layout.len() {
            let alignment = if i == 0 {
                Alignment::Left
            } else {
                Alignment::Right
            };

            // Determine style: special for CPU (index 3), default otherwise
            let style = if i == 3 {
                // Index 3 is CPU
                match cpu_usage_percentage_opt {
                    Some(Some(percent)) => Style::default().fg(get_cpu_color(percent)), // Inner Option is Some(f64)
                    Some(None) => DATA_CELL_STYLE, // Inner Option is None (metric exists but CPU is None)
                    None => DATA_CELL_STYLE,       // Outer Option is None (no metrics result)
                }
            } else {
                // Other columns use default data style
                DATA_CELL_STYLE
            };

            // Add space suffix EXCEPT for the Err column (index 8)
            let cell_text = if i != 8 {
                format!("{} ", cell_content)
            } else {
                cell_content.clone()
            };

            let cell_paragraph = Paragraph::new(cell_text).style(style).alignment(alignment);
            f.render_widget(cell_paragraph, column_layout[chunk_index]);
        }
    }

    // --- Rx Column Rendering (Index 10) ---
    let rx_col_index = 10;
    if rx_col_index < column_layout.len() {
        // Restore original internal layout for Rx
        let rx_col_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(8), // Total Bytes
                Constraint::Length(1), // Spacer
                Constraint::Min(1),    // Chart
                Constraint::Length(1), // Spacer
                Constraint::Length(8), // Speed
            ])
            .split(column_layout[rx_col_index]);

        // Render widgets into correct chunks (0, 1, 2)
        let total_in_para = Paragraph::new(formatted_total_in)
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Right);
        f.render_widget(total_in_para, rx_col_layout[0]); // Bytes in chunk 0

        if let Some(data) = chart_data_in {
            if let Some(chart) = create_summary_chart(data, Color::Cyan, "Rx") {
                f.render_widget(chart, rx_col_layout[2]); // Chart in chunk 2 (was 1)
            } else {
                let placeholder = Paragraph::new("-")
                    .style(DATA_CELL_STYLE)
                    .alignment(Alignment::Center);
                f.render_widget(placeholder, rx_col_layout[2]); // Placeholder in chunk 2 (was 1)
            }
        } else {
            let placeholder = Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center);
            f.render_widget(placeholder, rx_col_layout[2]); // Placeholder in chunk 2 (was 1)
        }

        let speed_in_para = Paragraph::new(formatted_speed_in)
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Right);
        f.render_widget(speed_in_para, rx_col_layout[4]); // Speed in chunk 4 (was 2)
    }

    // --- Tx Column Rendering (Index 12) ---
    let tx_col_index = 12;
    if tx_col_index < column_layout.len() {
        // Restore original internal layout for Tx
        let tx_col_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(8), // Total Bytes
                Constraint::Length(1), // Spacer
                Constraint::Min(1),    // Chart
                Constraint::Length(1), // Spacer
                Constraint::Length(8), // Speed
            ])
            .split(column_layout[tx_col_index]);

        // Render widgets into correct chunks (0, 1, 2)
        let total_out_para = Paragraph::new(formatted_total_out)
            .style(Style::default().fg(Color::Magenta))
            .alignment(Alignment::Right);
        f.render_widget(total_out_para, tx_col_layout[0]); // Bytes in chunk 0

        if let Some(data) = chart_data_out {
            if let Some(chart) = create_summary_chart(data, Color::Magenta, "Tx") {
                f.render_widget(chart, tx_col_layout[2]); // Chart in chunk 2 (was 1)
            } else {
                let placeholder = Paragraph::new("-")
                    .style(DATA_CELL_STYLE)
                    .alignment(Alignment::Center);
                f.render_widget(placeholder, tx_col_layout[2]); // Placeholder in chunk 2 (was 1)
            }
        } else {
            let placeholder = Paragraph::new("-")
                .style(DATA_CELL_STYLE)
                .alignment(Alignment::Center);
            f.render_widget(placeholder, tx_col_layout[2]); // Placeholder in chunk 2 (was 1)
        }

        let speed_out_para = Paragraph::new(formatted_speed_out)
            .style(Style::default().fg(Color::Magenta))
            .alignment(Alignment::Right);
        f.render_widget(speed_out_para, tx_col_layout[4]); // Speed in chunk 4 (was 2)
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
