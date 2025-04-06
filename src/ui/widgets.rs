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
    text::Line,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
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
