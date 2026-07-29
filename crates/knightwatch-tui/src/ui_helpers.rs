use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
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

/// Whether a mouse event's coordinates fall inside `rect`.
pub fn mouse_hit(mouse: &crossterm::event::MouseEvent, rect: &Rect) -> bool {
    mouse.column >= rect.x
        && mouse.column < rect.x + rect.width
        && mouse.row >= rect.y
        && mouse.row < rect.y + rect.height
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

/// Reserves a one-line banner at the top of `area` when a tab allows
/// commands but the user isn't currently logged in, and returns the
/// remaining `Rect` for the tab's normal content. If no banner is needed
/// (commands aren't a thing for this tab, or the user is logged in),
/// `area` is returned unchanged.
///
/// Every tab with a `commands_allowed` flag should call this as the very
/// first line of `render`, then render into the returned area instead of
/// the original one.
pub fn command_login_banner(
    frame: &mut Frame,
    area: Rect,
    commands_allowed: bool,
    logged_in: bool,
) -> Rect {
    if !commands_allowed || logged_in {
        return area;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("🔒 ", Style::default().fg(Color::Yellow)),
            Span::styled(
                "login required to run commands",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  (Ctrl+L to log in)", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Center),
        chunks[0],
    );

    chunks[1]
}
