use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, List, ListItem, Paragraph, Row, Table},
};

use kw_types::docker::{ContainerHealth, ContainerSnapshot, ContainerStatus};
use kw_utils::format_bytes;

use crate::{events::AppEvent, ui_helpers::*};

pub struct DockerTab {
    containers: Vec<ContainerSnapshot>,
    /// Persists the user's selection (by container id) across snapshot
    /// updates, mirroring how `ScreenTab` keeps `primary_monitor_id` sticky.
    selected_id: Option<String>,
    /// Screen-space rects of the table rows from the last render, tagged
    /// with the container id they represent, for mouse hit testing.
    row_hit_rects: Vec<(Rect, String)>,
}

impl DockerTab {
    pub fn new() -> Self {
        Self {
            containers: Vec::new(),
            selected_id: None,
            row_hit_rects: Vec::new(),
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

    fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Mouse(mouse) => {
                if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                    return false;
                }
                for (rect, id) in &self.row_hit_rects {
                    let hit = mouse.column >= rect.x
                        && mouse.column < rect.x + rect.width
                        && mouse.row >= rect.y
                        && mouse.row < rect.y + rect.height;
                    if hit {
                        self.selected_id = Some(id.clone());
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
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
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
        render_detail(frame, main[1], &self.containers[selected_idx]);
    }
}

impl Default for DockerTab {
    fn default() -> Self {
        Self::new()
    }
}

fn status_color(status: &ContainerStatus) -> Color {
    match status {
        ContainerStatus::Running => Color::Green,
        ContainerStatus::Paused
        | ContainerStatus::Restarting
        | ContainerStatus::Stopping
        | ContainerStatus::Removing => Color::Yellow,
        ContainerStatus::Created => Color::Cyan,
        ContainerStatus::Dead => Color::Red,
        ContainerStatus::Exited | ContainerStatus::Unknown(_) => Color::DarkGray,
    }
}

fn health_color(health: &ContainerHealth) -> Color {
    match health {
        ContainerHealth::Healthy => Color::Green,
        ContainerHealth::Unhealthy => Color::Red,
        ContainerHealth::Starting => Color::Yellow,
        ContainerHealth::None | ContainerHealth::Unknown => Color::DarkGray,
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

    let mut spans = vec![Span::raw(format!(
        "{running}/{total} running  ·  cpu {total_cpu:.1}%  ·  mem {}  ",
        format_bytes(total_mem)
    ))];
    if unhealthy > 0 {
        spans.push(Span::styled(
            format!(" {unhealthy} unhealthy "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
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
            let marker = if i == selected_idx { ">" } else { " " };
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
                    Cell::from("--").style(Style::default().fg(Color::DarkGray)),
                    Cell::from("--").style(Style::default().fg(Color::DarkGray)),
                ),
            };

            let row_style = if i == selected_idx {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(marker),
                Cell::from(c.name.clone()),
                Cell::from(c.image.clone()),
                Cell::from(c.status.to_string())
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
        Constraint::Length(11),
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
        Paragraph::new(container.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(container.image.clone()).style(Style::default().fg(Color::Gray)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!("id: {}", container.short_id))
            .style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                container.status.to_string(),
                Style::default().fg(status_color(&container.status)),
            ),
            Span::raw("  ·  "),
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
                List::new(items).style(Style::default().fg(Color::Gray)),
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
