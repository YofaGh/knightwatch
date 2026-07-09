use image::DynamicImage;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use ratatui_image::{StatefulImage, picker::Picker, protocol::StatefulProtocol};

use crate::events::AppEvent;

pub struct ScreenTab {
    picker: Picker,
    image: Option<StatefulProtocol>,
}

impl super::Tab for ScreenTab {
    fn name(&self) -> &'static str {
        "Screen"
    }

    fn handle_app_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::ScreenImage(image) => {
                self.set_image(image.clone());
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.image {
            Some(protocol) => {
                frame.render_stateful_widget(StatefulImage::default(), area, protocol);
            }
            None => {
                // Same "loading" placeholder as the default Tab::render,
                // shown until the first fetch succeeds.
                let mid = area.height / 2;
                let centered = Rect {
                    y: area.y + mid,
                    height: 1,
                    ..area
                };
                frame.render_widget(
                    Paragraph::new("[ Screen: waiting for first image... ]")
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(Alignment::Center),
                    centered,
                );
            }
        }
    }
}

impl ScreenTab {
    pub fn new(picker: Picker) -> Self {
        Self {
            picker,
            image: None,
        }
    }

    fn set_image(&mut self, image: DynamicImage) {
        self.image = Some(self.picker.new_resize_protocol(image));
    }
}
