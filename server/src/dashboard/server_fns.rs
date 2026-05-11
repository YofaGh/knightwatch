use leptos::prelude::*;

use crate::{
    api::{ConfigResponse, ScreenshotResponse},
    process_tracker::ProcessTree,
};

#[server(GetConfig, "/api/leptos")]
pub async fn get_config() -> Result<ConfigResponse, ServerFnError> {
    let args = &crate::prelude::get_config().args;
    Ok(ConfigResponse {
        blind: args.blind,
        pid: args.pid.clone(),
        top_processes: args.top_processes,
        limit_processes: args.limit_processes,
        telegram_bot: args.telegram,
        system_monitor: args.system_monitor,
    })
}

#[server(GetScreenshots, "/api/leptos")]
pub async fn get_screenshots() -> Result<ScreenshotResponse, ServerFnError> {
    let images = crate::screen_capture::get_screenshots().await;
    if images.is_empty() {
        return Err(ServerFnError::new("No screens found"));
    }
    use base64::{Engine as _, engine::general_purpose};
    let screens = images
        .into_iter()
        .map(|s| crate::api::ScreenshotImage {
            data: general_purpose::STANDARD.encode(&s.image),
            mime: "image/png".to_string(),
            monitor_name: s.monitor_name,
            monitor_id: s.monitor_id,
            width: s.width,
            height: s.height,
            timestamp: s.timestamp,
        })
        .collect::<Vec<_>>();
    let count = screens.len();
    Ok(ScreenshotResponse { screens, count })
}

#[server(GetRootPids, "/api/leptos")]
pub async fn get_root_pids() -> Result<Vec<u32>, ServerFnError> {
    Ok(crate::process_tracker::get_root_pids().await)
}

#[server(GetProcessTree, "/api/leptos")]
pub async fn get_process_tree(root_pid: u32) -> Result<ProcessTree, ServerFnError> {
    use crate::{process_tracker, utils::now_rfc3339};
    let (root, children, work_done) = tokio::join!(
        process_tracker::get_root(root_pid),
        process_tracker::get_children(root_pid),
        process_tracker::is_work_done(root_pid),
    );
    let child_count = children.len();
    Ok(ProcessTree {
        root,
        children,
        child_count,
        work_done,
        timestamp: now_rfc3339(),
    })
}

#[server(GetTopProcesses, "/api/leptos")]
pub async fn get_top_processes(
    sort: String,
    limit: usize,
) -> Result<Vec<crate::process_tracker::ProcessSnapshot>, ServerFnError> {
    use crate::process_tracker::SortKey;
    let sort_key = SortKey::try_from(sort).map_err(ServerFnError::new)?;
    Ok(crate::process_tracker::get_top_processes(sort_key, limit).await)
}

#[server(GetSystemSnapshot, "/api/leptos")]
pub async fn get_system_snapshot()
-> Result<Option<crate::system_monitor::SystemSnapshot>, ServerFnError> {
    Ok(crate::system_monitor::get_snapshot().await)
}

#[server(Shutdown, "/api/leptos")]
pub async fn shutdown() -> Result<(), ServerFnError> {
    let state = expect_context::<crate::api::AppState>();
    state.cancel_token.cancel();
    Ok(())
}
