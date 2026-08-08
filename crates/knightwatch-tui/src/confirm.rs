use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

pub enum ConfirmOutcome {
    None,
    Confirm,
    Cancel,
}

pub struct ConfirmState {
    title: &'static str,
    message: String,
    submitting: bool,
    error: Option<String>,
}

impl ConfirmState {
    pub fn new(title: &'static str, message: impl Into<String>) -> Self {
        Self {
            title,
            message: message.into(),
            submitting: false,
            error: None,
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> ConfirmOutcome {
        if self.submitting {
            return ConfirmOutcome::None;
        }
        let Event::Key(key) = event else {
            return ConfirmOutcome::None;
        };
        if key.kind != KeyEventKind::Press {
            return ConfirmOutcome::None;
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                self.submitting = true;
                self.error = None;
                ConfirmOutcome::Confirm
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N') => ConfirmOutcome::Cancel,
            _ => ConfirmOutcome::None,
        }
    }

    /// Called by `App` if the spawned action reports failure. Leaves the
    /// modal open so the user can see what happened and retry or cancel.
    pub fn fail(&mut self, message: String) {
        self.submitting = false;
        self.error = Some(message);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let box_width = 50u16.min(area.width.saturating_sub(4)).max(24);
        let box_height = 8u16.min(area.height.saturating_sub(2)).max(7);
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
            .border_style(Style::default().fg(Color::Red))
            .title(
                Line::from(Span::styled(
                    format!(" ⚠ {} ", self.title),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Center),
            );
        let inner = block.inner(box_area);
        frame.render_widget(block, box_area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(2),    // message
                Constraint::Length(1), // blank
                Constraint::Length(1), // status/help
            ])
            .split(inner);

        let Ok(rows): Result<[Rect; 3], _> = rows.as_ref().try_into() else {
            return;
        };

        frame.render_widget(
            Paragraph::new(self.message.clone())
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::White)),
            rows[0],
        );

        let status = if self.submitting {
            Line::from(Span::styled(
                "⏳ working…",
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
                Span::styled("Enter/y", Style::default().fg(Color::Red)),
                Span::styled(" confirm  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc/n", Style::default().fg(Color::Cyan)),
                Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
            ])
        };
        frame.render_widget(Paragraph::new(status).alignment(Alignment::Center), rows[2]);
    }
}
