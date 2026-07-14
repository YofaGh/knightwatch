use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui_image::picker::Picker;

use crate::{
    events::AppEvent,
    tabs::{self, Tab},
};

pub struct App {
    pub should_quit: bool,
    pub selected_tab: usize,
    pub tab_hit_rects: Vec<(u16, u16)>,
    pub tabs: Vec<Box<dyn Tab>>,
    /// Set whenever something happened that changed what should be on
    /// screen. The main loop checks this after every event and only calls
    /// `terminal.draw` when it's true — that's what makes this genuinely
    /// event-driven instead of redrawing on a fixed tick regardless of
    /// whether anything changed.
    pub dirty: bool,
}

impl App {
    pub fn new(picker: Picker, tabs_filter: tabs::TabsFilter) -> Self {
        let mut tabs: Vec<Box<dyn Tab>> = vec![];

        if tabs_filter.show_screen {
            tabs.push(Box::new(tabs::ScreenTab::new(picker)));
        }
        if tabs_filter.show_processes {
            tabs.push(Box::new(tabs::ProcessesTab::new()));
        }
        if tabs_filter.show_system_resources {
            tabs.push(Box::new(tabs::SystemResourcesTab::new()));
        }
        if tabs_filter.show_systemd {
            tabs.push(Box::new(tabs::SystemdTab::new()));
        }
        if tabs_filter.show_docker {
            tabs.push(Box::new(tabs::DockerTab::new()));
        }

        Self {
            should_quit: false,
            selected_tab: 0,
            tab_hit_rects: Vec::new(),
            tabs,
            dirty: false,
        }
    }

    /// Entry point for the event-driven main loop. Dispatches to the
    /// input handler or to the relevant tab(s), and marks `dirty` so the
    /// caller knows a redraw is due.
    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(ev) => {
                self.handle_event(ev);
                self.dirty = true;
            }
            AppEvent::ScreenImages(_) => {
                if let Some(tab) = self.get_tab_by_name("Screen")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::SystemSnapshot(_) => {
                if let Some(tab) = self.get_tab_by_name("System Resources")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::DockerContainers(_) => {
                if let Some(tab) = self.get_tab_by_name("Docker")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
        }
    }

    pub fn tab_titles(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name()).collect()
    }

    pub fn get_tab_by_name(&mut self, name: &str) -> Option<&mut Box<dyn Tab>> {
        self.tabs.iter_mut().find(|t| t.name() == name)
    }

    pub fn handle_event(&mut self, event: Event) {
        let consumed = self
            .tabs
            .get_mut(self.selected_tab)
            .map(|tab| tab.handle_event(&event))
            .unwrap_or(false);

        if consumed {
            return;
        }

        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true
                }
                KeyCode::Tab | KeyCode::Right => self.next_tab(),
                KeyCode::BackTab | KeyCode::Left => self.prev_tab(),
                KeyCode::Char(c) if ('1'..='9').contains(&c) => {
                    let idx = (c as usize) - ('1' as usize);
                    if idx < self.tabs.len() {
                        self.selected_tab = idx;
                    }
                }
                _ => {}
            },
            Event::Mouse(mouse) => {
                if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                    if mouse.row < 3 {
                        for (i, &(x0, x1)) in self.tab_hit_rects.iter().enumerate() {
                            if mouse.column >= x0 && mouse.column < x1 {
                                self.selected_tab = i;
                                break;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % self.tabs.len();
    }

    fn prev_tab(&mut self) {
        self.selected_tab =
            (self.selected_tab + self.tabs.len().saturating_sub(1)) % self.tabs.len();
    }
}
