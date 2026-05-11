use leptos::prelude::*;

use super::super::server_fns::get_screenshots;

#[component]
pub fn ScreensPane() -> impl IntoView {
    let tick = use_context::<super::super::app::Tick>()
        .expect("Tick context")
        .0;

    let screenshots = Resource::new(move || tick.get(), |_| get_screenshots());

    view! {
        <main id="screens-pane">
            <header>
                <h2>"Monitored Screens"</h2>
                <Transition fallback=|| ()>
                    <ScreenStatus screenshots=screenshots/>
                </Transition>
            </header>
            <div id="screens">
                <Transition fallback=|| ()>
                    {move || screenshots.get().map(|res| match res {
                        Ok(data) => data.screens.into_iter().enumerate().map(|(i, screen)| {
                            let src = format!("data:{};base64,{}", screen.mime, screen.data);
                            let name = if screen.monitor_name.is_empty() {
                                format!("Display {}", i + 1)
                            } else {
                                screen.monitor_name.clone()
                            };
                            let dims = format!("{}×{}", screen.width, screen.height);
                            view! {
                                <div class="screen-container">
                                    <div class="screen-label-row">
                                        <span class="screen-label screen-name">{name}</span>
                                        <span class="screen-meta screen-dims">{dims}</span>
                                        <span class="screen-meta screen-ts">{screen.timestamp}</span>
                                    </div>
                                    <img src=src alt=screen.monitor_name/>
                                </div>
                            }
                        }).collect_view().into_any(),
                        Err(_) => view! {}.into_any(),
                    })}
                </Transition>
            </div>
        </main>
    }
}

#[component]
fn ScreenStatus(
    screenshots: Resource<Result<crate::api::ScreenshotResponse, ServerFnError>>,
) -> impl IntoView {
    move || match screenshots.get() {
        None => view! { <span id="status">"Loading…"</span> }.into_any(),
        Some(Ok(data)) => view! {
            <span id="status">
                {format!("● LIVE · {} SCREEN{}", data.count,
                    if data.count == 1 { "" } else { "S" })}
            </span>
        }
        .into_any(),
        Some(Err(_)) => view! {
            <span id="status" style="color:var(--error)">"● OFFLINE"</span>
        }
        .into_any(),
    }
}

// fn fmt_timestamp(ts: &str) -> String {
//     // On WASM we can use js_sys; on SSR just return as-is.
//     #[cfg(feature = "hydrate")]
//     {
//         use js_sys::Date;
//         let d = Date::new(&wasm_bindgen::JsValue::from_str(ts));
//         if d.get_time().is_nan() {
//             return ts.to_string();
//         }
//         format!(
//             "{:02}:{:02}:{:02}",
//             d.get_hours(),
//             d.get_minutes(),
//             d.get_seconds()
//         )
//     }
//     #[cfg(not(feature = "hydrate"))]
//     ts.to_string()
// }
