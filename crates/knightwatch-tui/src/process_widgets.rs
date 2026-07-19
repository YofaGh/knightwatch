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

use kw_types::process::{ProcessSnapshot, ProcessState};
use kw_utils::format_bytes;

use crate::ui_helpers::*;

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
                        let hit = mouse.column >= rect.x
                            && mouse.column < rect.x + rect.width
                            && mouse.row >= rect.y
                            && mouse.row < rect.y + rect.height;
                        if hit {
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
