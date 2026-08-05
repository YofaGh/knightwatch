use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{
    error::Error,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::Sender;

use kw_clients::ApiClient;

use crate::{
    events::{AppEvent, CommandOutcome},
    pollers::PollControl,
    ui_helpers::{icon, theme},
};

type ActionResult = Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + Send>>;
type PollAction = fn(Arc<ApiClient>) -> ActionResult;
type IntervalAction = fn(Arc<ApiClient>, u64) -> ActionResult;

/// Right-side panel offering pause/resume/interval controls for one tab's
/// poller. Every tab that polls the server embeds one of these; only the
/// three function pointers differ between tabs.
pub struct PollPanel {
    tab: &'static str,
    control: Arc<Mutex<PollControl>>,
    api: Arc<ApiClient>,
    tx: Sender<AppEvent>,
    pause: PollAction,
    resume: PollAction,
    interval: IntervalAction,
    last_result: Option<(String, bool)>,
}

impl PollPanel {
    pub fn new(
        tab: &'static str,
        control: Arc<Mutex<PollControl>>,
        api: Arc<ApiClient>,
        tx: Sender<AppEvent>,
        pause: PollAction,
        resume: PollAction,
        interval: IntervalAction,
    ) -> Self {
        Self {
            tab,
            control,
            api,
            tx,
            pause,
            resume,
            interval,
            last_result: None,
        }
    }

    /// Call from the tab's `handle_event`, only when commands are allowed
    /// and the user is logged in. Returns whether the key was consumed.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        let Event::Key(key) = event else { return false };
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Char('p') => {
                self.control.lock().unwrap().paused = true; // optimistic
                self.fire("pause", (self.pause)(self.api.clone()));
                true
            }
            KeyCode::Char('r') => {
                self.control.lock().unwrap().paused = false;
                self.fire("resume", (self.resume)(self.api.clone()));
                true
            }
            KeyCode::Char('+') => {
                self.adjust_interval(500);
                true
            }
            KeyCode::Char('-') => {
                self.adjust_interval(-500);
                true
            }
            _ => false,
        }
    }

    fn adjust_interval(&mut self, delta_ms: i64) {
        let new_ms = {
            let mut ctrl = self.control.lock().unwrap();
            ctrl.interval_ms = (ctrl.interval_ms as i64 + delta_ms).max(250) as u64;
            ctrl.interval_ms
        };
        self.fire("interval", (self.interval)(self.api.clone(), new_ms));
    }

    fn fire(&self, label: &'static str, request: ActionResult) {
        crate::commands::spawn_command(self.tx.clone(), self.tab, label, request, |_| {
            CommandOutcome::Ack
        });
    }

    /// Call from the tab's `handle_app_event` when it sees a
    /// `CommandResult` addressed to it with `pid: None`.
    pub fn apply_result(&mut self, label: &str, result: &Result<CommandOutcome, String>) {
        self.last_result = Some(match result {
            Ok(_) => (format!("{label}: ok"), false),
            Err(e) => (format!("{label} failed: {e}"), true),
        });
    }

    /// Always-visible one-line status bar: pause state, interval, key
    /// hints, and the last command's result, all packed horizontally.
    /// Reserves a single line off the top of `area` and returns the rest.
    pub fn render(&self, frame: &mut Frame, area: Rect) -> Rect {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        let ctrl = *self.control.lock().unwrap();
        let status = if ctrl.paused {
            Span::styled(
                format!("{} paused", icon::DOT_OFF),
                Style::default().fg(theme::WARNING),
            )
        } else {
            Span::styled(
                format!("{} live", icon::DOT_ON),
                Style::default().fg(theme::SUCCESS),
            )
        };

        let mut spans = vec![
            Span::raw("Poll  "),
            status,
            Span::styled(
                format!("  {}ms  ", ctrl.interval_ms),
                Style::default().fg(theme::TEXT_DIM),
            ),
            Span::styled(
                "[p] pause  [r] resume  [+/-] interval",
                Style::default().fg(theme::ACCENT),
            ),
        ];

        if let Some((msg, is_err)) = &self.last_result {
            spans.push(Span::raw("   "));
            let color = if *is_err {
                theme::DANGER
            } else {
                theme::SUCCESS
            };
            spans.push(Span::styled(msg.clone(), Style::default().fg(color)));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);
        chunks[1]
    }
}
