use leptos::prelude::*;

use super::super::server_fns::get_system_snapshot;
use crate::utils::{format_bytes, format_uptime};

#[component]
pub fn SystemPanel() -> impl IntoView {
    let tick = use_context::<super::super::app::Tick>()
        .expect("Tick context")
        .0;

    let snap = Resource::new(move || tick.get(), |_| get_system_snapshot());

    view! {
        <div id="system-panel">
            <div id="system-panel-inner">
                <Transition fallback=|| ()>
                    {move || snap.get().map(|res| match res {
                        Ok(Some(s)) => view! { <SystemSections snap=s/> }.into_any(),
                        _ => view! {}.into_any(),
                    })}
                </Transition>
            </div>
        </div>
    }
}

#[component]
fn SystemSections(snap: crate::system_monitor::SystemSnapshot) -> impl IntoView {
    use crate::system_monitor::SystemHealth;
    let health_cls = match snap.health {
        SystemHealth::Healthy => "sv health-healthy",
        SystemHealth::Warning => "sv health-warning",
        SystemHealth::Critical => "sv health-critical",
    };

    // ── Host ────────────────────────────────────────────────
    let h = &snap.host;
    let host_kvs = vec![
        ("Hostname", h.hostname.clone().unwrap_or_default()),
        ("OS", h.os_name.clone().unwrap_or_default()),
        ("Kernel", h.kernel_version.clone().unwrap_or_default()),
        ("Arch", h.cpu_arch.clone().unwrap_or_default()),
        ("Uptime", format_uptime(h.uptime_secs)),
        ("Procs", h.process_count.to_string()),
    ];
    let health_str = snap.health.to_string();

    // ── CPU ─────────────────────────────────────────────────
    let cpu = &snap.cpu;
    let cores = cpu.cores.clone();
    let cpu_usage = cpu.usage_percent;
    let cpu_brand = cpu.brand.clone();
    let cpu_cores_n = cpu.physical_core_count.unwrap_or(cores.len());
    let cpu_freq = cpu.frequency_mhz;
    #[cfg(target_os = "linux")]
    let load = view! {
        <Kv label="Load 1m"  value=format!("{:.2}", cpu.load_avg.one)/>
        <Kv label="Load 5m"  value=format!("{:.2}", cpu.load_avg.five)/>
    };
    #[cfg(not(target_os = "linux"))]
    let load = view! { <></> };

    // ── Memory ──────────────────────────────────────────────
    let mem = snap.memory.clone();

    // ── Disks ───────────────────────────────────────────────
    let disks = snap.disks.clone();

    // ── Network ─────────────────────────────────────────────
    let nets: Vec<_> = snap
        .networks
        .iter()
        .filter(|n| n.rx_total_bytes > 0 || n.tx_total_bytes > 0)
        .cloned()
        .collect();

    // ── GPU ─────────────────────────────────────────────────
    let gpus = snap.gpus.clone();
    let has_gpus = !gpus.is_empty();

    // ── Battery ─────────────────────────────────────────────
    let battery = StoredValue::new(snap.battery.clone());
    let has_battery = snap.battery.is_some();

    // ── Thermals ────────────────────────────────────────────
    let thermals: Vec<_> = snap
        .temperatures
        .iter()
        .filter(|t| t.temperature_celsius.is_some())
        .cloned()
        .collect();
    let has_thermals = !thermals.is_empty();
    let thermals = StoredValue::new(thermals);

    view! {
        <div id="system-panel">
            <div id="system-panel-inner">
                // ── Host ──────────────────────────────────────────────
                <div class="sys-section">
                    <div class="sys-section-title">"Host"</div>
                    <div class="sys-grid">
                        {host_kvs.into_iter().map(|(k, v)| view! { <Kv label=k value=v/> }).collect_view()}
                        <div class="sys-kv">
                            <span class="sk">"Health"</span>
                            <span class=health_cls>{health_str}</span>
                        </div>
                    </div>
                </div>

                // ── CPU ───────────────────────────────────────────────
                <div class="sys-section">
                    <div class="sys-section-title">"CPU"</div>
                    <div class="sys-grid">
                        <Kv label="Brand" value=cpu_brand/>
                        <Kv label="Cores" value=cpu_cores_n.to_string()/>
                        <Kv label="Freq"  value=format!("{cpu_freq} MHz")/>
                        <Kv label="Usage" value=format!("{cpu_usage:.1}%")/>
                        {load}
                    </div>
                    // Core bars
                    <div class="sys-cores-row">
                        {cores.iter().map(|c| {
                            let h = (c.usage_percent / 100.0 * 28.0).max(2.0);
                            let col = if c.usage_percent >= 90.0 { "var(--error)" }
                                    else if c.usage_percent >= 75.0 { "var(--warning)" }
                                    else { "var(--accent)" };
                            let title = format!("{}: {:.1}%", c.name, c.usage_percent);
                            view! {
                                <div
                                    class="sys-core-bar"
                                    title=title
                                    style=format!("height:{h}px;background:{col}")
                                />
                            }
                        }).collect_view()}
                    </div>
                </div>

                // ── Memory ────────────────────────────────────────────
                <div class="sys-section">
                    <div class="sys-section-title">"Memory"</div>
                    <div class="sys-grid">
                        <div style="grid-column:1/-1" class="sys-bar-wrap">
                            <UsageBar label="RAM"  pct=mem.used_percent/>
                            {mem.swap_used_percent.map(|p| view! { <UsageBar label="SWAP" pct=p/> })}
                        </div>
                        <Kv label="Total" value=format_bytes(mem.total_bytes)/>
                        <Kv label="Used"  value=format_bytes(mem.used_bytes)/>
                        <Kv label="Free"  value=format_bytes(mem.free_bytes)/>
                        <Kv label="Avail" value=format_bytes(mem.available_bytes)/>
                        {(mem.swap_total_bytes > 0).then(|| view! {
                            <Kv label="Swap Total" value=format_bytes(mem.swap_total_bytes)/>
                        })}
                        {(mem.swap_used_bytes > 0).then(|| view! {
                            <Kv label="Swap Used" value=format_bytes(mem.swap_used_bytes)/>
                        })}
                    </div>
                </div>

                // ── Disks ─────────────────────────────────────────────
                <div class="sys-section">
                    <div class="sys-section-title">"Disks"</div>
                    <div id="sys-disk-list">
                        {disks.into_iter().map(|d| {
                            let fill_cls = if d.used_percent >= 95.0 { "sys-bar-fill crit" }
                                        else if d.used_percent >= 80.0 { "sys-bar-fill warn" }
                                        else { "sys-bar-fill" };
                            let pct = d.used_percent.min(100.0);
                            let kind = d.kind.to_string();
                            let removable = if d.is_removable { " · removable" } else { "" };
                            view! {
                                <div class="sys-item">
                                    <span class="sys-item-name" title=d.name.clone()>{d.mount_point.clone()}</span>
                                    <span class="sys-item-sub">
                                        {format!("{} · {}{}", d.file_system, kind, removable)}
                                    </span>
                                    <div class="sys-bar-track" style="min-width:140px">
                                        <div class=fill_cls style=format!("width:{pct:.1}%")/>
                                    </div>
                                    <div style="display:flex;gap:0.75rem">
                                        <Kv label="Used"  value=format_bytes(d.used_bytes)/>
                                        <Kv label="Free"  value=format_bytes(d.available_bytes)/>
                                        <Kv label="Total" value=format_bytes(d.total_bytes)/>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </div>

                // ── Network ───────────────────────────────────────────
                <div class="sys-section">
                    <div class="sys-section-title">"Network"</div>
                    <div id="sys-net-list">
                        {if nets.is_empty() {
                            view! { <span class="sys-item-sub">"No active interfaces"</span> }.into_any()
                        } else {
                            nets.into_iter().map(|n| view! {
                                <div class="sys-item">
                                    <span class="sys-item-name">{n.interface.clone()}</span>
                                    <div class="sys-net-io">
                                        <div class="sys-net-badge">
                                            <span class="dir">"↓"</span>
                                            <span class="bw">{format_bytes(n.rx_bytes_per_sec)}"/s"</span>
                                        </div>
                                        <div class="sys-net-badge">
                                            <span class="dir">"↑"</span>
                                            <span class="bw">{format_bytes(n.tx_bytes_per_sec)}"/s"</span>
                                        </div>
                                    </div>
                                    <div style="display:flex;gap:0.75rem">
                                        <Kv label="RX Total" value=format_bytes(n.rx_total_bytes)/>
                                        <Kv label="TX Total" value=format_bytes(n.tx_total_bytes)/>
                                    </div>
                                </div>
                            }).collect_view().into_any()
                        }}
                    </div>
                </div>

                // ── GPU ───────────────────────────────────────────────
                <Show when=move || has_gpus>
                    <div class="sys-section">
                        <div class="sys-section-title">"GPU"</div>
                        <div id="sys-gpu-list">
                            {gpus.iter().map(|g| {
                                let fill_cls = g.usage_percent.map(|p| {
                                    if p >= 90.0 { "sys-bar-fill crit" }
                                    else if p >= 75.0 { "sys-bar-fill warn" }
                                    else { "sys-bar-fill" }
                                });
                                view! {
                                    <div class="sys-item">
                                        <span class="sys-item-name">{g.name.clone()}</span>
                                        {g.usage_percent.map(|p| view! {
                                            <div class="sys-bar-track" style="min-width:120px">
                                                <div class=fill_cls.unwrap_or("sys-bar-fill")
                                                    style=format!("width:{:.1}%", p.min(100.0))/>
                                            </div>
                                        })}
                                        <div style="display:flex;gap:0.75rem;flex-wrap:wrap">
                                            {g.usage_percent.map(|p| view! {
                                                <Kv label="Core" value=format!("{p:.1}%")/>
                                            })}
                                            {g.vram_used_bytes.zip(g.vram_total_bytes).map(|(u, t)| view! {
                                                <Kv label="VRAM" value=format!("{} / {}", format_bytes(u), format_bytes(t))/>
                                            })}
                                            {g.temperature_celsius.map(|t| view! {
                                                <Kv label="Temp" value=format!("{t:.0}°C")/>
                                            })}
                                            {g.power_draw_watts.map(|p| view! {
                                                <Kv label="Power" value=format!("{p:.0}W")/>
                                            })}
                                            {(!g.fan_speed_percent.is_empty()).then(|| {
                                                let label = if g.fan_speed_percent.len() > 1 { "Fans" } else { "Fan" };
                                                let val = g.fan_speed_percent.iter()
                                                    .map(|f| format!("{f:.0}%"))
                                                    .collect::<Vec<_>>()
                                                    .join(", ");
                                                view! { <Kv label=label value=val/> }
                                            })}
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                </Show>

                // ── Battery ───────────────────────────────────────────────
                <Show when=move || has_battery>
                    {battery.with_value(|bat| bat.clone().map(|bat| view! {
                        <div class="sys-section">
                            <div class="sys-section-title">"Battery"</div>
                            <div class="sys-grid">
                                <div style="grid-column:1/-1" class="sys-bar-wrap">
                                    <UsageBar label="Charge" pct=bat.charge_percent/>
                                </div>
                                <Kv label="State" value=bat.state.to_string()/>
                                {bat.time_to_empty_secs.map(|s| view! {
                                    <Kv label="Empty in" value=format_uptime(s)/>
                                })}
                                {bat.time_to_full_secs.map(|s| view! {
                                    <Kv label="Full in" value=format_uptime(s)/>
                                })}
                                {bat.power_draw_watts.map(|w| view! {
                                    <Kv label="Draw" value=format!("{w:.1}W")/>
                                })}
                                {bat.health_percent.map(|h| view! {
                                    <Kv label="Health" value=format!("{h:.0}%")/>
                                })}
                                {bat.cycle_count.map(|c| view! {
                                    <Kv label="Cycles" value=c.to_string()/>
                                })}
                            </div>
                        </div>
                    }))}
                </Show>

                // ── Thermals ──────────────────────────────────────────────
                <Show when=move || has_thermals>
                    <div class="sys-section">
                        <div class="sys-section-title">"Thermals"</div>
                        <div class="sys-thermal-chips">
                            {thermals.with_value(|ts| ts.iter().map(|t| {
                                let temp = t.temperature_celsius.unwrap();
                                let crit = t.temperature_critical_celsius;
                                let is_crit = crit.map_or(false, |c| temp >= c);
                                let is_warn = !is_crit && temp >= 80.0;
                                let val_cls = if is_crit { "sys-thermal-val crit" }
                                            else if is_warn { "sys-thermal-val warn" }
                                            else { "sys-thermal-val" };
                                view! {
                                    <div class="sys-thermal-chip">
                                        <span class="sys-thermal-label" title=t.label.clone()>
                                            {t.label.clone()}
                                        </span>
                                        <span class=val_cls>{format!("{temp:.0}°C")}</span>
                                        {crit.map(|c| view! {
                                            <span class="sys-thermal-label" style="min-width:0">
                                                {format!("/ {c:.0}°C")}
                                            </span>
                                        })}
                                    </div>
                                }
                            }).collect_view())}
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[component]
fn Kv(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="sys-kv">
            <span class="sk">{label}</span>
            <span class="sv">{value}</span>
        </div>
    }
}

#[component]
fn UsageBar(label: &'static str, pct: f32) -> impl IntoView {
    let fill = pct.min(100.0);
    let color_cls = if fill >= 90.0 {
        "sys-bar-fill crit"
    } else if fill >= 75.0 {
        "sys-bar-fill warn"
    } else {
        "sys-bar-fill"
    };
    view! {
        <div class="sys-bar-row">
            <span class="sys-bar-label">{label}</span>
            <div class="sys-bar-track">
                <div class=color_cls style=format!("width:{fill:.1}%")/>
            </div>
            <span class="sys-bar-val">{format!("{fill:.1}%")}</span>
        </div>
    }
}
