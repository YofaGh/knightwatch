use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, List, ListItem, Paragraph, Row, Table},
};
use std::sync::{Arc, Mutex};

use kw_types::systemd::{UnitActiveState, UnitLoadState, UnitSnapshot};
use kw_utils::format_bytes;

use crate::{events::AppEvent, poll_panel::PollPanel, ui_helpers::*};

pub struct SystemdTab {
    units: Vec<UnitSnapshot>,
    failed_count: u32,
    active_count: u32,
    inactive_count: u32,
    /// Persists the user's selection (by unit name) across snapshot
    /// updates, mirroring how `DockerTab` keeps `selected_id` sticky.
    selected_name: Option<String>,
    /// Screen-space rects of the table rows from the last render, tagged
    /// with the unit name they represent, for mouse hit testing.
    row_hit_rects: Vec<(Rect, String)>,
    /// Index of the first visible row in the table, kept in sync with the
    /// selection so moving past the bottom/top of the viewport scrolls it.
    scroll_offset: usize,
    commands_allowed: bool,
    poll_panel: PollPanel,
}

impl SystemdTab {
    pub fn new(
        allow_systemd_commands: bool,
        api: Arc<kw_clients::ApiClient>,
        tx: tokio::sync::mpsc::Sender<AppEvent>,
        control: Arc<Mutex<crate::pollers::PollControl>>,
    ) -> Self {
        let poll_panel = PollPanel::new(
            "Systemd",
            control,
            api,
            tx,
            |api| Box::pin(async move { api.systemd_poll_pause().await }),
            |api| Box::pin(async move { api.systemd_poll_resume().await }),
            |api, ms| Box::pin(async move { api.systemd_poll_interval(ms).await }),
        );
        Self {
            units: Vec::new(),
            failed_count: 0,
            active_count: 0,
            inactive_count: 0,
            selected_name: None,
            row_hit_rects: Vec::new(),
            scroll_offset: 0,
            commands_allowed: allow_systemd_commands,
            poll_panel,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.units.is_empty() {
            return;
        }
        let current = self
            .selected_name
            .as_ref()
            .and_then(|name| self.units.iter().position(|u| &u.unit_name == name))
            .unwrap_or(0);
        let len = self.units.len() as i32;
        let next = (current as i32 + delta).rem_euclid(len) as usize;
        self.selected_name = Some(self.units[next].unit_name.clone());
    }
}

impl super::Tab for SystemdTab {
    fn name(&self) -> &'static str {
        "Systemd"
    }

    fn handle_event(&mut self, event: &Event, logged_in: bool) -> bool {
        if self.commands_allowed && logged_in && self.poll_panel.handle_event(event) {
            return true;
        }
        match event {
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    for (rect, name) in &self.row_hit_rects {
                        if mouse_hit(mouse, rect) {
                            self.selected_name = Some(name.clone());
                            return true;
                        }
                    }
                    false
                }
                MouseEventKind::ScrollDown => {
                    self.move_selection(1);
                    true
                }
                MouseEventKind::ScrollUp => {
                    self.move_selection(-1);
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
                        self.move_selection(-1);
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.move_selection(1);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn handle_app_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::SystemdSnapshot(snapshot) => {
                self.units = snapshot.units.clone();
                self.failed_count = snapshot.failed_count;
                self.active_count = snapshot.active_count;
                self.inactive_count = snapshot.inactive_count;
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

        if self.units.is_empty() {
            waiting_placeholder(frame, area, "Systemd");
            return;
        }

        let selected_idx = self
            .selected_name
            .as_ref()
            .and_then(|name| self.units.iter().position(|u| &u.unit_name == name))
            .unwrap_or(0);
        self.selected_name = Some(self.units[selected_idx].unit_name.clone());

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        render_summary(
            frame,
            outer[0],
            self.active_count,
            self.inactive_count,
            self.failed_count,
        );

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);

        let (hit_rects, scroll_offset) = render_table(
            frame,
            main[0],
            &self.units,
            selected_idx,
            self.scroll_offset,
        );
        self.row_hit_rects = hit_rects;
        self.scroll_offset = scroll_offset;
        render_detail(frame, main[1], &self.units[selected_idx]);
    }
}

fn active_state_color(state: &UnitActiveState) -> Color {
    match state {
        UnitActiveState::Active => Color::Green,
        UnitActiveState::Reloading
        | UnitActiveState::Activating
        | UnitActiveState::Deactivating => Color::Yellow,
        UnitActiveState::Inactive => Color::DarkGray,
        UnitActiveState::Failed => Color::Red,
    }
}

fn load_state_color(state: &UnitLoadState) -> Color {
    match state {
        UnitLoadState::Loaded => Color::Green,
        UnitLoadState::NotFound => Color::DarkGray,
        UnitLoadState::BadSetting | UnitLoadState::Error => Color::Red,
        UnitLoadState::Masked => Color::Yellow,
    }
}

fn format_cpu_ns(ns: u64) -> String {
    let secs = ns as f64 / 1_000_000_000.0;
    if secs >= 1.0 {
        format!("{secs:.2}s")
    } else {
        format!("{:.0}ms", secs * 1000.0)
    }
}

fn render_summary(
    frame: &mut Frame,
    area: Rect,
    active_count: u32,
    inactive_count: u32,
    failed_count: u32,
) {
    let inner = bordered_block(frame, area, "Systemd");

    let mut spans = vec![Span::raw(format!(
        "{active_count} active  ·  {inactive_count} inactive  "
    ))];
    if failed_count > 0 {
        spans.push(Span::styled(
            format!(" {failed_count} failed "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// Renders the unit table, scrolling the viewport so `selected_idx` is
/// always visible. Returns the row hit-rects (for mouse selection) and the
/// possibly-adjusted scroll offset to persist for the next render.
fn render_table(
    frame: &mut Frame,
    area: Rect,
    units: &[UnitSnapshot],
    selected_idx: usize,
    scroll_offset: usize,
) -> (Vec<(Rect, String)>, usize) {
    let title = format!("Units ({}/{})", selected_idx + 1, units.len());
    let inner = bordered_block(frame, area, &title);
    let visible_rows = inner.height.saturating_sub(1) as usize;
    let max_offset = units.len().saturating_sub(visible_rows.max(1));
    let mut offset = scroll_offset.min(max_offset);
    if selected_idx < offset {
        offset = selected_idx;
    } else if visible_rows > 0 && selected_idx >= offset + visible_rows {
        offset = selected_idx + 1 - visible_rows;
    }

    let end = (offset + visible_rows).min(units.len());
    let visible_units = &units[offset..end];

    let header = Row::new(vec!["", "Unit", "Type", "Load", "Active", "Sub", "Mem"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = visible_units
        .iter()
        .enumerate()
        .map(|(visible_i, u)| {
            let i = offset + visible_i;
            let marker = if i == selected_idx { ">" } else { " " };

            let row_style = if i == selected_idx {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            let mem_cell = match u.memory_bytes {
                Some(bytes) => Cell::from(format_bytes(bytes)),
                None => Cell::from("--").style(Style::default().fg(Color::DarkGray)),
            };

            Row::new(vec![
                Cell::from(marker),
                Cell::from(u.unit_name.clone()),
                Cell::from(u.unit_type.to_string()),
                Cell::from(u.load_state.to_string())
                    .style(Style::default().fg(load_state_color(&u.load_state))),
                Cell::from(u.active_state.as_str())
                    .style(Style::default().fg(active_state_color(&u.active_state))),
                Cell::from(u.sub_state.clone()),
                mem_cell,
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(32),
        Constraint::Length(8),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);

    let hit_rects = visible_units
        .iter()
        .enumerate()
        .filter_map(|(visible_i, u)| {
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
                u.unit_name.clone(),
            ))
        })
        .collect();

    (hit_rects, offset)
}

fn render_detail(frame: &mut Frame, area: Rect, unit: &UnitSnapshot) {
    let inner = bordered_block(frame, area, "Details");

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // name
            Constraint::Length(1), // description
            Constraint::Length(1), // type
            Constraint::Length(1), // load/active/sub
            Constraint::Length(1), // blank
            Constraint::Length(1), // pid
            Constraint::Length(1), // memory
            Constraint::Length(1), // cpu
            Constraint::Length(1), // restarts
            Constraint::Length(1), // since
            Constraint::Length(1), // blank
            Constraint::Min(0),    // fragment path
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(unit.unit_name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(unit.description.clone()).style(Style::default().fg(Color::Gray)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!("type: {}", unit.unit_type))
            .style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                unit.load_state.to_string(),
                Style::default().fg(load_state_color(&unit.load_state)),
            ),
            Span::raw("  ·  "),
            Span::styled(
                unit.active_state.as_str(),
                Style::default().fg(active_state_color(&unit.active_state)),
            ),
            Span::raw("  ·  "),
            Span::raw(unit.sub_state.clone()),
        ])),
        rows[3],
    );

    match unit.main_pid {
        Some(pid) => frame.render_widget(Paragraph::new(format!("pid: {pid}")), rows[5]),
        None => frame.render_widget(
            Paragraph::new("pid: --").style(Style::default().fg(Color::DarkGray)),
            rows[5],
        ),
    }

    match unit.memory_bytes {
        Some(bytes) => frame.render_widget(
            Paragraph::new(format!("memory: {}", format_bytes(bytes))),
            rows[6],
        ),
        None => frame.render_widget(
            Paragraph::new("memory: --").style(Style::default().fg(Color::DarkGray)),
            rows[6],
        ),
    }

    match unit.cpu_usage_ns {
        Some(ns) => frame.render_widget(
            Paragraph::new(format!("cpu time: {}", format_cpu_ns(ns))),
            rows[7],
        ),
        None => frame.render_widget(
            Paragraph::new("cpu time: --").style(Style::default().fg(Color::DarkGray)),
            rows[7],
        ),
    }

    match unit.restart_count {
        Some(count) => {
            let style = if count > 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(format!("restarts: {count}")).style(style),
                rows[8],
            );
        }
        None => frame.render_widget(
            Paragraph::new("restarts: --").style(Style::default().fg(Color::DarkGray)),
            rows[8],
        ),
    }

    match &unit.since {
        Some(since) => frame.render_widget(
            Paragraph::new(format!("since: {since}")).style(Style::default().fg(Color::Gray)),
            rows[9],
        ),
        None => frame.render_widget(
            Paragraph::new("since: --").style(Style::default().fg(Color::DarkGray)),
            rows[9],
        ),
    }

    match &unit.fragment_path {
        Some(path) => {
            let items = vec![ListItem::new(format!("fragment: {path}"))];
            frame.render_widget(
                List::new(items).style(Style::default().fg(Color::DarkGray)),
                rows[11],
            );
        }
        None => {
            empty_note(frame, rows[11], "no fragment path reported");
        }
    }
}
