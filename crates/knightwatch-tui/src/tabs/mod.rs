use ratatui::layout::Rect;

use crate::events::AppEvent;

mod docker;
mod processes;
mod screen;
mod system_resources;
mod systemd;
mod top_processes;

pub use docker::DockerTab;
pub use processes::ProcessesTab;
pub use screen::ScreenTab;
pub use system_resources::SystemResourcesTab;
pub use systemd::SystemdTab;
pub use top_processes::{TopProcessesPollConfig, TopProcessesTab};

pub trait Tab {
    fn name(&self) -> &'static str;

    fn handle_event(&mut self, _event: &crossterm::event::Event) -> bool {
        false
    }

    /// Non-input events: data arriving from a background poller, etc.
    /// Returns whether the tab's visible state actually changed, so the
    /// caller knows whether a redraw is warranted. Most tabs ignore most
    /// variants and just return `false`.
    fn handle_app_event(&mut self, _event: &AppEvent) -> bool {
        false
    }

    fn render(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let mid = area.height / 2;
        let centered = Rect {
            y: area.y + mid,
            height: 1,
            ..area
        };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(format!("[ {} Loading... ]", self.name()))
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center),
            centered,
        );
    }
}
