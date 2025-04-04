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
pub const COLUMN_CONSTRAINTS: [Constraint; 12] = [
    Constraint::Length(18), // Node
    Constraint::Length(12), // Uptime
    Constraint::Length(6),  // Mem MB
    Constraint::Length(5),  // CPU %
    Constraint::Length(7),  // Peers
    Constraint::Length(10), // BW In
    Constraint::Length(10), // BW Out
    Constraint::Length(7),  // Records
    Constraint::Length(8),  // Reward
    Constraint::Length(6),  // Err
    Constraint::Length(10), // Status
    Constraint::Min(0),     // Spacer to fill text_area
];

// --- Rendering Helpers ---

/// Renders the header row with column titles.
pub fn render_header(f: &mut Frame, area: Rect) {
    // Split the header row area horizontally like the data rows (60% text, 20% Rx, 20% Tx)
    // This ensures the header titles align with the data columns below.
    let header_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Text area for header titles
            Constraint::Percentage(20), // Empty space corresponding to Rx chart area
            Constraint::Percentage(20), // Empty space corresponding to Tx chart area
        ])
        .split(area); // Split the entire header row area

    let header_text_area = header_row_chunks[0]; // The 60% area where titles will go

    // Split the header text area into columns using the defined constraints
    let header_column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the constant directly
        .split(header_text_area);

    // Render each header title in its respective column chunk
    for (i, title) in HEADER_TITLES.iter().enumerate() {
        // Ensure we only render titles into the actual title columns, not the spacer
        if i < header_column_chunks.len() - 1 {
            let title_paragraph = Paragraph::new(*title)
                .style(HEADER_STYLE)
                .alignment(Alignment::Left); // Align titles to the left within their columns
            f.render_widget(title_paragraph, header_column_chunks[i]);
        }
    }

    // Render Rx title in the second chunk (chart area)
    let rx_title_paragraph = Paragraph::new("Rx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center); // Center align the title
    f.render_widget(rx_title_paragraph, header_row_chunks[1]);

    // Render Tx title in the third chunk (chart area)
    let tx_title_paragraph = Paragraph::new("Tx")
        .style(HEADER_STYLE)
        .alignment(Alignment::Center); // Center align the title
    f.render_widget(tx_title_paragraph, header_row_chunks[2]);
}

/// Renders a single bandwidth chart (Rx or Tx) and its associated speed.
fn render_bandwidth_chart_and_speed(
    f: &mut Frame,
    area: Rect,
    chart_data: Option<&[(f64, f64)]>,
    current_speed: Option<f64>,
    style: Style,
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
        let speed_paragraph = Paragraph::new(speed_text)
            // .style(style) // Use the row's overall style (Green/Yellow/Gray)
            .alignment(Alignment::Right);
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
    // Split row: Text | Rx Chart | Tx Chart
    let row_content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Text area
            Constraint::Percentage(20), // Rx Chart + Speed area
            Constraint::Percentage(20), // Tx Chart + Speed area
        ])
        .split(area);

    let text_area = row_content_chunks[0];
    let rx_area = row_content_chunks[1];
    let tx_area = row_content_chunks[2];

    // Split the text area into columns
    let column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(COLUMN_CONSTRAINTS) // Use the constant
        .split(text_area);

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
    for (idx, cell_text) in data_cells.iter().enumerate() {
        if idx < column_chunks.len() - 1 {
            // Ensure we don't overwrite status column index (or spacer)
            let cell_paragraph = Paragraph::new(cell_text.clone()).style(DATA_CELL_STYLE);
            f.render_widget(cell_paragraph, column_chunks[idx]);
        }
    }

    // Render status in the dedicated status column chunk (index 10)
    let status_paragraph = Paragraph::new(format!("{:<10}", status_text)).style(style); // Pad status
    f.render_widget(status_paragraph, column_chunks[10]);

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
    render_bandwidth_chart_and_speed(
        f,
        rx_area,
        chart_data_in,
        speed_in,
        style, // Pass the overall row style
        Color::Cyan,
        "Rx",
    );

    // Render Tx Chart and Speed
    render_bandwidth_chart_and_speed(
        f,
        tx_area,
        chart_data_out,
        speed_out,
        style, // Pass the overall row style
        Color::Magenta,
        "Tx",
    );
}
