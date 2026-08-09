use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

pub enum ConnectOutcome {
    None,
    Submit(String),
    Quit,
}

/// Shown once, before anything else exists — there's no session to fall
/// back to, so unlike `LoginState` this has no `cancellable` flag; Esc /
/// Ctrl+C just quit the whole app instead of dismissing the screen.
pub struct ConnectState {
    url: String,
    connecting: bool,
    error: Option<String>,
}

impl ConnectState {
    /// `initial_url` is whatever `KW_URL` was set to (or default).
    pub const fn new(initial_url: String) -> Self {
        Self {
            url: initial_url,
            connecting: false,
            error: None,
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> ConnectOutcome {
        if self.connecting {
            return ConnectOutcome::None; // ignore input while a request is in flight
        }

        let Event::Key(key) = event else {
            return ConnectOutcome::None;
        };
        if key.kind != KeyEventKind::Press {
            return ConnectOutcome::None;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return ConnectOutcome::Quit;
            }
            KeyCode::Char(c) => {
                self.error = None;
                self.url.push(c);
            }
            KeyCode::Backspace => {
                self.error = None;
                self.url.pop();
            }
            KeyCode::Enter => {
                let trimmed = self.url.trim();
                if trimmed.is_empty() {
                    self.error = Some("server url is required".into());
                } else {
                    self.connecting = true;
                    self.error = None;
                    return ConnectOutcome::Submit(trimmed.to_string());
                }
            }
            KeyCode::Esc => return ConnectOutcome::Quit,
            _ => {}
        }

        ConnectOutcome::None
    }

    /// Called by `main` once a connection attempt fails, so the user can
    /// edit the url and retry.
    pub fn fail(&mut self, message: String) {
        self.connecting = false;
        self.error = Some(message);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let box_width = 50u16.min(area.width.saturating_sub(4)).max(20);
        let box_height = 11u16.min(area.height.saturating_sub(2)).max(9);
        let x = area
            .x
            .saturating_add((area.width.saturating_sub(box_width)) / 2);
        let y = area
            .y
            .saturating_add((area.height.saturating_sub(box_height)) / 2);
        let box_area = Rect {
            x,
            y,
            width: box_width,
            height: box_height,
        };

        frame.render_widget(
            Paragraph::new("").style(Style::default().bg(Color::Reset)),
            area,
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(
                Line::from(Span::styled(
                    " ◈ Knightwatch ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Center),
            );
        let outer_inner = block.inner(box_area);
        frame.render_widget(block, box_area);

        let inner = Rect {
            x: outer_inner.x.saturating_add(1),
            y: outer_inner.y,
            width: outer_inner.width.saturating_sub(2),
            height: outer_inner.height,
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // subtitle
                Constraint::Length(1), // blank
                Constraint::Length(1), // label
                Constraint::Length(1), // value
                Constraint::Length(1), // blank
                Constraint::Min(1),    // status/help
            ])
            .split(inner);

        let Ok(rows): Result<[Rect; 6], _> = rows.as_ref().try_into() else {
            return;
        };

        frame.render_widget(
            Paragraph::new(Span::styled(
                "connect to a knightwatch server",
                Style::default().fg(Color::DarkGray),
            )),
            rows[0],
        );

        frame.render_widget(
            Paragraph::new(Span::styled(
                "▸ SERVER URL",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            rows[2],
        );

        let show_cursor = !self.connecting;
        let mut spans = vec![Span::styled("  ", Style::default())];
        spans.push(Span::styled(
            self.url.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::UNDERLINED),
        ));
        if show_cursor {
            spans.push(Span::styled(
                "▏",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), rows[3]);

        let status = if self.connecting {
            Line::from(Span::styled(
                "⏳ connecting…",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ))
        } else if let Some(err) = &self.error {
            Line::from(vec![
                Span::styled(
                    "✗ ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.clone(), Style::default().fg(Color::Red)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(" connect  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(" quit", Style::default().fg(Color::DarkGray)),
            ])
        };
        frame.render_widget(Paragraph::new(status), rows[5]);
    }
}
