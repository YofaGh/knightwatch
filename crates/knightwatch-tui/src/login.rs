use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Username,
    Password,
}

/// What the login screen wants the caller (`App`) to do, reported back
/// from `handle_event` so `App` can react — fire off the async login
/// call, or drop the screen if the user backs out of an *optional* login.
pub enum LoginOutcome {
    None,
    Submit { username: String, password: String },
    Cancel,
}

pub struct LoginState {
    username: String,
    password: String,
    focused: Field,
    /// If false there's no session to fall back to (the mandatory
    /// startup login), so Esc/Cancel is disabled.
    cancellable: bool,
    submitting: bool,
    error: Option<String>,
}

impl LoginState {
    pub const fn new(cancellable: bool) -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            focused: Field::Username,
            cancellable,
            submitting: false,
            error: None,
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> LoginOutcome {
        if self.submitting {
            return LoginOutcome::None; // ignore input while a request is in flight
        }

        let Event::Key(key) = event else {
            return LoginOutcome::None;
        };
        if key.kind != KeyEventKind::Press {
            return LoginOutcome::None;
        }

        match key.code {
            KeyCode::Tab | KeyCode::Down | KeyCode::Up | KeyCode::BackTab => {
                self.focused = match self.focused {
                    Field::Username => Field::Password,
                    Field::Password => Field::Username,
                };
            }
            KeyCode::Char(c) => {
                self.error = None;
                match self.focused {
                    Field::Username => self.username.push(c),
                    Field::Password => self.password.push(c),
                }
            }
            KeyCode::Backspace => {
                self.error = None;
                match self.focused {
                    Field::Username => {
                        self.username.pop();
                    }
                    Field::Password => {
                        self.password.pop();
                    }
                }
            }
            KeyCode::Enter => {
                if self.username.is_empty() {
                    self.error = Some("username is required".into());
                } else {
                    self.submitting = true;
                    self.error = None;
                    return LoginOutcome::Submit {
                        username: self.username.clone(),
                        password: self.password.clone(),
                    };
                }
            }
            KeyCode::Esc if self.cancellable => return LoginOutcome::Cancel,
            _ => {}
        }

        LoginOutcome::None
    }

    /// Called by `App` once the spawned login task reports failure.
    pub fn fail(&mut self, message: String) {
        self.submitting = false;
        self.password.clear();
        self.error = Some(message);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let box_width = 46u16.min(area.width.saturating_sub(4)).max(20);
        let box_height = 13u16.min(area.height.saturating_sub(2)).max(11);
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

        // dim the background behind the dialog so it reads as a modal
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

        // horizontal breathing room so fields don't touch the border
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
                Constraint::Length(1), // username label
                Constraint::Length(1), // username value
                Constraint::Length(1), // blank
                Constraint::Length(1), // password label
                Constraint::Length(1), // password value
                Constraint::Length(1), // blank
                Constraint::Min(1),    // status/help
            ])
            .split(inner);

        let Ok(rows): Result<[Rect; 9], _> = rows.as_ref().try_into() else {
            return;
        };

        frame.render_widget(
            Paragraph::new(Span::styled(
                "sign in to continue",
                Style::default().fg(Color::DarkGray),
            )),
            rows[0],
        );

        self.render_field(
            frame,
            rows[2],
            rows[3],
            "USERNAME",
            &self.username,
            false,
            Field::Username,
        );
        self.render_field(
            frame,
            rows[5],
            rows[6],
            "PASSWORD",
            &self.password,
            true,
            Field::Password,
        );

        let status = if self.submitting {
            Line::from(Span::styled(
                "⏳ logging in…",
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
        } else if self.cancellable {
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::styled(" switch  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(" login  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::styled(" switch  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(" login", Style::default().fg(Color::DarkGray)),
            ])
        };
        frame.render_widget(Paragraph::new(status), rows[8]);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_field(
        &self,
        frame: &mut Frame,
        label_area: Rect,
        value_area: Rect,
        label: &str,
        value: &str,
        mask: bool,
        field: Field,
    ) {
        let focused = self.focused == field;

        let marker = if focused { "▸ " } else { "  " };
        let label_style = if focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(format!("{marker}{label}")).style(label_style),
            label_area,
        );

        let display = if mask {
            "•".repeat(value.chars().count())
        } else {
            value.to_string()
        };
        let show_cursor = focused && !self.submitting;

        let base_style = if focused {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::Gray)
        };

        let mut spans = vec![Span::styled("  ", Style::default())];
        if display.is_empty() && !show_cursor {
            spans.push(Span::styled("", Style::default().fg(Color::DarkGray)));
        } else {
            spans.push(Span::styled(display, base_style));
        }
        if show_cursor {
            spans.push(Span::styled(
                "▏",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), value_area);
    }
}
