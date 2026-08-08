use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use kw_utils::conv::usize_to_u16_saturating;

use crate::ui_helpers::{icon, theme};

pub fn render(frame: &mut ratatui::Frame, app: &mut crate::app::App) {
    let area = frame.area();

    if let Some(login) = &app.login {
        login.render(frame, area);
        return;
    }

    if let Some(confirm) = &app.confirm_shutdown {
        confirm.render(frame, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let [nav_area, content_area] = chunks.as_ref() else {
        return;
    };
    let nav_area = *nav_area;
    let content_area = *content_area;

    // ── Nav bar ── same rounded/cyan/◈ language as the login dialog, so
    // the app doesn't visually "change products" once you're past it.
    let nav_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT_MUTED))
        .title(Span::styled(
            format!(" {} Knightwatch ", icon::APP),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

    let inner_nav_area = nav_block.inner(nav_area);
    frame.render_widget(nav_block, nav_area);

    // Right-hand cluster: telegram status (only when active) + shutdown.
    let shutdown_label = format!(" {} Shutdown ", icon::POWER);
    let telegram_label = format!(" {} Telegram ", icon::BOT);
    let mut right_width = usize_to_u16_saturating(shutdown_label.chars().count());
    if app.telegram_bot {
        right_width =
            right_width.saturating_add(usize_to_u16_saturating(telegram_label.chars().count()));
    }

    let nav_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(right_width)])
        .split(inner_nav_area);
    let [tabs_area, right_area] = nav_cols.as_ref() else {
        return;
    };
    let tabs_area = *tabs_area;
    let right_area = *right_area;

    let mut spans = Vec::new();
    app.tab_hit_rects.clear();
    let mut current_x = tabs_area.x;

    let titles = app.tab_titles();
    let last = titles.len().saturating_sub(1);

    for (i, title) in titles.iter().enumerate() {
        let is_selected = i == app.selected_tab;
        // Reserve the same width whether or not the cursor glyph shows,
        // so hit-testing lines up with what's drawn either way.
        let label = format!(" {title} ");
        let width = usize_to_u16_saturating(label.chars().count());
        app.tab_hit_rects
            .push((current_x, current_x.saturating_add(width)));
        current_x = current_x.saturating_add(width);

        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        spans.push(Span::styled(label, style));

        if i < last {
            spans.push(Span::styled("│", Style::default().fg(theme::ACCENT_MUTED)));
            current_x = current_x.saturating_add(1);
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), tabs_area);

    let mut right_spans = Vec::new();
    if app.telegram_bot {
        right_spans.push(Span::styled(
            telegram_label,
            Style::default()
                .fg(theme::SUCCESS)
                .add_modifier(Modifier::BOLD),
        ));
    }
    right_spans.push(Span::styled(
        shutdown_label.clone(),
        Style::default()
            .fg(theme::TEXT)
            .bg(theme::DANGER)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right),
        right_area,
    );

    if app.logged_in() {
        let shutdown_width = usize_to_u16_saturating(shutdown_label.chars().count());
        app.shutdown_hit_rect = Some(Rect {
            x: right_area
                .x
                .saturating_add(right_area.width.saturating_sub(shutdown_width)),
            y: right_area.y,
            width: shutdown_width.min(right_area.width),
            height: 1,
        });
    }

    // ── Content shell ── rounded to match, no top border since the nav
    // bar's bottom edge already closes it off.
    let content_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT_MUTED));
    let inner_content = content_block.inner(content_area);
    frame.render_widget(content_block, content_area);

    // No login screen showing => either auth is off, or the user already
    // has a session. Either way, commands are runnable.
    let logged_in = app.authenticated;

    // ── Route inner rendering to the active tab ──
    if let Some(tab) = app.tabs.get_mut(app.selected_tab) {
        tab.render(frame, inner_content, logged_in);
    }
}
