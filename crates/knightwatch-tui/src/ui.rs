use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(frame: &mut ratatui::Frame, app: &mut crate::app::App) {
    let area = frame.area();

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

    let mut spans = Vec::new();
    app.tab_hit_rects.clear();
    let mut current_x = inner_nav_area.x;

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
    frame.render_widget(Paragraph::new(Line::from(spans)), inner_nav_area);

    // ── Content shell ──
    let content_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner_content = content_block.inner(content_area);
    frame.render_widget(content_block, content_area);

    // ── Route inner rendering to the active tab ──
    if let Some(tab) = app.tabs.get_mut(app.selected_tab) {
        tab.render(frame, inner_content);
    }
}
