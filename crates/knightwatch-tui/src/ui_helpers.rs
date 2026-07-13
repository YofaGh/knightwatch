use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Gauge, Paragraph},
};

/// Unicode-block progress bar, e.g. "████░░░░".
pub fn bar(percent: f64, width: usize) -> String {
    let percent = percent.clamp(0.0, 100.0);
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Standard red/yellow/green threshold coloring used across tabs.
pub fn percent_color(p: f64) -> Color {
    if p >= 90.0 {
        Color::Red
    } else if p >= 70.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// Draws the standard dark-gray-bordered, cyan-titled block and returns
/// the inner content area.
pub fn bordered_block(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(Color::Cyan),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// Centered "waiting for first snapshot" placeholder.
pub fn waiting_placeholder(frame: &mut Frame, area: Rect, label: &str) {
    let mid = area.height / 2;
    let centered = Rect {
        y: area.y + mid,
        height: 1,
        ..area
    };
    frame.render_widget(
        Paragraph::new(format!("[ {label}: waiting for first snapshot... ]"))
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        centered,
    );
}

/// Dimmed one-line "no X reported" message.
pub fn empty_note(frame: &mut Frame, area: Rect, msg: &str) {
    frame.render_widget(
        Paragraph::new(msg.to_string()).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

/// A gauge auto-colored by percent-color thresholds.
pub fn percent_gauge(title: &str, percent: f64, label: String) -> Gauge<'static> {
    Gauge::default()
        .block(Block::default().title(title.to_string()))
        .gauge_style(Style::default().fg(percent_color(percent)))
        .ratio((percent / 100.0).clamp(0.0, 1.0))
        .label(label)
}
