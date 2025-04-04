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
// Define column widths including charts for even distribution + gap
// 11 text columns + 1 gap column + 2 chart columns = 14 total
pub const COLUMN_CONSTRAINTS: [Constraint; 14] = [
    Constraint::Ratio(1, 19), // 0: Node
    Constraint::Ratio(1, 19), // 1: Uptime
    Constraint::Ratio(1, 19), // 2: Mem MB
    Constraint::Ratio(1, 19), // 3: CPU %
    Constraint::Ratio(1, 19), // 4: Peers
    Constraint::Ratio(1, 19), // 5: BW In
    Constraint::Ratio(1, 19), // 6: BW Out
    Constraint::Ratio(1, 19), // 7: Records
    Constraint::Ratio(1, 19), // 8: Reward
    Constraint::Length(1),    // 9: << Gap >>
    Constraint::Ratio(1, 19), // 10: Err (was 9)
    Constraint::Ratio(1, 19), // 11: Status (was 10)
    Constraint::Ratio(4, 19), // 12: Rx Chart Area (was 11)
    Constraint::Ratio(4, 19), // 13: Tx Chart Area (was 12)
];

// --- Rendering Helpers ---

/// Renders the header row with column titles.
pub fn render_header(f: &mut Frame, area: Rect) {
    // Split the entire header area into columns using the unified constraints
    let header_column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the updated constant for all columns
        .split(area);

    // Render each header title in its respective column chunk, accounting for the gap
    for (i, title) in HEADER_TITLES.iter().enumerate() {
        // Determine the correct chunk index, skipping the gap column (index 9)
        let chunk_index = if i <= 8 {
            i // Node (0) to Rwds (8) map directly
        } else {
            i + 1 // Err (9) maps to chunk 10, Status (10) maps to chunk 11
        };

        if chunk_index < header_column_chunks.len() {
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

    // Render Rx title in the second chunk (chart area)
    // Render Rx title in the 12th column chunk (index 11)
    let rx_title_paragraph = Paragraph::new("Rx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(rx_title_paragraph, header_column_chunks[12]); // Shifted due to gap

    // Render Tx title in the 13th column chunk (index 12)
    let tx_title_paragraph = Paragraph::new("Tx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center);
    f.render_widget(tx_title_paragraph, header_column_chunks[13]); // Shifted due to gap
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
    // Indices 0-8, 10-11 are text columns (index 9 is the gap)
    let rx_area = column_chunks[12]; // 13th column is Rx chart (was 11)
    let tx_area = column_chunks[13]; // 14th column is Tx chart (was 12)

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
    // Render each data cell in its corresponding column chunk, accounting for the gap
    // Note: data_cells contains 11 items (matching HEADER_TITLES)
    for (idx, cell_text) in data_cells.iter().enumerate() {
        // Determine the correct chunk index, skipping the gap column (index 9)
        let chunk_index = if idx <= 8 {
            idx // Node (0) to Rwds (8) map directly
        } else {
            idx + 1 // Err (9) maps to chunk 10, Status (10) maps to chunk 11
        };

        if chunk_index < 13 {
            let alignment = Alignment::Right;
            let cell_paragraph = Paragraph::new(cell_text.clone())
                .style(DATA_CELL_STYLE)
                .alignment(alignment);
            f.render_widget(cell_paragraph, column_chunks[chunk_index]);
        }
    }

    // Render status in the 11th column chunk (index 10)
    // Status text is the 11th item in data_cells (index 10), rendered into chunk 11
    // No need to render it separately anymore as the loop above handles it.
    // We can remove these lines if status is already included in data_cells.
    // Let's double-check formatters.rs to confirm if status is part of data_cells.
    // Assuming status is NOT part of data_cells and needs separate rendering:
    let status_paragraph = Paragraph::new(format!("{}", status_text))
        .style(style)
        .alignment(Alignment::Right); // Pad status
    f.render_widget(status_paragraph, column_chunks[11]); // Status is now index 11

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
