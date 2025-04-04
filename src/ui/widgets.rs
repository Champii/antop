use super::formatters::{
    // Use super to access sibling module
    create_list_item_cells,
    create_placeholder_cells,
    format_speed_bps,
};
use crate::{app::App, metrics::NodeMetrics};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols,
    text::Line, // Added import for Line
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

// --- Constants ---

const HEADER_TITLES: [&str; 11] = [
    "Node",
    "Uptime",
    "Mem",
    "CPU",
    "Peers",
    "Total In",
    "Total Out",
    "Recs",
    "Rwds",
    "Err",
    "Status",
];
const HEADER_STYLE: Style = Style::new().fg(Color::Yellow); // Use Style::new() for const
const DATA_CELL_STYLE: Style = Style::new().fg(Color::Gray); // Use Style::new() for const

// Define column widths based on header comment (line 33 in original ui.rs) + Status
// These must match the constraints used for data rows
// Define column widths including charts for even distribution
// 11 text columns + 2 chart columns = 13 total
pub const COLUMN_CONSTRAINTS: [Constraint; 13] = [
    Constraint::Ratio(1, 19), // Node
    Constraint::Ratio(1, 19), // Uptime
    Constraint::Ratio(1, 19), // Mem MB
    Constraint::Ratio(1, 19), // CPU %
    Constraint::Ratio(1, 19), // Peers
    Constraint::Ratio(1, 19), // BW In
    Constraint::Ratio(1, 19), // BW Out
    Constraint::Ratio(1, 19), // Records
    Constraint::Ratio(1, 19), // Reward
    Constraint::Ratio(1, 19), // Err
    Constraint::Ratio(1, 19), // Status
    Constraint::Ratio(4, 19), // Rx Chart Area - Now 4x proportional
    Constraint::Ratio(4, 19), // Tx Chart Area - Now 4x proportional
];

// --- Rendering Helpers ---

/// Renders the header row with column titles.
pub fn render_header(f: &mut Frame, area: Rect) {
    // Split the entire header area into columns using the unified constraints
    let header_column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the updated constant for all columns
        .split(area);

    // Render each header title in its respective column chunk
    for (i, title) in HEADER_TITLES.iter().enumerate() {
        // Render text titles into the first 11 columns
        if i < HEADER_TITLES.len() {
            // Check against HEADER_TITLES length
            let alignment = if i == 5 || i == 6 {
                // Indices for "Total In" and "Total Out"
                Alignment::Right
            } else {
                Alignment::Left
            };
            let title_paragraph = Paragraph::new(*title)
                .style(HEADER_STYLE)
                .alignment(alignment);
            f.render_widget(title_paragraph, header_column_chunks[i]);
        }
    }

    // Render Rx title in the second chunk (chart area)
    // Render Rx title in the 12th column chunk (index 11)
    let rx_title_paragraph = Paragraph::new("Rx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(rx_title_paragraph, header_column_chunks[11]);

    // Render Tx title in the 13th column chunk (index 12)
    let tx_title_paragraph = Paragraph::new("Tx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(tx_title_paragraph, header_column_chunks[12]);
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
    // Split area: 70% chart, 30% speed
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
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
        f.render_widget(placeholder, area); // Use the whole area
    }
}

/// Renders a single node's data row, including text cells and bandwidth charts.
pub fn render_node_row(f: &mut Frame, app: &App, area: Rect, name: &str, url: &str) {
    // Split the entire row area using the unified constraints
    let column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the updated constant for all columns
        .split(area);

    // Assign areas based on the new layout
    // Indices 0-10 are text columns
    let rx_area = column_chunks[11]; // 12th column is Rx chart
    let tx_area = column_chunks[12]; // 13th column is Tx chart

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

    // Render each data cell in its column chunk
    // Render each data cell in its corresponding column chunk (first 11 columns)
    // Note: data_cells contains 11 items (including name)
    for (idx, cell_text) in data_cells.iter().enumerate() {
        // Render into columns 0 through 10 (inclusive)
        if idx < 11 {
            let alignment = if idx == 5 || idx == 6 {
                // Indices for "Total In" and "Total Out"
                Alignment::Right
            } else {
                Alignment::Left // Explicitly set left for others
            };
            let cell_paragraph = Paragraph::new(cell_text.clone())
                .style(DATA_CELL_STYLE)
                .alignment(alignment);
            f.render_widget(cell_paragraph, column_chunks[idx]);
        }
    }

    // Render status in the 11th column chunk (index 10)
    let status_paragraph = Paragraph::new(format!("{:<10}", status_text)).style(style); // Pad status
    f.render_widget(status_paragraph, column_chunks[10]); // Status is still index 10

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
