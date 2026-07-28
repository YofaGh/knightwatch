use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::sync::{Arc, Mutex};

use kw_types::process::{ProcessSnapshot, ProcessState, ProcessTree};

use crate::{
    events::AppEvent,
    poll_panel::PollPanel,
    process_widgets::{ProcessListState, render_process_detail, render_process_table},
    ui_helpers::*,
};

pub struct ProcessesTab {
    trees: Vec<ProcessTree>,
    /// Flattened (root, then its children) rows rebuilt every time a new
    /// snapshot arrives. `depths` is parallel: 0 for a tree root, 1 for
    /// its children.
    rows: Vec<ProcessSnapshot>,
    depths: Vec<usize>,
    list: ProcessListState,
    scroll_offset: usize,
    commands_allowed: bool,
    poll_panel: PollPanel,
}

impl ProcessesTab {
    pub fn new(
        allow_process_commands: bool,
        api: Arc<kw_clients::ApiClient>,
        tx: tokio::sync::mpsc::Sender<AppEvent>,
        control: Arc<Mutex<crate::pollers::PollControl>>,
    ) -> Self {
        let poll_panel = PollPanel::new(
            "Processes",
            control,
            api,
            tx,
            |api| Box::pin(async move { api.process_poll_pause().await }),
            |api| Box::pin(async move { api.process_poll_resume().await }),
            |api, ms| Box::pin(async move { api.process_poll_interval(ms).await }),
        );
        Self {
            trees: Vec::new(),
            rows: Vec::new(),
            depths: Vec::new(),
            list: ProcessListState::default(),
            scroll_offset: 0,
            commands_allowed: allow_process_commands,
            poll_panel,
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        self.depths.clear();
        for tree in &self.trees {
            match &tree.root {
                Some(root) => self.rows.push(root.clone()),
                None => {
                    // Root has already exited but children are still
                    // being reported; keep a synthetic placeholder row
                    // so the tree stays navigable and the pid stays
                    // visible instead of silently vanishing.
                    self.rows.push(ProcessSnapshot {
                        pid: tree.root_pid,
                        name: "(exited)".to_string(),
                        state: ProcessState::Gone,
                        cpu_usage: 0.0,
                        memory_bytes: 0,
                        disk_usage: 0,
                        cwd: None,
                        cmdline: Vec::new(),
                        open_files: Vec::new(),
                        io_stats: None,
                    });
                }
            }
            self.depths.push(0);

            for child in &tree.children {
                self.rows.push(child.clone());
                self.depths.push(1);
            }
        }
    }
}

impl super::Tab for ProcessesTab {
    fn name(&self) -> &'static str {
        "Processes"
    }

    fn handle_event(&mut self, event: &Event, logged_in: bool) -> bool {
        if self.commands_allowed && logged_in && self.poll_panel.handle_event(event) {
            return true;
        }
        self.list.handle_event(event, &self.rows)
    }

    fn handle_app_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::ProcessTrees(trees) => {
                self.trees = trees.clone();
                self.rebuild_rows();
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

        if self.rows.is_empty() {
            waiting_placeholder(frame, area, "Processes");
            return;
        }

        let selected_idx = self.list.resolve_selected_idx(&self.rows);

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        render_summary(frame, outer[0], &self.trees);

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);

        let title = format!("Process Trees ({})", self.trees.len());
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

fn render_summary(frame: &mut Frame, area: Rect, trees: &[ProcessTree]) {
    let inner = bordered_block(frame, area, "Processes");

    let total_children: usize = trees.iter().map(|t| t.child_count).sum();
    let exited_roots = trees.iter().filter(|t| t.root.is_none()).count();

    let mut spans = vec![Span::raw(format!(
        "{} trees  ·  {total_children} children  ",
        trees.len()
    ))];
    if exited_roots > 0 {
        spans.push(Span::styled(
            format!(" {exited_roots} root exited "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}
