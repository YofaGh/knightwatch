use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph},
};

use kw_utils::conv;

/// every part should pull its colors from here instead of hardcoding `Color::Cyan`.
pub mod theme {
    use ratatui::style::Color;

    pub const ACCENT: Color = Color::Cyan;
    pub const ACCENT_MUTED: Color = Color::DarkGray;
    pub const DANGER: Color = Color::Red;
    pub const SUCCESS: Color = Color::Green;
    pub const WARNING: Color = Color::Yellow;
    pub const TEXT: Color = Color::White;
    pub const TEXT_DIM: Color = Color::Gray;
    pub const TEXT_MUTED: Color = Color::DarkGray;
}

/// Small, consistent symbol set used across screens instead of each tab
/// picking its own emoji/glyphs ad hoc.
pub mod icon {
    pub const APP: &str = "◈";
    pub const LOCK: &str = "🔒";
    pub const POWER: &str = "⏻";
    pub const BOT: &str = "🤖";
    pub const WARNING: &str = "⚠";
    pub const OK: &str = "✓";
    pub const ERR: &str = "✗";
    pub const PENDING: &str = "⏳";
    pub const DOT_ON: &str = "●";
    pub const DOT_OFF: &str = "○";
    pub const CURSOR: &str = "▸";
}

/// Unicode-block progress bar, e.g. "████░░░░".
pub fn bar(percent: f64, width: usize) -> String {
    let percent = percent.clamp(0.0, 100.0);
    let filled =
        conv::f64_to_usize_saturating(((percent / 100.0) * conv::usize_to_f64(width)).round());
    let filled = filled.min(width);
    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

/// Standard red/yellow/green threshold coloring used across tabs.
pub fn percent_color(p: f64) -> ratatui::style::Color {
    if p >= 90.0 {
        theme::DANGER
    } else if p >= 70.0 {
        theme::WARNING
    } else {
        theme::SUCCESS
    }
}

/// Whether a mouse event's coordinates fall inside `rect`.
pub const fn mouse_hit(mouse: crossterm::event::MouseEvent, rect: Rect) -> bool {
    mouse.column >= rect.x
        && mouse.column < rect.x.saturating_add(rect.width)
        && mouse.row >= rect.y
        && mouse.row < rect.y.saturating_add(rect.height)
}

/// Draws the standard rounded, accent-titled block and returns the inner content area.
pub fn bordered_block(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    bordered_block_styled(frame, area, title, false)
}

pub fn bordered_block_focused(frame: &mut Frame, area: Rect, title: &str, focused: bool) -> Rect {
    bordered_block_styled(frame, area, title, focused)
}

fn bordered_block_styled(frame: &mut Frame, area: Rect, title: &str, active: bool) -> Rect {
    let border_color = if active {
        theme::ACCENT
    } else {
        theme::ACCENT_MUTED
    };
    let title_style = if active {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::ACCENT)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(format!(" {title} "), title_style));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// Centered "waiting for first snapshot" placeholder.
pub fn waiting_placeholder(frame: &mut Frame, area: Rect, label: &str) {
    let mid = area.height / 2;
    let centered = Rect {
        y: area.y.saturating_add(mid),
        height: 1,
        ..area
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{} {label}: waiting for first snapshot… ",
            icon::PENDING
        ))
        .style(Style::default().fg(theme::TEXT_MUTED))
        .alignment(Alignment::Center),
        centered,
    );
}

/// Dimmed one-line "no X reported" message.
pub fn empty_note(frame: &mut Frame, area: Rect, msg: &str) {
    frame.render_widget(
        Paragraph::new(msg.to_string()).style(Style::default().fg(theme::TEXT_MUTED)),
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

/// One-line status/result readout shared by poll panels and action
/// panels: "label: ok" in success color, "label failed: msg" in danger
/// color, or nothing if there's no result yet. Centralizing this keeps
/// every panel's last-command feedback looking identical.
pub fn result_line(label: &str, is_err: bool) -> Line<'static> {
    let icon = if is_err { icon::ERR } else { icon::OK };
    let color = if is_err {
        theme::DANGER
    } else {
        theme::SUCCESS
    };
    Line::from(vec![
        Span::styled(
            format!("{icon} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(color)),
    ])
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

    let [banner_area, rest_area] = chunks.as_ref() else {
        return area;
    };
    let banner_area = *banner_area;
    let rest_area = *rest_area;

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", icon::LOCK),
                Style::default().fg(theme::WARNING),
            ),
            Span::styled(
                "login required to run commands",
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  (Ctrl+L to log in)",
                Style::default().fg(theme::TEXT_MUTED),
            ),
        ]))
        .alignment(Alignment::Center),
        banner_area,
    );

    rest_area
}

/// Reserves a one-line banner at the top of `area` when there are active
/// alarms, and returns the remaining `Rect`. Shown above the tab content
/// regardless of which tab is selected, since alarm state is global.
/// If there's no snapshot yet or nothing is active, `area` is unchanged.
pub fn alarm_banner(
    frame: &mut Frame,
    area: Rect,
    alarms: Option<&kw_types::resources::AlarmSnapshot>,
) -> Rect {
    let Some(alarms) = alarms else { return area };
    let any_active = alarms.cpu.active
        || alarms.memory.active
        || alarms.battery_low.active
        || alarms.disks.iter().any(|(_, s)| s.active);
    if !any_active {
        return area;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    let [banner_area, rest_area] = chunks.as_ref() else {
        return area;
    };
    let banner_area = *banner_area;
    let rest_area = *rest_area;

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", icon::WARNING),
                Style::default()
                    .fg(theme::DANGER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(alarms.to_string(), Style::default().fg(theme::DANGER)),
        ]))
        .alignment(Alignment::Center),
        banner_area,
    );

    rest_area
}
