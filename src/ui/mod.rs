pub mod formatters;
pub mod widgets;

// --- Imports (Combined and adjusted from src/ui.rs) ---
use self::widgets::{render_header, render_node_row};
use crate::ui::formatters::format_duration_human;
use crate::{app::App, cli::Cli, discovery::find_metrics_nodes, fetch::fetch_metrics};
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tokio::time::interval;

// --- TUI Setup and Restore ---

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("Failed to create terminal")
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

// --- Main Application Loop ---

pub async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    _cli: &Cli,
    effective_log_path: &str,
) -> Result<()> {
    let mut discover_timer = interval(Duration::from_secs(60)); // Check for new node URLs every 60s
    let mut last_tick = Instant::now(); // Track the last metrics update time

    // Initial metrics fetch for nodes that had URLs at startup
    if !app.node_urls.is_empty() {
        let urls: Vec<String> = app.node_urls.values().cloned().collect();
        let initial_results = fetch_metrics(&urls).await;
        app.update_metrics(initial_results);
        last_tick = Instant::now(); // Reset last_tick after initial fetch
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        // Calculate time until next tick to potentially sleep or adjust poll timeout
        let now = Instant::now();
        let elapsed_since_last_tick = now.duration_since(last_tick);
        let time_until_next_tick = app.tick_rate.saturating_sub(elapsed_since_last_tick);

        // Poll for events with a timeout. Use a small fixed timeout for responsiveness,
        // or the time until the next tick, whichever is smaller.
        let poll_timeout = time_until_next_tick.min(Duration::from_millis(50)); // Max 50ms wait for input

        tokio::select! {
            _ = discover_timer.tick() => {
                let log_path_buf = std::path::PathBuf::from(effective_log_path);
                match find_metrics_nodes(log_path_buf).await {
                    Ok(found_nodes_with_urls) => {
                        // Found nodes are Vec<(dir_path, url)>
                        let mut updated = false;
                        for (dir_path, url) in found_nodes_with_urls {
                             // Check if this directory is known and if the URL is new or changed
                            if app.nodes.contains(&dir_path) {
                                let current_url = app.node_urls.get(&dir_path);
                                if current_url != Some(&url) {
                                    // New URL or changed URL for a known directory
                                    app.node_urls.insert(dir_path.clone(), url.clone());
                                    // Initialize or re-initialize metrics status
                                    app.node_metrics.insert(url.clone(), Err("Discovered - Fetching...".to_string()));
                                    updated = true;
                                }
                            }
                            // We don't add new directories here, only update URLs for existing ones
                        }

                        // Optional: Check for URLs that are no longer found and mark nodes? Maybe later.

                        if updated {
                            app.status_message = Some("Node URLs updated.".to_string());
                        }
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Error re-discovering node URLs: {}", e));
                    }
                }
            },
            // Poll for keyboard/mouse events
            result = tokio::task::spawn_blocking(move || event::poll(poll_timeout)) => { // Use calculated poll_timeout
                match result {
                    Ok(Ok(true)) => {
                        // Read the event
                        if let Ok(event) = event::read() {
                            match event {
                                Event::Key(key) => {
                                    match key.code {
                                        KeyCode::Char('q') => return Ok(()), // Exit app
                                        KeyCode::Up => {
                                            app.scroll_offset = app.scroll_offset.saturating_sub(1);
                                        }
                                        KeyCode::Down => {
                                            let num_nodes = app.nodes.len();
                                            if num_nodes > 0 {
                                                let max_offset = num_nodes.saturating_sub(1);
                                                 app.scroll_offset = (app.scroll_offset + 1).min(max_offset);
                                            }
                                        }
                                        KeyCode::Char('+') | KeyCode::Char('=') => { // Also handle '=' which is often shift+'+'
                                            app.adjust_tick_rate(true); // Increase interval (slower)
                                            // No need to reset timer, logic below handles it
                                        }
                                         KeyCode::Char('-') => {
                                            app.adjust_tick_rate(false); // Decrease interval (faster)
                                            // No need to reset timer, logic below handles it
                                        }
                                        _ => {} // Ignore other keys
                                    }
                                }
                                Event::Mouse(MouseEvent { kind, .. }) => {
                                    match kind {
                                        MouseEventKind::ScrollUp => {
                                            app.scroll_offset = app.scroll_offset.saturating_sub(1);
                                        }
                                        MouseEventKind::ScrollDown => {
                                            let num_nodes = app.nodes.len();
                                            if num_nodes > 0 {
                                                let max_offset = num_nodes.saturating_sub(1);
                                                 app.scroll_offset = (app.scroll_offset + 1).min(max_offset);
                                            }
                                        }
                                        _ => {} // Ignore other mouse events like move, click
                                    }
                                }
                                _ => {} // Ignore other event types
                            }
                        }
                    }
                    Ok(Ok(false)) => {} // Timeout elapsed without event
                    Ok(Err(e)) => {
                        app.status_message = Some(format!("Input polling error: {}", e));
                    }
                    Err(e) => {
                         app.status_message = Some(format!("Input task spawn error: {}", e));
                    }
                }
            },
            // Use a small sleep if there's significant time until the next tick and no event occurred
            _ = tokio::time::sleep(poll_timeout), if !poll_timeout.is_zero() => {
                // This branch ensures the loop doesn't spin wildly if poll_timeout is very small
                // but it's not yet time for the next tick.
            }
        }

        // Check if it's time for the next tick AFTER handling events/sleep
        if Instant::now().duration_since(last_tick) >= app.tick_rate {
            // Fetch metrics only for nodes with known URLs
            if !app.node_urls.is_empty() {
                let urls: Vec<String> = app.node_urls.values().cloned().collect();
                let results = fetch_metrics(&urls).await;
                app.update_metrics(results);
            }
            last_tick = Instant::now(); // Update last tick time
        }
    }
}

// --- UI Rendering ---

// This function is now internal to the ui module, called by run_app
fn ui(f: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(2), // Top Title area (might need adjustment if content wraps)
                Constraint::Length(2), // Summary Gauges
                Constraint::Min(0),    // Node Table
                Constraint::Length(1), // Bottom Status / Error
            ]
            .as_ref(),
        )
        .split(f.size());

    // --- Calculate Running Node Count ---
    let mut running_nodes_count = 0;
    for node_path in &app.nodes {
        if let Some(url) = app.node_urls.get(node_path) {
            if let Some(Ok(_)) = app.node_metrics.get(url) {
                running_nodes_count += 1;
            }
        }
    }
    let total_nodes_count = app.nodes.len();

    // --- Top Bar (Title + Node Count) ---
    let top_area = main_chunks[0];
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),     // Title takes remaining space
            Constraint::Length(20), // Increased space for "Nodes: N / N"
        ])
        .split(top_area);

    let title = Paragraph::new("Autonomi Node Dashboard")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Left);
    f.render_widget(title, top_chunks[0]);

    // Create spans for different parts of the node count text
    let node_count_spans = Line::from(vec![
        Span::styled("Nodes: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            running_nodes_count.to_string(),
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
        Span::styled(" / ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            total_nodes_count.to_string(),
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
    ]);

    // Create the paragraph with the styled spans
    let node_count_widget = Paragraph::new(node_count_spans).alignment(Alignment::Right);

    f.render_widget(node_count_widget, top_chunks[1]);

    // Render summary gauges in the next chunk
    widgets::render_summary_gauges(f, app, main_chunks[1]);

    // Render node table in the adjusted chunk
    render_custom_node_rows(f, app, main_chunks[2]);

    // --- Bottom Status Bar ---
    let bottom_area = main_chunks[3];
    if let Some(msg) = &app.status_message {
        // If there's an error/status message, display it across the whole bottom bar
        let error_paragraph = Paragraph::new(msg.clone()).style(Style::default().fg(Color::Red));
        f.render_widget(error_paragraph, bottom_area);
    } else {
        // Otherwise, split the bottom bar for standard status
        let status_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50), // Left side ("Press 'q' to quit")
                Constraint::Percentage(50), // Right side (Update info)
            ])
            .split(bottom_area);

        // Left status with 'q' highlighted
        let left_status_spans = Line::from(vec![
            Span::styled("Press '", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Rgb(255, 165, 0))),
            Span::styled("' to quit", Style::default().fg(Color::DarkGray)),
        ]);
        let left_status = Paragraph::new(left_status_spans).alignment(Alignment::Left);

        // Right status with values highlighted
        let tick_rate_str = format_duration_human(app.tick_rate);
        let elapsed_secs_str = app.last_update.elapsed().as_secs().to_string();
        let right_status_spans = Line::from(vec![
            Span::styled("Update: ", Style::default().fg(Color::DarkGray)),
            Span::styled(tick_rate_str, Style::default().fg(Color::Rgb(255, 165, 0))),
            Span::styled(" | Last: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                elapsed_secs_str,
                Style::default().fg(Color::Rgb(255, 165, 0)),
            ),
            Span::styled("s ago", Style::default().fg(Color::DarkGray)),
            Span::styled(" | Speed: ", Style::default().fg(Color::DarkGray)),
            Span::styled("+/-", Style::default().fg(Color::Rgb(255, 165, 0))),
        ]);
        let right_status = Paragraph::new(right_status_spans).alignment(Alignment::Right);

        f.render_widget(left_status, status_chunks[0]);
        f.render_widget(right_status, status_chunks[1]);
    }

    // Clear the status message after displaying it once (optional, remove if messages should persist)
    // app.status_message = None;
}

/// Renders the main content area containing the node list (header + rows).
/// Delegates rendering the header and individual rows to helper functions.
// This function is also internal to the ui module, called by ui
fn render_custom_node_rows(f: &mut Frame, app: &mut App, area: Rect) {
    let inner_area = area.inner(&Margin {
        vertical: 1,
        horizontal: 1,
    });

    let num_nodes = app.nodes.len();
    if num_nodes == 0 {
        let no_nodes_text = Paragraph::new("No nodes discovered yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_nodes_text, inner_area);
        return;
    }

    // Calculate available height for node rows (excluding header)
    let header_height = 1;
    let available_height = inner_area.height.saturating_sub(header_height);
    let num_visible_rows = available_height as usize;

    // Adjust scroll offset if it's too large for the current number of nodes/visible rows
    if num_nodes > num_visible_rows {
        app.scroll_offset = app
            .scroll_offset
            .min(num_nodes.saturating_sub(num_visible_rows));
    } else {
        // If all nodes fit, reset scroll offset
        app.scroll_offset = 0;
    }

    // Define layout constraints: 1 for header, then 1 for each VISIBLE row
    let mut constraints = vec![Constraint::Length(header_height)];
    constraints.extend(std::iter::repeat_n(
        Constraint::Length(1),
        num_visible_rows.min(num_nodes), // Don't create more constraints than nodes
    ));

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    render_header(f, vertical_chunks[0]);

    // Determine the range of nodes to display
    let start_index = app.scroll_offset;
    let end_index = (start_index + num_visible_rows).min(num_nodes);

    // Iterate only over the visible nodes based on scroll offset
    for (relative_index, node_index) in (start_index..end_index).enumerate() {
        let chunk_index = relative_index + 1; // +1 to skip header chunk
        if chunk_index >= vertical_chunks.len() {
            // Should not happen with correct constraint calculation, but safeguard
            break;
        }
        let row_area = vertical_chunks[chunk_index];

        // Get the directory path for the current node index
        let dir_path = &app.nodes[node_index];
        // Find the corresponding URL, if it exists
        let url_option = app.node_urls.get(dir_path);

        // Pass the directory path and the Option<&String> URL to render_node_row
        render_node_row(f, app, row_area, dir_path, url_option);
    }
}
