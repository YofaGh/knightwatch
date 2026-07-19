use std::sync::{Arc, Mutex};

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use kw_types::process::{ProcessSnapshot, ProcessesSortKey};

use crate::{
    events::AppEvent,
    process_widgets::{ProcessListState, render_process_detail, render_process_table},
    ui_helpers::*,
};

/// Shared poll settings for the top-processes poller. Wrapped in an
/// `Arc<Mutex<_>>` so the UI thread can update it and the poller task
/// can pick up the latest value on its next tick without any message
/// replay/coalescing logic.
#[derive(Clone, Copy, Debug)]
pub struct TopProcessesPollConfig {
    pub sort: ProcessesSortKey,
    pub limit: Option<usize>,
}

impl Default for TopProcessesPollConfig {
    fn default() -> Self {
        Self {
            sort: ProcessesSortKey::Cpu,
            limit: Some(5),
        }
    }
}

const LIMIT_STEP: usize = 10;
const LIMIT_MIN: usize = 10;

pub struct TopProcessesTab {
    /// Flat (no tree structure) list of every process across all trees
    /// from the latest snapshot, kept sorted by `sort_by`.
    rows: Vec<ProcessSnapshot>,
    /// All zero — `render_process_table` takes a depths slice so it can
    /// be shared with the tree-shaped Processes tab, but this view is flat.
    depths: Vec<usize>,
    list: ProcessListState,
    scroll_offset: usize,
    sort_by: ProcessesSortKey,
    limit: Option<usize>,
    poll_config: Arc<Mutex<TopProcessesPollConfig>>,
}

impl TopProcessesTab {
    pub fn new(poll_config: Arc<Mutex<TopProcessesPollConfig>>) -> Self {
        let cfg = *poll_config.lock().unwrap();
        Self {
            rows: Vec::new(),
            depths: Vec::new(),
            list: ProcessListState::default(),
            scroll_offset: 0,
            sort_by: cfg.sort,
            limit: cfg.limit,
            poll_config,
        }
    }

    fn rebuild_rows(&mut self, processes: &[ProcessSnapshot]) {
        self.rows = processes.to_vec();
        self.sort_rows();
    }

    fn sort_rows(&mut self) {
        match self.sort_by {
            ProcessesSortKey::Cpu => self
                .rows
                .sort_by(|a, b| b.cpu_usage.total_cmp(&a.cpu_usage)),
            ProcessesSortKey::Memory => {
                self.rows.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes))
            }
            ProcessesSortKey::Disk => self.rows.sort_by_key(|p| std::cmp::Reverse(p.disk_usage)),
        }
        self.depths = vec![0; self.rows.len()];
    }

    /// Push the current sort/limit settings out to the poller.
    fn push_config(&self) {
        *self.poll_config.lock().unwrap() = TopProcessesPollConfig {
            sort: self.sort_by,
            limit: self.limit,
        };
    }

    fn increase_limit(&mut self) {
        self.limit = Some(self.limit.unwrap_or(LIMIT_MIN) + LIMIT_STEP);
        self.push_config();
    }

    fn decrease_limit(&mut self) {
        self.limit = Some(
            self.limit
                .unwrap_or(LIMIT_MIN)
                .saturating_sub(LIMIT_STEP)
                .max(LIMIT_MIN),
        );
        self.push_config();
    }

    fn clear_limit(&mut self) {
        self.limit = None;
        self.push_config();
    }
}

impl super::Tab for TopProcessesTab {
    fn name(&self) -> &'static str {
        "Top Processes"
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                let new_sort = match key.code {
                    KeyCode::Char('c') => Some(ProcessesSortKey::Cpu),
                    KeyCode::Char('m') => Some(ProcessesSortKey::Memory),
                    KeyCode::Char('d') => Some(ProcessesSortKey::Disk),
                    _ => None,
                };
                if let Some(sort) = new_sort {
                    if sort != self.sort_by {
                        self.sort_by = sort;
                        self.sort_rows();
                        self.push_config();
                    }
                    return true;
                }

                match key.code {
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        self.increase_limit();
                        return true;
                    }
                    KeyCode::Char('-') => {
                        self.decrease_limit();
                        return true;
                    }
                    KeyCode::Char('0') => {
                        self.clear_limit();
                        return true;
                    }
                    _ => {}
                }
            }
        }
        self.list.handle_event(event, &self.rows)
    }

    fn handle_app_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::TopProcesses(processes) => {
                self.rebuild_rows(processes);
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if self.rows.is_empty() {
            waiting_placeholder(frame, area, "Top Processes");
            return;
        }

        let selected_idx = self.list.resolve_selected_idx(&self.rows);

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        render_summary(frame, outer[0], self.rows.len(), self.sort_by, self.limit);

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);

        let title = format!("Top Processes (sorted by {})", self.sort_by.to_string());
        let (hit_rects, offset) = render_process_table(
            frame,
            main[0],
            &title,
            &self.rows,
            &self.depths,
            selected_idx,
            self.scroll_offset,
        );
        self.list.set_hit_rects(hit_rects);
        self.scroll_offset = offset;

        match selected_idx {
            Some(idx) => render_process_detail(frame, main[1], &self.rows[idx]),
            None => empty_note(frame, main[1], "no process selected"),
        }
    }
}

fn render_summary(
    frame: &mut Frame,
    area: Rect,
    count: usize,
    sort_by: ProcessesSortKey,
    limit: Option<usize>,
) {
    let inner = bordered_block(frame, area, "Top Processes");
    let limit_str = match limit {
        Some(n) => n.to_string(),
        None => "all".to_string(),
    };
    let spans = vec![Span::raw(format!(
        "{count} processes  ·  sorted by {}  ·  limit {limit_str}  ·  [c]pu [m]em [d]isk  ·  [+/-] limit  ·  [0] all",
        sort_by.to_string()
    ))];
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}