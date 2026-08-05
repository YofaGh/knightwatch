use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, List, ListItem, Paragraph, Row, Table},
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;

use kw_clients::ApiClient;
use kw_types::docker::{ContainerHealth, ContainerSnapshot, ContainerStatus};
use kw_utils::format_bytes;

use crate::{
    events::{AppEvent, CommandOutcome},
    poll_panel::PollPanel,
    ui_helpers::*,
};

pub struct DockerTab {
    containers: Vec<ContainerSnapshot>,
    /// Persists the user's selection (by container id) across snapshot
    /// updates, mirroring how `ScreenTab` keeps `primary_monitor_id` sticky.
    selected_id: Option<String>,
    /// Screen-space rects of the table rows from the last render, tagged
    /// with the container id they represent, for mouse hit testing.
    row_hit_rects: Vec<(Rect, String)>,
    commands_allowed: bool,
    poll_panel: PollPanel,
    actions: ContainerActionsPanel,
}

impl DockerTab {
    pub fn new(
        allow_docker_commands: bool,
        api: Arc<ApiClient>,
        tx: Sender<AppEvent>,
        control: Arc<Mutex<crate::pollers::PollControl>>,
    ) -> Self {
        let poll_panel = PollPanel::new(
            "Docker",
            control,
            api.clone(),
            tx.clone(),
            |api| Box::pin(async move { api.docker_poll_pause().await }),
            |api| Box::pin(async move { api.docker_poll_resume().await }),
            |api, ms| Box::pin(async move { api.docker_poll_interval(ms).await }),
        );
        let actions = ContainerActionsPanel::new("Docker", api, tx);
        Self {
            containers: Vec::new(),
            selected_id: None,
            row_hit_rects: Vec::new(),
            commands_allowed: allow_docker_commands,
            poll_panel,
            actions,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.containers.is_empty() {
            return;
        }
        let current = self
            .selected_id
            .as_ref()
            .and_then(|id| self.containers.iter().position(|c| &c.id == id))
            .unwrap_or(0);
        let len = self.containers.len() as i32;
        let next = (current as i32 + delta).rem_euclid(len) as usize;
        self.selected_id = Some(self.containers[next].id.clone());
    }
}

impl super::Tab for DockerTab {
    fn name(&self) -> &'static str {
        "Docker"
    }

    fn handle_event(&mut self, event: &Event, logged_in: bool) -> bool {
        if self.commands_allowed && logged_in {
            if matches!(event, Event::Mouse(_)) {
                if self
                    .actions
                    .handle_event(event, self.selected_id.as_deref())
                {
                    return true;
                }
                // fall through: mouse missed the actions panel, let it
                // hit the container table below.
            } else if self.actions.focused {
                // Keyboard: while focused, the actions panel owns all key input.
                return self
                    .actions
                    .handle_event(event, self.selected_id.as_deref());
            } else if self.poll_panel.handle_event(event) {
                return true;
            } else if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Right
                    && self.selected_id.is_some()
                {
                    self.actions.focused = true;
                    return true;
                }
            }
        }

        match event {
            Event::Mouse(mouse) => {
                if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                    return false;
                }
                for (rect, id) in &self.row_hit_rects {
                    if mouse_hit(mouse, rect) {
                        self.selected_id = Some(id.clone());
                        self.actions.focused = false;
                        return true;
                    }
                }
                false
            }
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
            AppEvent::DockerContainers(containers) => {
                self.containers = containers.clone();
                true
            }
            AppEvent::CommandResult { tab, label, result } => {
                if *tab != "Docker" {
                    return false;
                }
                match *label {
                    "pause" | "resume" | "interval" => self.poll_panel.apply_result(label, result),
                    _ => self.actions.apply_result(label, result),
                }
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

        if self.containers.is_empty() {
            waiting_placeholder(frame, area, "Docker");
            return;
        }

        // Resolve selection, falling back to the first container if the
        // previously-selected id has disappeared (e.g. container removed).
        let selected_idx = self
            .selected_id
            .as_ref()
            .and_then(|id| self.containers.iter().position(|c| &c.id == id))
            .unwrap_or(0);
        self.selected_id = Some(self.containers[selected_idx].id.clone());

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        render_summary(frame, outer[0], &self.containers);

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);

        self.row_hit_rects = render_table(frame, main[0], &self.containers, selected_idx);

        if self.commands_allowed && logged_in {
            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(self.actions.height()),
                ])
                .split(main[1]);

            render_detail(frame, right[0], &self.containers[selected_idx]);
            self.actions.render(frame, right[1]);
        } else {
            render_detail(frame, main[1], &self.containers[selected_idx]);
        }
    }
}

fn status_icon(status: &ContainerStatus) -> &'static str {
    match status {
        ContainerStatus::Running => icon::DOT_ON,
        ContainerStatus::Paused
        | ContainerStatus::Restarting
        | ContainerStatus::Stopping
        | ContainerStatus::Removing => icon::DOT_ON,
        ContainerStatus::Created => icon::DOT_OFF,
        ContainerStatus::Dead => icon::ERR,
        ContainerStatus::Exited | ContainerStatus::Unknown(_) => icon::DOT_OFF,
    }
}

fn status_color(status: &ContainerStatus) -> Color {
    match status {
        ContainerStatus::Running => theme::SUCCESS,
        ContainerStatus::Paused
        | ContainerStatus::Restarting
        | ContainerStatus::Stopping
        | ContainerStatus::Removing => theme::WARNING,
        ContainerStatus::Created => theme::ACCENT,
        ContainerStatus::Dead => theme::DANGER,
        ContainerStatus::Exited | ContainerStatus::Unknown(_) => theme::TEXT_MUTED,
    }
}

fn health_color(health: &ContainerHealth) -> Color {
    match health {
        ContainerHealth::Healthy => theme::SUCCESS,
        ContainerHealth::Unhealthy => theme::DANGER,
        ContainerHealth::Starting => theme::WARNING,
        ContainerHealth::None | ContainerHealth::Unknown => theme::TEXT_MUTED,
    }
}

fn render_summary(frame: &mut Frame, area: Rect, containers: &[ContainerSnapshot]) {
    let inner = bordered_block(frame, area, "Docker");

    let total = containers.len();
    let running = containers
        .iter()
        .filter(|c| c.status == ContainerStatus::Running)
        .count();
    let unhealthy = containers
        .iter()
        .filter(|c| c.health == ContainerHealth::Unhealthy)
        .count();

    let total_cpu: f64 = containers
        .iter()
        .filter_map(|c| c.stats.as_ref())
        .map(|s| s.cpu_percent)
        .sum();
    let total_mem = containers
        .iter()
        .filter_map(|c| c.stats.as_ref())
        .map(|s| s.memory_bytes)
        .sum();

    let mut spans = vec![
        Span::styled(
            format!("{} ", icon::DOT_ON),
            Style::default().fg(theme::SUCCESS),
        ),
        Span::raw(format!(
            "{running}/{total} running   cpu {total_cpu:.1}%   mem {}  ",
            format_bytes(total_mem)
        )),
    ];
    if unhealthy > 0 {
        spans.push(Span::styled(
            format!(" {} {unhealthy} unhealthy ", icon::WARNING),
            Style::default()
                .fg(theme::TEXT)
                .bg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    containers: &[ContainerSnapshot],
    selected_idx: usize,
) -> Vec<(Rect, String)> {
    let inner = bordered_block(frame, area, "Containers");

    let header = Row::new(vec!["", "Name", "Image", "Status", "Health", "CPU", "Mem"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = containers
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let marker = if i == selected_idx { icon::CURSOR } else { " " };
            let (cpu_cell, mem_cell) = match &c.stats {
                Some(stats) => {
                    let mem_pct = stats.memory_percent.unwrap_or(0.0) * 100.0;
                    (
                        Cell::from(format!(
                            "{} {:>5.1}%",
                            bar(stats.cpu_percent, 8),
                            stats.cpu_percent
                        ))
                        .style(Style::default().fg(percent_color(stats.cpu_percent))),
                        Cell::from(format!("{} {}", bar(mem_pct, 8), mem_cell_text(stats)))
                            .style(Style::default().fg(percent_color(mem_pct))),
                    )
                }
                None => (
                    Cell::from("--").style(Style::default().fg(theme::TEXT_MUTED)),
                    Cell::from("--").style(Style::default().fg(theme::TEXT_MUTED)),
                ),
            };

            let row_style = if i == selected_idx {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(marker).style(Style::default().fg(theme::ACCENT)),
                Cell::from(c.name.clone()),
                Cell::from(c.image.clone()).style(Style::default().fg(theme::TEXT_DIM)),
                Cell::from(format!("{} {}", status_icon(&c.status), c.status))
                    .style(Style::default().fg(status_color(&c.status))),
                Cell::from(c.health.to_string())
                    .style(Style::default().fg(health_color(&c.health))),
                cpu_cell,
                mem_cell,
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(18),
        Constraint::Length(20),
        Constraint::Length(13),
        Constraint::Length(9),
        Constraint::Length(15),
        Constraint::Min(22),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);

    containers
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let y = inner.y + 1 + i as u16;
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
                c.id.clone(),
            ))
        })
        .collect()
}

fn mem_cell_text(stats: &kw_types::docker::ContainerStats) -> String {
    match stats.memory_percent {
        Some(pct) => format!(
            "{:>5.1}%  {}",
            pct * 100.0,
            format_bytes(stats.memory_bytes)
        ),
        None => format_bytes(stats.memory_bytes),
    }
}

fn render_detail(frame: &mut Frame, area: Rect, container: &ContainerSnapshot) {
    let inner = bordered_block(frame, area, "Details");

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // name
            Constraint::Length(1), // image
            Constraint::Length(1), // id
            Constraint::Length(1), // status/health
            Constraint::Length(1), // blank
            Constraint::Length(3), // cpu gauge
            Constraint::Length(3), // mem gauge
            Constraint::Min(0),    // io / pids list
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(container.name.clone()).style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(container.image.clone()).style(Style::default().fg(theme::TEXT_DIM)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!("id: {}", container.short_id))
            .style(Style::default().fg(theme::TEXT_MUTED)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} {}", status_icon(&container.status), container.status),
                Style::default().fg(status_color(&container.status)),
            ),
            Span::styled("   ", Style::default()),
            Span::styled(
                container.health.to_string(),
                Style::default().fg(health_color(&container.health)),
            ),
        ])),
        rows[3],
    );

    match &container.stats {
        Some(stats) => {
            let cpu_gauge = percent_gauge(
                "CPU",
                stats.cpu_percent,
                format!("{:.1}%", stats.cpu_percent),
            );
            frame.render_widget(cpu_gauge, rows[5]);

            let mem_pct = stats.memory_percent.unwrap_or(0.0) * 100.0;
            let mem_label = if stats.memory_limit_bytes > 0 {
                format!(
                    "{:.1}%  {} / {}",
                    mem_pct,
                    format_bytes(stats.memory_bytes),
                    format_bytes(stats.memory_limit_bytes)
                )
            } else {
                format!("{}", format_bytes(stats.memory_bytes))
            };
            let mem_gauge = percent_gauge("Memory", mem_pct, mem_label);
            frame.render_widget(mem_gauge, rows[6]);

            let items = vec![
                ListItem::new(format!(
                    "net   ↓ {}  ↑ {}",
                    format_bytes(stats.net_rx_bytes),
                    format_bytes(stats.net_tx_bytes)
                )),
                ListItem::new(format!(
                    "block r {}  w {}",
                    format_bytes(stats.block_read_bytes),
                    format_bytes(stats.block_write_bytes)
                )),
                ListItem::new(format!("pids  {}", stats.pid_count)),
            ];
            frame.render_widget(
                List::new(items).style(Style::default().fg(theme::TEXT_DIM)),
                rows[7],
            );
        }
        None => {
            empty_note(
                frame,
                rows[5],
                "no stats available (container not running?)",
            );
        }
    }
}

/// One entry in the actions list: its shortcut key and what it does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ContainerActionItem {
    Stop,
    Kill,
    Restart,
    Start,
    Pause,
    Unpause,
}

impl ContainerActionItem {
    /// Actions that interrupt a running container get a confirm step.
    fn is_destructive(&self) -> bool {
        matches!(
            self,
            ContainerActionItem::Stop | ContainerActionItem::Kill | ContainerActionItem::Restart
        )
    }
}

const ALL_CONTAINER_ACTIONS: &[(char, ContainerActionItem, &str)] = &[
    ('s', ContainerActionItem::Stop, "Stop"),
    ('k', ContainerActionItem::Kill, "Kill"),
    ('r', ContainerActionItem::Restart, "Restart"),
    ('a', ContainerActionItem::Start, "Start"),
    ('p', ContainerActionItem::Pause, "Pause"),
    ('u', ContainerActionItem::Unpause, "Unpause"),
];

/// Persistent, always-visible command list for the currently selected
/// container — mirrors `ProcessActionsPanel` in `process_widgets.rs`,
/// but keyed by container id (String) instead of pid (u32).
pub struct ContainerActionsPanel {
    tab: &'static str,
    api: Arc<ApiClient>,
    tx: Sender<AppEvent>,
    pub focused: bool,
    selected: usize,
    confirm_pending: Option<usize>,
    hit_rects: Vec<(Rect, usize)>,
    last_result: Option<(String, bool)>,
}

impl ContainerActionsPanel {
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

    /// Rows + border + one status line; used by the tab to size the
    /// layout chunk this panel renders into.
    pub fn height(&self) -> u16 {
        ALL_CONTAINER_ACTIONS.len() as u16 + 1 + 2
    }

    /// Call from the tab's `handle_event`, only when commands are
    /// allowed and the user is logged in. `selected_id` is the
    /// currently-selected container id.
    pub fn handle_event(&mut self, event: &Event, selected_id: Option<&str>) -> bool {
        let Some(id) = selected_id else {
            return false;
        };

        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                for (rect, idx) in &self.hit_rects {
                    if mouse_hit(mouse, rect) {
                        self.focused = true;
                        self.selected = *idx;
                        self.confirm_pending = None;
                        self.trigger(id, *idx);
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
                    self.fire_confirmed(id, idx);
                }
                _ => self.confirm_pending = None,
            }
            return true;
        }

        let len = ALL_CONTAINER_ACTIONS.len();
        match key.code {
            KeyCode::Left | KeyCode::Esc => {
                self.focused = false;
                true
            }
            KeyCode::Right => false,
            KeyCode::Up if len > 0 => {
                self.selected = (self.selected + len - 1) % len;
                true
            }
            KeyCode::Down if len > 0 => {
                self.selected = (self.selected + 1) % len;
                true
            }
            KeyCode::Enter => {
                self.trigger(id, self.selected);
                true
            }
            KeyCode::Char(c) => {
                if let Some(idx) = ALL_CONTAINER_ACTIONS.iter().position(|(k, _, _)| *k == c) {
                    self.selected = idx;
                    self.trigger(id, idx);
                }
                true
            }
            _ => true,
        }
    }

    fn trigger(&mut self, id: &str, idx: usize) {
        let Some((_, item, _)) = ALL_CONTAINER_ACTIONS.get(idx).copied() else {
            return;
        };
        if item.is_destructive() {
            self.confirm_pending = Some(idx);
        } else {
            self.fire(id, item);
        }
    }

    fn fire_confirmed(&mut self, id: &str, idx: usize) {
        if let Some((_, item, _)) = ALL_CONTAINER_ACTIONS.get(idx).copied() {
            self.fire(id, item);
        }
    }

    fn fire(&mut self, id: &str, item: ContainerActionItem) {
        let id = id.to_string();
        match item {
            ContainerActionItem::Stop => {
                let api = self.api.clone();
                let fut = Box::pin(async move { api.stop_container(&id, None).await });
                crate::commands::spawn_command(self.tx.clone(), self.tab, "stop", fut, |_| {
                    CommandOutcome::Ack
                });
            }
            ContainerActionItem::Kill => {
                let api = self.api.clone();
                let fut = Box::pin(async move { api.kill_container(&id, None).await });
                crate::commands::spawn_command(self.tx.clone(), self.tab, "kill", fut, |_| {
                    CommandOutcome::Ack
                });
            }
            ContainerActionItem::Restart => {
                let api = self.api.clone();
                let fut = Box::pin(async move { api.restart_container(&id, None).await });
                crate::commands::spawn_command(self.tx.clone(), self.tab, "restart", fut, |_| {
                    CommandOutcome::Ack
                });
            }
            ContainerActionItem::Start => {
                let api = self.api.clone();
                let fut = Box::pin(async move { api.start_container(&id).await });
                crate::commands::spawn_command(self.tx.clone(), self.tab, "start", fut, |_| {
                    CommandOutcome::Ack
                });
            }
            ContainerActionItem::Pause => {
                let api = self.api.clone();
                let fut = Box::pin(async move { api.pause_container(&id).await });
                crate::commands::spawn_command(
                    self.tx.clone(),
                    self.tab,
                    "pause-container",
                    fut,
                    |_| CommandOutcome::Ack,
                );
            }
            ContainerActionItem::Unpause => {
                let api = self.api.clone();
                let fut = Box::pin(async move { api.unpause_container(&id).await });
                crate::commands::spawn_command(
                    self.tx.clone(),
                    self.tab,
                    "unpause-container",
                    fut,
                    |_| CommandOutcome::Ack,
                );
            }
        }
    }

    /// Call from the tab's `handle_app_event` for any `CommandResult`
    /// whose label isn't one of the poll panel's.
    pub fn apply_result(&mut self, label: &str, result: &Result<CommandOutcome, String>) {
        self.last_result = Some(match result {
            Ok(_) => (format!("{label}: ok"), false),
            Err(e) => (format!("{label} failed: {e}"), true),
        });
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.focused {
            format!("{} Actions", icon::CURSOR)
        } else {
            "Actions".to_string()
        };
        let inner = bordered_block_focused(frame, area, &title, self.focused);

        let mut hit_rects = Vec::with_capacity(ALL_CONTAINER_ACTIONS.len());

        for (i, (key, _, label)) in ALL_CONTAINER_ACTIONS.iter().enumerate() {
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
                    format!(" {} {label} — confirm? [Enter] / [Esc]", icon::WARNING),
                    Style::default()
                        .fg(theme::TEXT)
                        .bg(theme::DANGER)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let marker = if is_selected { icon::CURSOR } else { " " };
                let style = if is_selected {
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT_DIM)
                };
                (format!("{marker} [{key}] {label}"), style)
            };

            frame.render_widget(Paragraph::new(text).style(style), rect);
            hit_rects.push((rect, i));
        }

        if let Some(status_y) =
            (inner.y + ALL_CONTAINER_ACTIONS.len() as u16..inner.y + inner.height).next()
        {
            if let Some((msg, is_err)) = &self.last_result {
                let rect = Rect {
                    x: inner.x,
                    y: status_y,
                    width: inner.width,
                    height: 1,
                };
                frame.render_widget(Paragraph::new(result_line(msg, *is_err)), rect);
            }
        }

        self.hit_rects = hit_rects;
    }
}
