use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use ratatui_image::{StatefulImage, picker::Picker, protocol::StatefulProtocol};
use std::sync::{Arc, Mutex};

use crate::{events::AppEvent, poll_panel::PollPanel};

/// One decoded, ready-to-render screenshot.
///
/// We keep the metadata (monitor id/name) alongside the already-built
/// `StatefulProtocol` so we don't have to re-decode base64 on every frame —
/// decoding only happens when a fresh `ScreenImages` event arrives.
struct ScreenshotEntry {
    monitor_id: u32,
    monitor_name: String,
    protocol: StatefulProtocol,
}

pub struct ScreenTab {
    picker: Picker,
    screenshots: Vec<ScreenshotEntry>,
    /// Which monitor is currently shown large. Persists across events so a
    /// user's selection survives the next screenshot batch, as long as that
    /// monitor is still present.
    primary_monitor_id: Option<u32>,
    /// Screen-space rects of the thumbnails from the last render, each
    /// tagged with the monitor id it represents, so `handle_event` can hit
    /// test mouse clicks against them.
    thumb_hit_rects: Vec<(Rect, u32)>,
    commands_allowed: bool,
    poll_panel: PollPanel,
}

impl super::Tab for ScreenTab {
    fn name(&self) -> &'static str {
        "Screen"
    }

    fn handle_event(&mut self, event: &Event, logged_in: bool) -> bool {
        if self.commands_allowed && logged_in && self.poll_panel.handle_event(event) {
            return true;
        }

        let Event::Mouse(mouse) = event else {
            return false;
        };
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }

        for (rect, monitor_id) in &self.thumb_hit_rects {
            let hit = mouse.column >= rect.x
                && mouse.column < rect.x + rect.width
                && mouse.row >= rect.y
                && mouse.row < rect.y + rect.height;
            if hit {
                self.primary_monitor_id = Some(*monitor_id);
                return true;
            }
        }
        false
    }

    fn handle_app_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::ScreenImages(screenshots) => {
                self.set_images(screenshots);
                true
            }
            AppEvent::CommandResult { label, result, .. } => {
                self.poll_panel.apply_result(label, result);
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, logged_in: bool) {
        let area = if self.commands_allowed && logged_in {
            self.poll_panel.render(frame, area)
        } else {
            area
        };
        let area =
            crate::ui_helpers::command_login_banner(frame, area, self.commands_allowed, logged_in);

        if self.screenshots.is_empty() {
            crate::ui_helpers::waiting_placeholder(frame, area, "Screen");
            return;
        }

        // Resolve which entry is primary, falling back to the first one if
        // the previously-selected monitor id is no longer present.
        let primary_idx = self
            .screenshots
            .iter()
            .position(|s| Some(s.monitor_id) == self.primary_monitor_id)
            .unwrap_or(0);
        self.primary_monitor_id = Some(self.screenshots[primary_idx].monitor_id);

        let others: Vec<usize> = (0..self.screenshots.len())
            .filter(|&i| i != primary_idx)
            .collect();

        self.thumb_hit_rects.clear();

        let (primary_area, thumbs_area) = if others.is_empty() {
            (area, None)
        } else {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(28)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        };

        // ── Primary image ──
        frame.render_stateful_widget(
            StatefulImage::default(),
            primary_area,
            &mut self.screenshots[primary_idx].protocol,
        );

        // ── Thumbnails on the right, stacked vertically ──
        if let Some(thumbs_area) = thumbs_area {
            let mut constraints: Vec<Constraint> =
                others.iter().map(|_| Constraint::Length(9)).collect();
            constraints.push(Constraint::Min(0)); // soak up leftover space
            let thumb_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(thumbs_area);

            for (slot, &idx) in others.iter().enumerate() {
                let chunk = thumb_chunks[slot];
                let label_area = Rect { height: 1, ..chunk };
                let image_area = Rect {
                    y: chunk.y + 1,
                    height: chunk.height.saturating_sub(1),
                    ..chunk
                };

                let monitor_id = self.screenshots[idx].monitor_id;
                frame.render_widget(
                    Paragraph::new(self.screenshots[idx].monitor_name.clone())
                        .style(Style::default().fg(Color::DarkGray)),
                    label_area,
                );
                frame.render_stateful_widget(
                    StatefulImage::default(),
                    image_area,
                    &mut self.screenshots[idx].protocol,
                );

                self.thumb_hit_rects.push((chunk, monitor_id));
            }
        }
    }
}

impl ScreenTab {
    pub fn new(
        picker: Picker,
        allow_screen_commands: bool,
        api: Arc<kw_clients::ApiClient>,
        tx: tokio::sync::mpsc::Sender<AppEvent>,
        control: Arc<Mutex<crate::pollers::PollControl>>,
    ) -> Self {
        let poll_panel = PollPanel::new(
            "Screen",
            control,
            api,
            tx,
            |api| Box::pin(async move { api.screen_capture_poll_pause().await }),
            |api| Box::pin(async move { api.screen_capture_poll_resume().await }),
            |api, ms| Box::pin(async move { api.screen_capture_interval(ms).await }),
        );
        Self {
            picker,
            screenshots: Vec::new(),
            primary_monitor_id: None,
            thumb_hit_rects: Vec::new(),
            commands_allowed: allow_screen_commands,
            poll_panel,
        }
    }

    fn set_images(&mut self, screenshots: &[kw_types::api::ScreenshotImage]) {
        let mut entries = Vec::with_capacity(screenshots.len());
        for shot in screenshots {
            match crate::utils::base64_to_image(&shot.data) {
                Ok(image) => {
                    let protocol = self.picker.new_resize_protocol(image);
                    entries.push(ScreenshotEntry {
                        monitor_id: shot.monitor_id,
                        monitor_name: shot.monitor_name.clone(),
                        protocol,
                    });
                }
                Err(err) => {
                    eprintln!(
                        "screen tab: failed to decode screenshot for monitor {} ({}): {err}",
                        shot.monitor_id, shot.monitor_name
                    );
                }
            }
        }
        self.screenshots = entries;
    }
}
