use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
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
use tokio::sync::mpsc::Sender;

use kw_clients::ApiClient;
use kw_types::resources::{
    self, BatteryState, CpuSnapshot, RefreshMask, SystemHealth, SystemSnapshot, Thresholds,
};
use kw_utils::{format_bytes, format_time};

use crate::{
    events::{AppEvent, CommandOutcome},
    poll_panel::PollPanel,
    ui_helpers::*,
};

/// How many samples of CPU/memory history to keep for the sparklines.
const HISTORY_LEN: usize = 90;

pub struct SystemResourcesTab {
    snapshot: Option<SystemSnapshot>,
    cpu_history: VecDeque<u64>,
    mem_history: VecDeque<u64>,
    commands_allowed: bool,
    poll_panel: PollPanel,
    settings: ResourceSettingsPanel,
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
            api.clone(),
            tx.clone(),
            |api| Box::pin(async move { api.systemd_poll_pause().await }),
            |api| Box::pin(async move { api.systemd_poll_resume().await }),
            |api, ms| Box::pin(async move { api.systemd_poll_interval(ms).await }),
        );
        let settings = ResourceSettingsPanel::new("System Resources", api, tx);
        Self {
            snapshot: None,
            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            mem_history: VecDeque::with_capacity(HISTORY_LEN),
            commands_allowed: allow_system_resources_commands,
            poll_panel,
            settings,
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

    fn handle_event(&mut self, event: &Event, logged_in: bool) -> bool {
        if self.commands_allowed && logged_in {
            if matches!(event, Event::Mouse(_)) {
                if self.settings.handle_event(event) {
                    return true;
                }
                return false;
            }

            // Keyboard: while focused, the settings panel owns all key input.
            if self.settings.focused {
                return self.settings.handle_event(event);
            }

            if self.poll_panel.handle_event(event) {
                return true;
            }

            // No natural "selected row" to arrow in from here (unlike
            // Processes/Docker), so Tab is the dedicated key to focus
            // the settings panel.
            if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Tab {
                    self.settings.focused = true;
                    return true;
                }
            }
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
            AppEvent::CommandResult { tab, label, result } => {
                if *tab != "System Resources" {
                    return false;
                }
                match *label {
                    "pause" | "resume" | "interval" => self.poll_panel.apply_result(label, result),
                    _ => self.settings.apply_result(label, result),
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

        let snapshot = match &self.snapshot {
            Some(s) => s,
            None => {
                waiting_placeholder(frame, area, "System Resources");
                return;
            }
        };

        let show_settings = self.commands_allowed && logged_in;
        let mut constraints = vec![
            Constraint::Length(3),      // host banner
            Constraint::Percentage(35), // cpu | memory
            Constraint::Percentage(30), // disks | networks
            Constraint::Min(8),         // gpus | battery+temps
        ];
        if show_settings {
            constraints.push(Constraint::Length(self.settings.height()));
        }
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
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
        if show_settings {
            self.settings.render(frame, outer[4]);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn health_color(h: &SystemHealth) -> Color {
    match h {
        SystemHealth::Healthy => theme::SUCCESS,
        SystemHealth::Warning => theme::WARNING,
        SystemHealth::Critical => theme::DANGER,
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
            "{}   {}   kernel {}   {}   up {}   {} procs  ",
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
        "{}   {} MHz   {} physical cores{}",
        cpu.brand,
        cpu.frequency_mhz,
        cpu.physical_core_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into()),
        if load.is_empty() {
            String::new()
        } else {
            format!("   {load}")
        },
    );
    frame.render_widget(
        Paragraph::new(info_line).style(Style::default().fg(theme::TEXT_DIM)),
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
                .border_style(Style::default().fg(theme::ACCENT_MUTED)),
        )
        .data(&data)
        .max(100)
        .style(Style::default().fg(theme::ACCENT));
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
        .style(Style::default().fg(theme::TEXT_MUTED)),
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
            Paragraph::new("Swap: none").style(Style::default().fg(theme::TEXT_MUTED)),
            rows[2],
        );
    }

    let data: Vec<u64> = history.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .title("history")
                .border_style(Style::default().fg(theme::ACCENT_MUTED)),
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
                Style::default().fg(theme::DANGER)
            } else {
                Style::default().fg(theme::TEXT_MUTED)
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
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
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
                Style::default().fg(theme::TEXT_MUTED),
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
                theme::DANGER
            } else if b.charge_percent <= 30.0 {
                theme::WARNING
            } else {
                theme::SUCCESS
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
                Some(crit) if temp >= crit * 0.9 => theme::DANGER,
                Some(crit) if temp >= crit * 0.75 => theme::WARNING,
                _ => theme::SUCCESS,
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

/// Row 0-3 = thresholds (editable %), row 4-9 = refresh-mask toggles.
const THRESHOLD_LABELS: &[&str] = &["CPU warn", "Memory warn", "Disk warn", "Battery low"];
const MASK_LABELS: &[&str] = &["CPU", "Memory", "Disks", "Networks", "Temperatures", "GPUs"];
const THRESHOLD_ROWS: usize = 4;
const MASK_ROWS: usize = 6;
const TOTAL_ROWS: usize = THRESHOLD_ROWS + MASK_ROWS;

pub struct ResourceSettingsPanel {
    tab: &'static str,
    api: Arc<ApiClient>,
    tx: Sender<AppEvent>,
    pub focused: bool,
    selected: usize,
    /// Digit-entry buffer while editing the currently-selected threshold row.
    editing: Option<String>,
    thresholds: Thresholds,
    mask: RefreshMask,
    hit_rects: Vec<(Rect, usize)>,
    last_result: Option<(String, bool)>,
}

impl ResourceSettingsPanel {
    pub fn new(tab: &'static str, api: Arc<ApiClient>, tx: Sender<AppEvent>) -> Self {
        Self {
            tab,
            api,
            tx,
            focused: false,
            selected: 0,
            editing: None,
            thresholds: Thresholds::default(),
            mask: RefreshMask::default(),
            hit_rects: Vec::new(),
            last_result: None,
        }
    }

    pub fn height(&self) -> u16 {
        if self.focused {
            1 + THRESHOLD_ROWS as u16 + 1 + 1 + MASK_ROWS as u16 + 1 + 1 + 2
        } else {
            3
        }
    }

    fn threshold_value(&self, row: usize) -> f32 {
        match row {
            0 => self.thresholds.cpu_warn,
            1 => self.thresholds.memory_warn,
            2 => self.thresholds.disk_warn,
            _ => self.thresholds.battery_low,
        }
    }

    fn set_threshold_value(&mut self, row: usize, value: f32) {
        match row {
            0 => self.thresholds.cpu_warn = value,
            1 => self.thresholds.memory_warn = value,
            2 => self.thresholds.disk_warn = value,
            _ => self.thresholds.battery_low = value,
        }
    }

    fn mask_value(&self, idx: usize) -> bool {
        match idx {
            0 => self.mask.cpu,
            1 => self.mask.memory,
            2 => self.mask.disks,
            3 => self.mask.networks,
            4 => self.mask.temperatures,
            _ => self.mask.gpus,
        }
    }

    fn toggle_mask(&mut self, idx: usize) {
        match idx {
            0 => self.mask.cpu = !self.mask.cpu,
            1 => self.mask.memory = !self.mask.memory,
            2 => self.mask.disks = !self.mask.disks,
            3 => self.mask.networks = !self.mask.networks,
            4 => self.mask.temperatures = !self.mask.temperatures,
            _ => self.mask.gpus = !self.mask.gpus,
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> bool {
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                for (rect, idx) in &self.hit_rects {
                    if mouse_hit(mouse, rect) {
                        if !self.focused {
                            self.focused = true;
                            return true;
                        }
                        self.selected = *idx;
                        self.editing = None;
                        self.activate(*idx);
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

        if let Some(buf) = &mut self.editing {
            match key.code {
                KeyCode::Enter => {
                    if let Ok(value) = buf.parse::<f32>() {
                        self.set_threshold_value(self.selected, value.clamp(0.0, 100.0));
                        self.fire_thresholds();
                    }
                    self.editing = None;
                }
                KeyCode::Esc => self.editing = None,
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                    buf.push(c);
                }
                _ => {}
            }
            return true;
        }

        match key.code {
            KeyCode::Left | KeyCode::Esc => {
                self.focused = false;
                true
            }
            KeyCode::Up => {
                self.selected = (self.selected + TOTAL_ROWS - 1) % TOTAL_ROWS;
                true
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1) % TOTAL_ROWS;
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.activate(self.selected);
                true
            }
            _ => true,
        }
    }

    fn activate(&mut self, row: usize) {
        if row < THRESHOLD_ROWS {
            self.editing = Some(format!("{:.0}", self.threshold_value(row)));
        } else {
            self.toggle_mask(row - THRESHOLD_ROWS);
            self.fire_mask();
        }
    }

    fn fire_thresholds(&mut self) {
        let t = self.thresholds.clone();
        let api = self.api.clone();
        let fut = Box::pin(async move {
            api.set_thresholds(t.cpu_warn, t.memory_warn, t.disk_warn, t.battery_low)
                .await
        });
        crate::commands::spawn_command(self.tx.clone(), self.tab, "thresholds", fut, |_| {
            CommandOutcome::Ack
        });
    }

    fn fire_mask(&mut self) {
        let m = self.mask.clone();
        let api = self.api.clone();
        let fut = Box::pin(async move {
            api.set_refresh_mask(m.cpu, m.memory, m.disks, m.networks, m.temperatures, m.gpus)
                .await
        });
        crate::commands::spawn_command(self.tx.clone(), self.tab, "refresh-mask", fut, |_| {
            CommandOutcome::Ack
        });
    }

    /// Call from the tab's `handle_app_event` for `CommandResult`s
    /// labeled "thresholds" or "refresh-mask".
    pub fn apply_result(&mut self, label: &str, result: &Result<CommandOutcome, String>) {
        self.last_result = Some(match result {
            Ok(_) => (format!("{label}: ok"), false),
            Err(e) => (format!("{label} failed: {e}"), true),
        });
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.focused {
            format!("{} Settings", icon::CURSOR)
        } else {
            "Settings (Tab to edit)".to_string()
        };
        let inner = bordered_block_focused(frame, area, &title, self.focused);

        if !self.focused {
            let mask_on = [
                ("CPU", self.mask.cpu),
                ("Mem", self.mask.memory),
                ("Disks", self.mask.disks),
                ("Net", self.mask.networks),
                ("Temp", self.mask.temperatures),
                ("GPU", self.mask.gpus),
            ]
            .iter()
            .filter(|(_, on)| *on)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(",");

            let summary = format!(
                "cpu {:.0}%  mem {:.0}%  disk {:.0}%  batt {:.0}%   ·   refresh: {}",
                self.thresholds.cpu_warn,
                self.thresholds.memory_warn,
                self.thresholds.disk_warn,
                self.thresholds.battery_low,
                if mask_on.is_empty() { "none" } else { &mask_on }
            );
            frame.render_widget(
                Paragraph::new(summary).style(Style::default().fg(theme::TEXT_DIM)),
                inner,
            );
            self.hit_rects = vec![(inner, 0)]; // whole strip focuses the panel
            return;
        }

        let mut hit_rects = Vec::with_capacity(TOTAL_ROWS);
        let mut y = inner.y;
        let bottom = inner.y + inner.height;

        macro_rules! line {
            ($text:expr, $style:expr) => {
                if y < bottom {
                    frame.render_widget(
                        Paragraph::new($text).style($style),
                        Rect {
                            x: inner.x,
                            y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                }
            };
        }

        line!(
            "Thresholds (%)".to_string(),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        );
        y += 1;

        for row in 0..THRESHOLD_ROWS {
            let is_selected = self.selected == row;
            let is_editing = is_selected && self.editing.is_some();
            let marker = if is_selected { icon::CURSOR } else { " " };
            let value_text = match &self.editing {
                Some(buf) if is_selected => format!("{buf}_"),
                _ => format!("{:.0}", self.threshold_value(row)),
            };
            let text = format!("{marker} {:<14} {value_text}%", THRESHOLD_LABELS[row]);
            let style = if is_editing {
                Style::default()
                    .fg(Color::Black)
                    .bg(theme::WARNING)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM)
            };
            if y < bottom {
                let rect = Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                };
                frame.render_widget(Paragraph::new(text).style(style), rect);
                hit_rects.push((rect, row));
            }
            y += 1;
        }

        y += 1;
        line!(
            "Refresh Mask".to_string(),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        );
        y += 1;

        for idx in 0..MASK_ROWS {
            let row = THRESHOLD_ROWS + idx;
            let is_selected = self.selected == row;
            let marker = if is_selected { icon::CURSOR } else { " " };
            let check = if self.mask_value(idx) { "[x]" } else { "[ ]" };
            let text = format!("{marker} {check} {}", MASK_LABELS[idx]);
            let style = if is_selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM)
            };
            if y < bottom {
                let rect = Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                };
                frame.render_widget(Paragraph::new(text).style(style), rect);
                hit_rects.push((rect, row));
            }
            y += 1;
        }

        y += 1;
        if let Some((msg, is_err)) = &self.last_result {
            line!(result_line(msg, *is_err), Style::default());
        }

        self.hit_rects = hit_rects;
    }
}
