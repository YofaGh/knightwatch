use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::api::ConfigResponse;

/// Tick signal: bumped every 2 s on the client to drive reactive polling.
/// We store it in context so child components can share the same ticker.
#[derive(Clone, Copy)]
pub struct Tick(pub ReadSignal<u64>);

#[cfg(feature = "ssr")]
pub fn shell(options: leptos_config::LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="stylesheet" href="/view.css"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <leptos_meta::MetaTags/>
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Tick counter — incremented by set_interval on the client.
    let (tick, set_tick) = signal(0u64);
    #[cfg(feature = "hydrate")]
    {
        use std::time::Duration;
        leptos::leptos_dom::helpers::set_interval(
            move || set_tick.update(|n| *n += 1),
            Duration::from_millis(2000),
        );
    }
    provide_context(Tick(tick));

    // Config loaded once — no need to poll.
    let config = Resource::new(|| (), |_| super::server_fns::get_config());

    view! {
        <Title text="KnightWatch Dashboard"/>
        <Router>
            <Routes fallback=|| view! { <p>"Not found."</p> }>
                <Route path=path!("/dashboard") view=move || {
                    view! { <Dashboard config/> }
                }/>
            </Routes>
        </Router>
    }
}

#[component]
fn Dashboard(config: Resource<Result<ConfigResponse, ServerFnError>>) -> impl IntoView {
    let (blind, set_blind) = signal(false);
    let (system_monitor, set_system_monitor) = signal(false);
    let (top_processes, set_top_processes) = signal(false);
    let (telegram_bot, set_telegram_bot) = signal(false);
    let (has_pids, set_has_pids) = signal(false);
    let (limit_processes, set_limit_processes) = signal(5);

    view! {
        <div class="app-layout">
            <div id="screens-pane">
                <Suspense fallback=|| view! { <div class="app-layout"/> }>
                    {move || config.get().map(|res| {
                        let cfg = res.unwrap_or_else(|_| ConfigResponse {
                            blind: false,
                            pid: vec![],
                            top_processes: false,
                            limit_processes: 5,
                            telegram_bot: false,
                            system_monitor: false,
                        });
                        set_blind.set(cfg.blind);
                        set_has_pids.set(!cfg.pid.is_empty());
                        set_top_processes.set(cfg.top_processes);
                        set_telegram_bot.set(cfg.telegram_bot);
                        set_limit_processes.set(cfg.limit_processes);
                        set_system_monitor.set(cfg.system_monitor);
                    })}
                </Suspense>
                <Show when=move || !blind.get()>
                    <super::components::ScreensPane />
                </Show>
                <Show when=move || system_monitor.get()>
                    <super::components::SystemPanel />
                </Show>
            </div>
            <Show when=move || !blind.get()>
                <super::components::ProcessPane
                    has_pids=has_pids.get()
                    top_processes=top_processes.get()
                    limit_processes=limit_processes.get()
                    telegram_bot=telegram_bot.get()
                />
            </Show>
        </div>
    }
}
