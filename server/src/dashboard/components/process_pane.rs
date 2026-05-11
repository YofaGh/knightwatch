use leptos::prelude::*;
use leptos::web_sys;

use super::super::server_fns::Shutdown;
use crate::{process_tracker::ProcessSnapshot, utils::format_bytes};

#[component]
pub fn ProcessPane(
    has_pids: bool,
    top_processes: bool,
    limit_processes: usize,
    telegram_bot: bool,
) -> impl IntoView {
    let tick = use_context::<super::super::app::Tick>()
        .expect("Tick context")
        .0;
    let shutdown_action = ServerAction::<Shutdown>::new();
    let shutting_down = shutdown_action.pending();

    view! {
        <aside id="process-pane">
            <div class="brand">
                <h1>"Knight Watch"</h1>
                <span
                    class=move || if telegram_bot { "telegram-indicator tg-on" } else { "telegram-indicator tg-off" }
                >
                    "TG Bot"
                </span>
                <button
                    id="shutdown-btn"
                    title="Shut down the server"
                    disabled=shutting_down
                    on:click=move |_| {
                        if web_sys::window()
                            .and_then(|w| w.confirm_with_message("Shut down the server?").ok())
                            .unwrap_or(false)
                        {
                            shutdown_action.dispatch(Shutdown {});
                        }
                    }
                >
                    {move || if shutting_down.get() { "Shutting down…" } else { "● Shutdown" }}
                </button>
            </div>
            <div id="process-content">
                <Show when=move || has_pids>
                    <ProcessTracker tick/>
                </Show>
                <Show when=move || top_processes>
                    <TopProcesses tick limit_processes />
                </Show>
            </div>
        </aside>
    }
}

#[component]
fn ProcessTracker(tick: ReadSignal<u64>) -> impl IntoView {
    let root_pids = Resource::new(
        move || tick.get(),
        |_| super::super::server_fns::get_root_pids(),
    );

    view! {
        <div id="root-section">
            <Transition fallback=|| view! { <div class="muted">"Loading…"</div> }>
                {move || root_pids.get().map(|res| match res {
                    Err(_) => view! { <div class="muted">"Monitor disabled"</div> }.into_any(),
                    Ok(pids) if pids.is_empty() => {
                        view! { <div class="muted">"No process tracker running."</div> }.into_any()
                    }
                    Ok(pids) => view! {
                        <ProcessGroups pids tick/>
                    }.into_any(),
                })}
            </Transition>
        </div>
    }
}

#[component]
fn ProcessGroups(pids: Vec<u32>, tick: ReadSignal<u64>) -> impl IntoView {
    pids.into_iter().map(|pid| {
        let tree = Resource::new(
            move || tick.get(),
            move |_| super::super::server_fns::get_process_tree(pid),
        );
        view! {
            <Transition fallback=|| ()>
                {move || tree.get().map(|res| match res {
                    Ok(data) => view! { <ProcessGroup data/> }.into_any(),
                    Err(_) => view! { <div class="muted">"Failed to load process"</div> }.into_any(),
                })}
            </Transition>
        }
    }).collect_view()
}

#[component]
fn ProcessGroup(data: crate::process_tracker::ProcessTree) -> impl IntoView {
    let work_done = data.work_done;
    let child_count = data.child_count;
    let showc = child_count > 0;
    let children = StoredValue::new(data.children);

    view! {
        <div class="process-group" style="display:flex;flex-direction:column;gap:1rem">
            <Show when=move || work_done>
                <div id="work-banner" class="visible">
                    "✔ Work complete — all children exited"
                </div>
            </Show>

            {match data.root {
                Some(proc) => view! { <ProcCard proc is_root=true/> }.into_any(),
                None => view! { <div class="muted">"Root process exited"</div> }.into_any(),
            }}

            <Show when=move || showc>
                <details style="margin-top:0.5rem;margin-left:0.75rem">
                    <summary class="section-header" style="margin-top:0;cursor:pointer;user-select:none">
                        "Children "
                        <span class="count-badge">{child_count}</span>
                        <span class="muted" style="margin-left:auto;font-size:0.7rem;font-weight:normal">
                            "(click to toggle)"
                        </span>
                    </summary>
                    <div style="border-left:2px solid var(--border);padding-left:0.75rem;margin-top:0.5rem;display:flex;flex-direction:column;gap:0.5rem">
                        {children.with_value(|c| {
                            c.iter().map(|proc| view! { <ProcCard proc=proc.clone() is_root=false/> }).collect_view()
                        })}
                    </div>
                </details>
            </Show>
        </div>
    }
}

#[component]
fn TopProcesses(tick: ReadSignal<u64>, limit_processes: usize) -> impl IntoView {
    let (sort, set_sort) = signal("cpu".to_string());
    let (limit, set_limit) = signal(5usize);

    let top = Resource::new(
        move || (tick.get(), sort.get(), limit.get()),
        |(_, sort, limit)| super::super::server_fns::get_top_processes(sort, limit),
    );

    view! {
        <div id="top-processes-section">
            <div class="section-header">
                "Top Processes"
                <div class="top-controls">
                    <select
                        id="top-sort-select"
                        class="control-input"
                        on:change=move |e| set_sort.set(event_target_value(&e))
                    >
                        <option value="cpu">"CPU"</option>
                        <option value="mem">"MEM"</option>
                    </select>
                    <input
                        type="number"
                        id="top-limit-input"
                        class="control-input"
                        style="width:3rem"
                        min="1"
                        max=limit_processes.to_string()
                        value="5"
                        on:change=move |e| {
                            if let Ok(v) = event_target_value(&e).parse::<usize>() {
                                set_limit.set(v.min(limit_processes).max(1));
                            }
                        }
                    />
                </div>
            </div>
            <div id="top-processes-list">
                <Transition fallback=|| ()>
                    {move || top.get().map(|res| match res {
                        Ok(procs) if !procs.is_empty() => procs.into_iter()
                            .map(|proc| view! { <ProcCard proc is_root=false/> })
                            .collect_view()
                            .into_any(),
                        Ok(_) => view! { <div class="muted">"No processes found"</div> }.into_any(),
                        Err(_) => view! { <div class="muted">"Failed to load top processes"</div> }.into_any(),
                    })}
                </Transition>
            </div>
        </div>
    }
}

// ── Process card ─────────────────────────────────────────────

#[component]
fn ProcCard(proc: ProcessSnapshot, is_root: bool) -> impl IntoView {
    let state_str = proc.state.to_string();
    let state_cls = match state_str.as_str() {
        "running" => "state-pill state-running",
        "sleeping" => "state-pill state-sleeping",
        "gone" => "state-pill state-gone",
        _ => "state-pill state-other",
    };
    let card_cls = if is_root {
        "proc-card root-card"
    } else {
        "proc-card"
    };
    let name_prefix = if is_root { "⬢ " } else { "" };
    let mem = format_bytes(proc.memory_bytes);
    let cpu = format!("{:.1}%", proc.cpu_usage);
    let pid = proc.pid.to_string();
    let name = proc.name.clone();
    let namec = name.clone();
    #[cfg(target_os = "linux")]
    let linux_extras = view! { <LinuxExtras proc=proc.clone()/> };
    #[cfg(not(target_os = "linux"))]
    let linux_extras = view! { <></> };

    view! {
        <div class=card_cls>
            <div class="proc-header">
                <div class="proc-name" title=format!("{} (PID {})", namec, pid)>
                    {name_prefix}{name}
                </div>
                <span class=state_cls>{state_str}</span>
            </div>
            <div class="proc-meta">
                <MetaItem label="PID" value=pid/>
                <MetaItem label="CPU" value=cpu/>
                <MetaItem label="MEM" value=mem/>
            </div>
            // Linux-only extras — compiled out on other platforms.
            {linux_extras}
        </div>
    }
}

#[component]
fn MetaItem(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="proc-meta-item">
            <span class="label">{label}</span>
            <span class="value">{value}</span>
        </div>
    }
}

#[cfg(target_os = "linux")]
#[component]
fn LinuxExtras(proc: ProcessSnapshot) -> impl IntoView {
    let cmdline = proc.cmdline.join(" ");
    let has_cmdline = !cmdline.is_empty();
    let cwd = proc.cwd.clone();
    let io = proc.io_stats;
    let fds = proc.open_files.clone();
    let fd_count = fds.len();
    let has_linux = cwd.is_some() || !fds.is_empty() || io.is_some();

    view! {
        <Show when=move || has_cmdline>
            <div class="proc-cmdline" title=cmdline.clone()>{cmdline.clone()}</div>
        </Show>
        <Show when=move || has_linux>
            <div class="proc-meta proc-meta-linux">
                {cwd.as_ref().map(|c| view! { <MetaItem label="CWD" value=c.clone()/> })}
                {if !fds.is_empty() { Some(view! { <MetaItem label="FDs" value=fd_count.to_string()/> }) } else { None }}
                {io.map(|s| view! {
                    <MetaItem label="READ" value=format_bytes(s.read_bytes)/>
                    <MetaItem label="WRITE" value=format_bytes(s.write_bytes)/>
                })}
            </div>
        </Show>
        <Show when=move || fd_count > 0>
            <div class="fd-section">
                <div class="fd-section-header">
                    <span>"Open File Descriptors"</span>
                    <span class="count-badge">{fd_count}</span>
                </div>
                <div class="fd-list">
                    {fds.iter().map(|f| {
                        let color = match f.fd_type {
                            crate::api::models::FDType::File   => "#a78bfa",
                            crate::api::models::FDType::Socket => "#34d399",
                            crate::api::models::FDType::Pipe   => "#fbbf24",
                            crate::api::models::FDType::Other  => "#a1a1aa",
                        };
                        let fd_type = f.fd_type.to_string();
                        let target = f.target.clone();
                        view! {
                            <div class="fd-row">
                                <span class="fd-num">{f.fd}</span>
                                <span class="fd-type" style=format!("color:{color}")>{fd_type}</span>
                                <span class="fd-target" title=target.clone()>{target}</span>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </div>
        </Show>
    }
}
