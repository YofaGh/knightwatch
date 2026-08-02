use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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

    let nav_area = chunks[0];
    let content_area = chunks[1];

    // ── Nav bar ──
    let nav_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " ◈ Knightwatch ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let inner_nav_area = nav_block.inner(nav_area);
    frame.render_widget(nav_block, nav_area);

    // Right-hand cluster: telegram status (only when active) + shutdown.
    let shutdown_label = " ⏻ Shutdown ";
    let telegram_label = " 🤖 Telegram ";
    let mut right_width = shutdown_label.chars().count() as u16;
    if app.telegram_bot {
        right_width += telegram_label.chars().count() as u16;
    }

    let nav_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(right_width)])
        .split(inner_nav_area);
    let tabs_area = nav_cols[0];
    let right_area = nav_cols[1];

    let mut spans = Vec::new();
    app.tab_hit_rects.clear();
    let mut current_x = tabs_area.x;

    let titles = app.tab_titles();
    let last = titles.len().saturating_sub(1);

    for (i, title) in titles.iter().enumerate() {
        let width = title.chars().count() as u16;
        app.tab_hit_rects.push((current_x, current_x + width));
        current_x += width;

        let style = if i == app.selected_tab {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(*title, style));

        if i < last {
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
            current_x += 1;
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), tabs_area);

    let mut right_spans = Vec::new();
    if app.telegram_bot {
        right_spans.push(Span::styled(
            telegram_label,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }
    right_spans.push(Span::styled(
        shutdown_label,
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right),
        right_area,
    );

    if app.logged_in() {
        let shutdown_width = shutdown_label.chars().count() as u16;
        app.shutdown_hit_rect = Some(Rect {
            x: right_area.x + right_area.width.saturating_sub(shutdown_width),
            y: right_area.y,
            width: shutdown_width.min(right_area.width),
            height: 1,
        });
    }

    // ── Content shell ──
    let content_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));
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
