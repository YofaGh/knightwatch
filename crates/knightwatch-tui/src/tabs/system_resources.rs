use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use kw_types::resources::{self, BatteryState, CpuSnapshot, SystemHealth, SystemSnapshot};
use kw_utils::{format_bytes, format_time};

use crate::{events::AppEvent, poll_panel::PollPanel, ui_helpers::*};

/// How many samples of CPU/memory history to keep for the sparklines.
const HISTORY_LEN: usize = 90;

pub struct SystemResourcesTab {
    snapshot: Option<SystemSnapshot>,
    cpu_history: VecDeque<u64>,
    mem_history: VecDeque<u64>,
    commands_allowed: bool,
    poll_panel: PollPanel,
}

impl SystemResourcesTab {
    pub fn new(
        allow_system_resources_commands: bool,
        api: Arc<kw_clients::ApiClient>,
        tx: tokio::sync::mpsc::Sender<AppEvent>,
        control: Arc<Mutex<crate::pollers::PollControl>>,
    ) -> Self {
        let poll_panel = PollPanel::new(
            "System Resources",
            control,
            api,
            tx,
            |api| Box::pin(async move { api.systemd_poll_pause().await }),
            |api| Box::pin(async move { api.systemd_poll_resume().await }),
            |api, ms| Box::pin(async move { api.systemd_poll_interval(ms).await }),
        );
        Self {
            snapshot: None,
            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            mem_history: VecDeque::with_capacity(HISTORY_LEN),
            commands_allowed: allow_system_resources_commands,
            poll_panel,
        }
    }

    fn push_history(history: &mut VecDeque<u64>, value: f32) {
        if history.len() == HISTORY_LEN {
            history.pop_front();
        }
        history.push_back(value.round().clamp(0.0, 100.0) as u64);
    }
}

impl super::Tab for SystemResourcesTab {
    fn name(&self) -> &'static str {
        "System Resources"
    }

    fn handle_event(&mut self, event: &crossterm::event::Event, logged_in: bool) -> bool {
        if self.commands_allowed && logged_in && self.poll_panel.handle_event(event) {
            return true;
        }
        false
    }

    fn handle_app_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::SystemSnapshot(snap) => {
                Self::push_history(&mut self.cpu_history, snap.cpu.usage_percent);
                Self::push_history(&mut self.mem_history, snap.memory.used_percent);
                self.snapshot = Some(snap.clone());
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

        let snapshot = match &self.snapshot {
            Some(s) => s,
            None => {
                waiting_placeholder(frame, area, "System Resources");
                return;
            }
        };

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),      // host banner
                Constraint::Percentage(35), // cpu | memory
                Constraint::Percentage(30), // disks | networks
                Constraint::Min(8),         // gpus | battery+temps
            ])
            .split(area);

        render_host(frame, outer[0], snapshot);

        let cpu_mem = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);
        render_cpu(frame, cpu_mem[0], snapshot, &self.cpu_history);
        render_memory(frame, cpu_mem[1], snapshot, &self.mem_history);

        let disks_nets = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(outer[2]);
        render_disks(frame, disks_nets[0], &snapshot.disks);
        render_networks(frame, disks_nets[1], &snapshot.networks);

        let gpu_other = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer[3]);
        render_gpus(frame, gpu_other[0], &snapshot.gpus);
        render_battery_temps(
            frame,
            gpu_other[1],
            snapshot.battery.as_ref(),
            &snapshot.temperatures,
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn health_color(h: &SystemHealth) -> Color {
    match h {
        SystemHealth::Healthy => Color::Green,
        SystemHealth::Warning => Color::Yellow,
        SystemHealth::Critical => Color::Red,
    }
}

fn load_avg_line(cpu: &CpuSnapshot) -> String {
    match &cpu.load_avg {
        Some(la) => format!("load {:.2} {:.2} {:.2}", la.one, la.five, la.fifteen),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Section renderers
// ---------------------------------------------------------------------------

fn render_host(frame: &mut Frame, area: Rect, snap: &SystemSnapshot) {
    let inner = bordered_block(frame, area, "Host");

    let host = &snap.host;
    let health_label = format!(" {} ", snap.health);
    let line = Line::from(vec![
        Span::raw(format!(
            "{}  ·  {}  ·  kernel {}  ·  {}  ·  up {}  ·  {} procs  ",
            host.hostname.as_deref().unwrap_or("?"),
            host.os_name.as_deref().unwrap_or("?"),
            host.kernel_version.as_deref().unwrap_or("?"),
            host.cpu_arch.as_deref().unwrap_or("?"),
            format_time(host.uptime_secs),
            host.process_count,
        )),
        Span::styled(
            health_label,
            Style::default()
                .fg(Color::Black)
                .bg(health_color(&snap.health))
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

fn render_cpu(frame: &mut Frame, area: Rect, snap: &SystemSnapshot, history: &VecDeque<u64>) {
    let inner = bordered_block(frame, area, "CPU");

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // brand/freq/load line
            Constraint::Length(3), // gauge + sparkline
            Constraint::Min(0),    // per-core list
        ])
        .split(inner);

    let cpu = &snap.cpu;
    let load = load_avg_line(cpu);
    let info_line = format!(
        "{}  ·  {} MHz  ·  {} physical cores{}",
        cpu.brand,
        cpu.frequency_mhz,
        cpu.physical_core_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into()),
        if load.is_empty() {
            String::new()
        } else {
            format!("  ·  {load}")
        },
    );
    frame.render_widget(
        Paragraph::new(info_line).style(Style::default().fg(Color::Gray)),
        rows[0],
    );

    let gauge_sparkline = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(rows[1]);

    let gauge = percent_gauge(
        "",
        cpu.usage_percent as f64,
        format!("{:.1}%", cpu.usage_percent),
    );
    frame.render_widget(gauge, gauge_sparkline[0]);

    let data: Vec<u64> = history.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(&data)
        .max(100)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(sparkline, gauge_sparkline[1]);

    let items: Vec<ListItem> = cpu
        .cores
        .iter()
        .map(|c| {
            let line = format!(
                "{:<8} {} {:>5.1}%  {:>5} MHz",
                c.name,
                bar(c.usage_percent as f64, 20),
                c.usage_percent,
                c.frequency_mhz
            );
            ListItem::new(line).style(Style::default().fg(percent_color(c.usage_percent as f64)))
        })
        .collect();
    frame.render_widget(List::new(items), rows[2]);
}

fn render_memory(frame: &mut Frame, area: Rect, snap: &SystemSnapshot, history: &VecDeque<u64>) {
    let inner = bordered_block(frame, area, "Memory");

    let mem = &snap.memory;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // RAM gauge
            Constraint::Length(1), // free/available text
            Constraint::Length(3), // swap gauge
            Constraint::Min(0),    // history sparkline
        ])
        .split(inner);

    let ram_gauge = percent_gauge(
        "RAM",
        mem.used_percent as f64,
        format!(
            "{:.1}%  {} / {}",
            mem.used_percent,
            format_bytes(mem.used_bytes),
            format_bytes(mem.total_bytes)
        ),
    );
    frame.render_widget(ram_gauge, rows[0]);

    frame.render_widget(
        Paragraph::new(format!(
            "available: {}   free: {}",
            format_bytes(mem.available_bytes),
            format_bytes(mem.free_bytes)
        ))
        .style(Style::default().fg(Color::DarkGray)),
        rows[1],
    );

    if mem.swap_total_bytes > 0 {
        let swap_pct = mem.swap_used_percent.unwrap_or(0.0);
        let swap_gauge = percent_gauge(
            "Swap",
            swap_pct as f64,
            format!(
                "{:.1}%  {} / {}",
                swap_pct,
                format_bytes(mem.swap_used_bytes),
                format_bytes(mem.swap_total_bytes)
            ),
        );
        frame.render_widget(swap_gauge, rows[2]);
    } else {
        frame.render_widget(
            Paragraph::new("Swap: none").style(Style::default().fg(Color::DarkGray)),
            rows[2],
        );
    }

    let data: Vec<u64> = history.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .title("history")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(&data)
        .max(100)
        .style(Style::default().fg(Color::Magenta));
    frame.render_widget(sparkline, rows[3]);
}

fn render_disks(frame: &mut Frame, area: Rect, disks: &[resources::DiskSnapshot]) {
    let inner = bordered_block(frame, area, "Disks");

    if disks.is_empty() {
        empty_note(frame, inner, "no disks reported");
        return;
    }

    let header = Row::new(vec!["Mount", "FS", "Kind", "Usage", "Used / Total"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = disks
        .iter()
        .map(|d| {
            let color = percent_color(d.used_percent as f64);
            Row::new(vec![
                Cell::from(d.mount_point.clone()),
                Cell::from(d.file_system.clone()),
                Cell::from(d.kind.to_string()),
                Cell::from(format!(
                    "{} {:>5.1}%",
                    bar(d.used_percent as f64, 12),
                    d.used_percent
                ))
                .style(Style::default().fg(color)),
                Cell::from(format!(
                    "{} / {}",
                    format_bytes(d.used_bytes),
                    format_bytes(d.total_bytes)
                )),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(14),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(20),
        Constraint::Min(16),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn render_networks(frame: &mut Frame, area: Rect, nets: &[resources::NetworkSnapshot]) {
    let inner = bordered_block(frame, area, "Networks");

    if nets.is_empty() {
        empty_note(frame, inner, "no interfaces reported");
        return;
    }

    let header = Row::new(vec![
        "Interface",
        "↓/s",
        "↑/s",
        "Total ↓",
        "Total ↑",
        "Errs",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = nets
        .iter()
        .map(|n| {
            let errs = n.rx_errors + n.tx_errors;
            let err_style = if errs > 0 {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Row::new(vec![
                Cell::from(n.interface.clone()),
                Cell::from(format!("{}/s", format_bytes(n.rx_bytes_per_sec))),
                Cell::from(format!("{}/s", format_bytes(n.tx_bytes_per_sec))),
                Cell::from(format_bytes(n.rx_total_bytes)),
                Cell::from(format_bytes(n.tx_total_bytes)),
                Cell::from(errs.to_string()).style(err_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(5),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn render_gpus(frame: &mut Frame, area: Rect, gpus: &[resources::GpuSnapshot]) {
    let inner = bordered_block(frame, area, "GPUs");

    if gpus.is_empty() {
        empty_note(frame, inner, "no GPUs reported");
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for g in gpus {
        let mut parts = vec![Span::styled(
            g.name.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )];
        if let Some(u) = g.usage_percent {
            parts.push(Span::raw(format!("  {} {:>5.1}%", bar(u as f64, 12), u)));
        }
        lines.push(Line::from(parts));

        let mut detail = Vec::new();
        if let (Some(used), Some(total)) = (g.vram_used_bytes, g.vram_total_bytes) {
            detail.push(format!(
                "vram {}/{}",
                format_bytes(used),
                format_bytes(total)
            ));
        }
        if let Some(t) = g.temperature_celsius {
            detail.push(format!("{t:.0}°C"));
        }
        if let Some(p) = g.power_draw_watts {
            detail.push(match g.power_limit_watts {
                Some(limit) => format!("{p:.0}W / {limit:.0}W"),
                None => format!("{p:.0}W"),
            });
        }
        if !g.fan_speed_percent.is_empty() {
            let fans = g
                .fan_speed_percent
                .iter()
                .map(|f| format!("{f:.0}%"))
                .collect::<Vec<_>>()
                .join(", ");
            detail.push(format!("fans {fans}"));
        }
        if !detail.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("    {}", detail.join("  ·  ")),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_battery_temps(
    frame: &mut Frame,
    area: Rect,
    battery: Option<&resources::BatterySnapshot>,
    temps: &[resources::ThermalSnapshot],
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    let battery_inner = bordered_block(frame, rows[0], "Battery");

    match battery {
        Some(b) => {
            let color = if b.charge_percent <= 15.0 {
                Color::Red
            } else if b.charge_percent <= 30.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            let extra = match b.state {
                BatteryState::Discharging => b
                    .time_to_empty_secs
                    .map(|s| format!("  ·  {} remaining", format_time(s)))
                    .unwrap_or_default(),
                BatteryState::Charging => b
                    .time_to_full_secs
                    .map(|s| format!("  ·  full in {}", format_time(s)))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(color))
                .ratio((b.charge_percent as f64 / 100.0).clamp(0.0, 1.0))
                .label(format!("{:.0}%  {}{}", b.charge_percent, b.state, extra));
            frame.render_widget(gauge, battery_inner);
        }
        None => {
            empty_note(frame, battery_inner, "no battery");
        }
    }

    let temps_inner = bordered_block(frame, rows[1], "Temperatures");

    if temps.is_empty() {
        empty_note(frame, temps_inner, "no sensors reported");
        return;
    }

    let items: Vec<ListItem> = temps
        .iter()
        .map(|t| {
            let temp = t.temperature_celsius.unwrap_or(0.0);
            let color = match t.temperature_critical_celsius {
                Some(crit) if temp >= crit * 0.9 => Color::Red,
                Some(crit) if temp >= crit * 0.75 => Color::Yellow,
                _ => Color::Green,
            };
            let line = format!(
                "{:<28} {:>5.0}°C   (max {:>6}  crit {:>6})",
                t.label,
                temp,
                t.temperature_max_celsius
                    .map(|v| format!("{v:.0}°C"))
                    .unwrap_or_else(|| "?".into()),
                t.temperature_critical_celsius
                    .map(|v| format!("{v:.0}°C"))
                    .unwrap_or_else(|| "?".into()),
            );
            ListItem::new(line).style(Style::default().fg(color))
        })
        .collect();
    frame.render_widget(List::new(items), temps_inner);
}
