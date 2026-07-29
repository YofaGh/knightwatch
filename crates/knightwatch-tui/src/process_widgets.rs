// Shared building blocks for any tab that renders a list of
// `ProcessSnapshot`s: a selectable/scrollable table (with mouse hit
// testing, mirroring how `DockerTab`/`SystemdTab` do it) and a detail
// panel. `ProcessesTab` (tree view) and `TopProcessesTab` (flat, sorted
// view) both build on this.

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, List, ListItem, Paragraph, Row, Table},
};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use kw_clients::ApiClient;
use kw_types::process::{ProcessSignal, ProcessSnapshot, ProcessState};
use kw_utils::format_bytes;

use crate::{
    events::{AppEvent, CommandOutcome},
    ui_helpers::*,
};

/// Selection + mouse-hit-rect bookkeeping shared by any process list tab.
/// Selection is tracked by pid (not index) so it survives snapshot
/// updates that reorder or drop rows, same pattern as `selected_id` in
/// `DockerTab` and `selected_name` in `SystemdTab`.
#[derive(Default)]
pub struct ProcessListState {
    pub selected_pid: Option<u32>,
    row_hit_rects: Vec<(Rect, u32)>,
}

impl ProcessListState {
    pub fn handle_event(&mut self, event: &Event, processes: &[ProcessSnapshot]) -> bool {
        match event {
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    for (rect, pid) in &self.row_hit_rects {
                        if mouse_hit(mouse, rect) {
                            self.selected_pid = Some(*pid);
                            return true;
                        }
                    }
                    false
                }
                MouseEventKind::ScrollDown => {
                    self.move_selection(1, processes);
                    true
                }
                MouseEventKind::ScrollUp => {
                    self.move_selection(-1, processes);
                    true
                }
                _ => false,
            },
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    return false;
                }
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.move_selection(-1, processes);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.move_selection(1, processes);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn move_selection(&mut self, delta: i32, processes: &[ProcessSnapshot]) {
        if processes.is_empty() {
            return;
        }
        let current = self
            .selected_pid
            .and_then(|pid| processes.iter().position(|p| p.pid == pid))
            .unwrap_or(0);
        let len = processes.len() as i32;
        let next = (current as i32 + delta).rem_euclid(len) as usize;
        self.selected_pid = Some(processes[next].pid);
    }

    /// Resolves the current selection to an index, falling back to the
    /// first row and healing `selected_pid` if the previous selection
    /// has disappeared (process exited / fell out of the top-N).
    /// Mirrors the `selected_idx` fallback pattern in `DockerTab::render`.
    pub fn resolve_selected_idx(&mut self, processes: &[ProcessSnapshot]) -> Option<usize> {
        if processes.is_empty() {
            self.selected_pid = None;
            return None;
        }
        let idx = self
            .selected_pid
            .and_then(|pid| processes.iter().position(|p| p.pid == pid))
            .unwrap_or(0);
        self.selected_pid = Some(processes[idx].pid);
        Some(idx)
    }

    /// Called by the tab after `render_process_table` to store the new
    /// hit rects for the next mouse event.
    pub fn set_hit_rects(&mut self, rects: Vec<(Rect, u32)>) {
        self.row_hit_rects = rects;
    }
}

pub fn state_color(state: &ProcessState) -> Color {
    match state {
        ProcessState::Running => Color::Green,
        ProcessState::Sleeping => Color::DarkGray,
        ProcessState::Other(_) => Color::Yellow,
        ProcessState::Gone => Color::Red,
    }
}

/// Renders a process table. `depths` is parallel to `processes` and
/// controls indentation (0 = top-level); pass an all-zero slice for flat
/// lists like Top Processes, or 0/1 for a root+children tree like
/// Processes. Handles its own vertical scrolling to keep `selected_idx`
/// visible, same approach as `SystemdTab::render_table`.
///
/// Returns the row hit-rects (pid-tagged, for mouse selection) and the
/// possibly-adjusted scroll offset to persist for the next render.
pub fn render_process_table(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    processes: &[ProcessSnapshot],
    depths: &[usize],
    selected_idx: Option<usize>,
    scroll_offset: usize,
) -> (Vec<(Rect, u32)>, usize) {
    let inner = bordered_block(frame, area, title);
    let visible_rows = inner.height.saturating_sub(1) as usize;
    let max_offset = processes.len().saturating_sub(visible_rows.max(1));
    let mut offset = scroll_offset.min(max_offset);
    if let Some(idx) = selected_idx {
        if idx < offset {
            offset = idx;
        } else if visible_rows > 0 && idx >= offset + visible_rows {
            offset = idx + 1 - visible_rows;
        }
    }

    let end = (offset + visible_rows).min(processes.len());
    let visible = &processes[offset..end];
    let visible_depths = &depths[offset..end];

    let header = Row::new(vec!["", "PID", "Name", "State", "CPU", "Mem", "Disk"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = visible
        .iter()
        .zip(visible_depths)
        .enumerate()
        .map(|(visible_i, (p, depth))| {
            let i = offset + visible_i;
            let is_selected = selected_idx == Some(i);
            let marker = if is_selected { ">" } else { " " };
            let row_style = if is_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let indent = "  ".repeat(*depth);

            Row::new(vec![
                Cell::from(marker),
                Cell::from(p.pid.to_string()),
                Cell::from(format!("{indent}{}", p.name)),
                Cell::from(p.state.to_string()).style(Style::default().fg(state_color(&p.state))),
                Cell::from(format!(
                    "{} {:>5.1}%",
                    bar(p.cpu_usage as f64, 8),
                    p.cpu_usage
                ))
                .style(Style::default().fg(percent_color(p.cpu_usage as f64))),
                Cell::from(format_bytes(p.memory_bytes)),
                Cell::from(format_bytes(p.disk_usage)),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Min(20),
        Constraint::Length(11),
        Constraint::Length(15),
        Constraint::Length(12),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);

    let hit_rects = visible
        .iter()
        .enumerate()
        .filter_map(|(visible_i, p)| {
            let y = inner.y + 1 + visible_i as u16;
            if y >= inner.y + inner.height {
                return None;
            }
            Some((
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
                p.pid,
            ))
        })
        .collect();

    (hit_rects, offset)
}

/// Renders the detail panel for a single selected process. Rows that
/// depend on Linux-only / optional data (`cwd`, `cmdline`, `io_stats`)
/// are only allocated space when present.
pub fn render_process_detail(frame: &mut Frame, area: Rect, process: &ProcessSnapshot) {
    let inner = bordered_block(frame, area, "Details");

    let mut constraints = vec![
        Constraint::Length(1), // name
        Constraint::Length(1), // pid / state
        Constraint::Length(1), // cpu
        Constraint::Length(1), // mem
        Constraint::Length(1), // disk
        Constraint::Length(1), // blank
    ];
    if process.cwd.is_some() {
        constraints.push(Constraint::Length(1));
    }
    if !process.cmdline.is_empty() {
        constraints.push(Constraint::Length(1));
    }
    if process.io_stats.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(0)); // open files list

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut idx = 0;
    frame.render_widget(
        Paragraph::new(process.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
        rows[idx],
    );
    idx += 1;

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(format!("pid: {}  ·  ", process.pid)),
            Span::styled(
                process.state.to_string(),
                Style::default().fg(state_color(&process.state)),
            ),
        ])),
        rows[idx],
    );
    idx += 1;

    frame.render_widget(
        Paragraph::new(format!("cpu: {:.1}%", process.cpu_usage))
            .style(Style::default().fg(percent_color(process.cpu_usage as f64))),
        rows[idx],
    );
    idx += 1;

    frame.render_widget(
        Paragraph::new(format!("memory: {}", format_bytes(process.memory_bytes))),
        rows[idx],
    );
    idx += 1;

    frame.render_widget(
        Paragraph::new(format!("disk: {}", format_bytes(process.disk_usage)))
            .style(Style::default().fg(Color::Gray)),
        rows[idx],
    );
    idx += 1;

    idx += 1; // blank separator row

    if let Some(cwd) = &process.cwd {
        frame.render_widget(
            Paragraph::new(format!("cwd: {cwd}")).style(Style::default().fg(Color::DarkGray)),
            rows[idx],
        );
        idx += 1;
    }

    if !process.cmdline.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("cmd: {}", process.cmdline.join(" ")))
                .style(Style::default().fg(Color::DarkGray)),
            rows[idx],
        );
        idx += 1;
    }

    if let Some(io) = &process.io_stats {
        frame.render_widget(
            Paragraph::new(format!(
                "io: read {} ({} chars)  ·  write {} ({} chars)",
                format_bytes(io.read_bytes),
                format_bytes(io.read_chars),
                format_bytes(io.write_bytes),
                format_bytes(io.write_chars),
            ))
            .style(Style::default().fg(Color::Gray)),
            rows[idx],
        );
        idx += 1;
    }

    if process.open_files.is_empty() {
        empty_note(frame, rows[idx], "no open files reported");
    } else {
        let items: Vec<ListItem> = process
            .open_files
            .iter()
            .map(|fd| ListItem::new(format!("{:<4} {:<7} {}", fd.fd, fd.fd_type, fd.target)))
            .collect();
        frame.render_widget(
            List::new(items).style(Style::default().fg(Color::Gray)),
            rows[idx],
        );
    }
}

/// One entry in the actions list: its shortcut key, what it does, and
/// its display label.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ActionItem {
    Signal(ProcessSignal),
    KillTree,
    Track,
    Untrack,
}

impl ActionItem {
    /// Destructive actions get a confirm step; everything else fires
    /// immediately on activation.
    fn is_destructive(&self) -> bool {
        matches!(
            self,
            ActionItem::Signal(ProcessSignal::Kill)
                | ActionItem::Signal(ProcessSignal::Term)
                | ActionItem::KillTree
        )
    }
}

const ALL_ACTIONS: &[(char, ActionItem, &str)] = &[
    (
        'k',
        ActionItem::Signal(ProcessSignal::Kill),
        "Kill (SIGKILL)",
    ),
    (
        't',
        ActionItem::Signal(ProcessSignal::Term),
        "Terminate (SIGTERM)",
    ),
    (
        'i',
        ActionItem::Signal(ProcessSignal::Interrupt),
        "Interrupt (SIGINT)",
    ),
    (
        's',
        ActionItem::Signal(ProcessSignal::Stop),
        "Stop (SIGSTOP)",
    ),
    (
        'c',
        ActionItem::Signal(ProcessSignal::Continue),
        "Continue (SIGCONT)",
    ),
    ('x', ActionItem::KillTree, "Kill process tree"),
    ('w', ActionItem::Track, "Track pid"),
    ('u', ActionItem::Untrack, "Untrack pid"),
];

/// A persistent, always-visible command list for the currently selected
/// process — rendered as its own bordered box under the detail panel.
/// Rows are mouse-clickable at any time; `→` from the process table (or
/// a click) focuses it for arrow-key navigation, `Enter`/click activates
/// the highlighted row, and its shortcut letter (`k`/`t`/`i`/... below)
/// activates a row directly once focused, without needing to navigate to
/// it. `←`/`Esc` returns focus to the process table.
pub struct ProcessActionsPanel {
    tab: &'static str,
    api: Arc<ApiClient>,
    tx: Sender<AppEvent>,
    pub focused: bool,
    selected: usize,
    confirm_pending: Option<usize>,
    hit_rects: Vec<(Rect, usize)>,
    last_result: Option<(String, bool)>,
}

impl ProcessActionsPanel {
    pub fn new(tab: &'static str, api: Arc<ApiClient>, tx: Sender<AppEvent>) -> Self {
        Self {
            tab,
            api,
            tx,
            focused: false,
            selected: 0,
            confirm_pending: None,
            hit_rects: Vec::new(),
            last_result: None,
        }
    }

    fn visible_items(&self) -> Vec<(char, ActionItem, &'static str)> {
        ALL_ACTIONS
            .iter()
            .filter(|(_, item, _)| match item {
                ActionItem::Signal(sig) => sig.is_supported(),
                _ => true,
            })
            .copied()
            .collect()
    }

    /// Rows + border + one status line; used by the tab to size the
    /// layout chunk this panel renders into.
    pub fn height(&self) -> u16 {
        self.visible_items().len() as u16 + 1 + 2
    }

    /// Call from the tab's `handle_event`, only when commands are
    /// allowed and the user is logged in. `selected_pid` is
    /// `self.list.selected_pid` from the tab's `ProcessListState`.
    /// Mouse clicks on a row are handled regardless of focus; keyboard
    /// navigation only applies once focused (tab is responsible for
    /// routing `→` to focus this panel — see `focused`).
    pub fn handle_event(&mut self, event: &Event, selected_pid: Option<u32>) -> bool {
        let Some(pid) = selected_pid else {
            return false;
        };

        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                for (rect, idx) in &self.hit_rects {
                    if mouse_hit(mouse, rect) {
                        self.focused = true;
                        self.selected = *idx;
                        self.confirm_pending = None;
                        self.trigger(pid, *idx);
                        return true;
                    }
                }
            }
            return false;
        }

        if !self.focused {
            return false;
        }

        let Event::Key(key) = event else { return false };
        if key.kind != KeyEventKind::Press {
            return false;
        }

        if let Some(idx) = self.confirm_pending {
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_pending = None;
                    self.fire_confirmed(pid, idx);
                }
                _ => self.confirm_pending = None,
            }
            return true;
        }

        let items = self.visible_items();
        match key.code {
            KeyCode::Left | KeyCode::Esc => self.focused = false,
            KeyCode::Up if !items.is_empty() => {
                self.selected = (self.selected + items.len() - 1) % items.len();
            }
            KeyCode::Down if !items.is_empty() => {
                self.selected = (self.selected + 1) % items.len();
            }
            KeyCode::Enter => self.trigger(pid, self.selected),
            KeyCode::Char(c) => {
                if let Some(idx) = items.iter().position(|(k, _, _)| *k == c) {
                    self.selected = idx;
                    self.trigger(pid, idx);
                }
            }
            _ => {}
        }
        true
    }

    fn trigger(&mut self, pid: u32, idx: usize) {
        let items = self.visible_items();
        let Some((_, item, _)) = items.get(idx).copied() else {
            return;
        };
        if item.is_destructive() {
            self.confirm_pending = Some(idx);
        } else {
            self.fire(pid, item);
        }
    }

    fn fire_confirmed(&mut self, pid: u32, idx: usize) {
        let items = self.visible_items();
        if let Some((_, item, _)) = items.get(idx).copied() {
            self.fire(pid, item);
        }
    }

    fn fire(&mut self, pid: u32, item: ActionItem) {
        match item {
            ActionItem::Signal(signal) => self.send_signal(pid, signal),
            ActionItem::KillTree => self.fire_kill_tree(pid),
            ActionItem::Track => self.fire_track(pid),
            ActionItem::Untrack => self.fire_untrack(pid),
        }
    }

    fn send_signal(&mut self, pid: u32, signal: ProcessSignal) {
        let label: &'static str = match signal {
            ProcessSignal::Kill => "kill",
            ProcessSignal::Term => "term",
            ProcessSignal::Interrupt => "interrupt",
            ProcessSignal::Stop => "stop",
            ProcessSignal::Continue => "continue",
        };
        let api = self.api.clone();
        let fut = Box::pin(async move { api.kill_process(pid, signal).await });
        crate::commands::spawn_command(self.tx.clone(), self.tab, label, fut, |_| {
            CommandOutcome::Ack
        });
    }

    fn fire_kill_tree(&mut self, pid: u32) {
        let api = self.api.clone();
        let fut = Box::pin(async move { api.kill_process_tree(pid).await });
        crate::commands::spawn_command(self.tx.clone(), self.tab, "kill-tree", fut, |pids| {
            CommandOutcome::KillTree(pids)
        });
    }

    fn fire_track(&mut self, pid: u32) {
        let api = self.api.clone();
        let fut = Box::pin(async move { api.track_pid(pid).await });
        crate::commands::spawn_command(self.tx.clone(), self.tab, "track", fut, |_| {
            CommandOutcome::Ack
        });
    }

    fn fire_untrack(&mut self, pid: u32) {
        let api = self.api.clone();
        let fut = Box::pin(async move { api.untrack_pid(pid).await });
        crate::commands::spawn_command(self.tx.clone(), self.tab, "untrack", fut, |_| {
            CommandOutcome::Ack
        });
    }

    /// Call from the tab's `handle_app_event` for any `CommandResult`
    /// whose label isn't one of the poll panel's.
    pub fn apply_result(&mut self, label: &str, result: &Result<CommandOutcome, String>) {
        self.last_result = Some(match result {
            Ok(CommandOutcome::Ack) => (format!("{label}: ok"), false),
            Ok(CommandOutcome::KillTree(pids)) => (
                format!("kill-tree: {} process(es) killed", pids.len()),
                false,
            ),
            Err(e) => (format!("{label} failed: {e}"), true),
        });
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.focused {
            "Actions ●"
        } else {
            "Actions"
        };
        let inner = bordered_block(frame, area, title);

        let items = self.visible_items();
        let mut hit_rects = Vec::with_capacity(items.len());

        for (i, (key, _, label)) in items.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let rect = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };

            let is_confirming = self.confirm_pending == Some(i);
            let is_selected = self.focused && self.selected == i;

            let (text, style) = if is_confirming {
                (
                    format!("  {label} — confirm? [Enter] / [Esc]"),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let marker = if is_selected { ">" } else { " " };
                let style = if is_selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::Gray)
                };
                (format!("{marker} [{key}] {label}"), style)
            };

            frame.render_widget(Paragraph::new(text).style(style), rect);
            hit_rects.push((rect, i));
        }

        if let Some(status_y) = (inner.y + items.len() as u16..inner.y + inner.height).next() {
            if let Some((msg, is_err)) = &self.last_result {
                let rect = Rect {
                    x: inner.x,
                    y: status_y,
                    width: inner.width,
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(msg.clone()).style(Style::default().fg(if *is_err {
                        Color::Red
                    } else {
                        Color::Green
                    })),
                    rect,
                );
            }
        }

        self.hit_rects = hit_rects;
    }
}
