use axum::{
    Router, middleware,
    routing::{get, post},
};
use tokio_util::sync::CancellationToken;

use super::{end_points::*, middleware::auth_middleware};
use crate::sse::handlers::*;

fn create_auth_router() -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
}

fn create_common_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/info", get(info))
}

fn create_api_router(cancel_token: CancellationToken, auth_layer: bool) -> Router {
    let api = Router::new()
        .route("/shutdown", post(shutdown))
        // ── Screenshot ────────────────────────────────────────────────────
        .route("/screenshot", get(screenshot))
        // ── Process tracking ──────────────────────────────────────────────
        .route("/root_pids", get(root_pids)) // root pids
        .route("/process/{root_pid}", get(process_tree)) // full tree
        .route("/process/root/{root_pid}", get(process_root)) // root only
        .route("/process/children/{root_pid}", get(process_children)) // children only
        .route("/process/status/{root_pid}", get(process_status)) // lightweight summary
        .route("/process/is-done/{root_pid}", get(is_process_done)) // whether work is done (all children exited)
        .route("/process/trees", get(process_trees)) // all process trees
        .route("/top-processes", get(top_processes)) // top processes
        .route("/supported-signals", get(supported_signals)) // supported signals
        // ── System Resources ──────────────────────────────────────────────
        .route("/system", get(system_snapshot)) // full system snapshot
        .route("/cpu", get(cpu_snapshot)) // cpu snapshot
        .route("/memory", get(memory_snapshot)) // memory snapshot
        .route("/disks", get(disks_snapshots)) // disks snapshot
        .route("/networks", get(networks_snapshot)) // networks snapshot
        .route("/gpus", get(gpus_snapshots)) // gpus snapshot
        .route("/battery", get(battery_snapshot)) // battery snapshot
        .route("/host-info", get(host_info_snapshot)) // host info snapshot
        .route("/temperatures", get(temperatures_snapshots)) // temperatures snapshot
        // ── Systemd ───────────────────────────────────────────────────────
        .route("/systemd", get(systemd_snapshot)) // systemd snapshot
        .route("/unit/{unit_name}", get(unit_snapshot)) // unit snapshot
        .route("/units/{unit_state}", get(units_by_active_state)) // units by active state
        .route("/failed_units", get(failed_units)) // failed_units
        // ── Docker Containers ───────────────────────────────────────────────────────
        .route("/docker-containers", get(list_docker_containers)) // docker containers
        .route("/container/{id_or_name}", get(get_docker_container)) // container by name or id
        .route("/top-containers", get(top_docker_containers)) // top containers
        // ── SSE ───────────────────────────────────────────────────────
        .route("/sse", get(sse_stream))
        .route("/sse/process-tracker", get(sse_stream_process))
        .route("/sse/system-resources", get(sse_stream_system_resources))
        .route("/sse/systemd", get(sse_stream_systemd))
        .route("/sse/docker-tracker", get(sse_stream_docker))
        .with_state(cancel_token);
    if auth_layer {
        api.layer(middleware::from_fn(auth_middleware))
    } else {
        api
    }
}

fn create_process_commands_router() -> Router {
    Router::new()
        .route("/process/kill/{pid}", post(kill_process))
        .route("/process/kill-tree/{root_pid}", post(kill_tree))
        .route("/process/track/{pid}", post(track_pid))
        .route("/process/untrack/{pid}", post(untrack_pid))
        .route("/process/poll/pause", post(process_tracker_pause_poll))
        .route("/process/poll/resume", post(process_tracker_resume_poll))
        .route(
            "/process/poll/interval",
            post(process_tracker_set_poll_interval),
        )
        .layer(middleware::from_fn(auth_middleware))
}

#[cfg(feature = "screenshot")]
fn create_screen_commands_router() -> Router {
    Router::new()
        .route("/screen/poll/pause", post(screen_capture_pause_poll))
        .route("/screen/poll/resume", post(screen_capture_resume_poll))
        .route(
            "/screen/poll/interval",
            post(screen_capture_set_poll_interval),
        )
        .layer(middleware::from_fn(auth_middleware))
}

fn create_sr_commands_router() -> Router {
    Router::new()
        .route("/resources/thresholds", post(resources_set_thresholds))
        .route("/resources/refresh-mask", post(resources_set_refresh_mask))
        .route("/resources/poll/pause", post(resources_pause_poll))
        .route("/resources/poll/resume", post(resources_resume_poll))
        .route(
            "/resources/poll/interval",
            post(resources_set_poll_interval),
        )
        .layer(middleware::from_fn(auth_middleware))
}

fn create_systemd_commands_router() -> Router {
    Router::new()
        .route("/systemd/poll/pause", post(systemd_pause_poll))
        .route("/systemd/poll/resume", post(systemd_resume_poll))
        .route("/systemd/poll/interval", post(systemd_set_poll_interval))
        .layer(middleware::from_fn(auth_middleware))
}

fn create_docker_commands_router() -> Router {
    Router::new()
        .route("/docker/stop-container", post(stop_container))
        .route("/docker/kill-container", post(kill_container))
        .route("/docker/start-container", post(start_container))
        .route("/docker/restart-container", post(restart_container))
        .route("/docker/pause-container", post(pause_container))
        .route("/docker/unpause-container", post(unpause_container))
        .route("/docker/poll/pause", post(docker_pause_poll))
        .route("/docker/poll/resume", post(docker_resume_poll))
        .route("/docker/poll/interval", post(docker_set_poll_interval))
        .layer(middleware::from_fn(auth_middleware))
}

fn should_enable_auth(config: &crate::config::AppConfig) -> bool {
    config.args.enable_auth
        || config.args.allow_process_commands
        || {
            #[cfg(feature = "screenshot")]
            let screen_check = !config.args.blind && config.args.allow_screen_commands;
            #[cfg(not(feature = "screenshot"))]
            let screen_check = false;
            screen_check
        }
        || (config.args.system_resources && config.args.allow_system_resources_commands)
        || (config.args.systemd && config.args.allow_systemd_commands)
        || (config.args.docker && config.args.allow_docker_commands)
}

pub fn create_routers(
    config: &crate::config::AppConfig,
    cancel_token: CancellationToken,
) -> Router {
    let api_router = create_api_router(cancel_token, config.args.enable_auth);
    let mut app = Router::new()
        .nest("/api", api_router)
        .nest("/api", create_common_router());
    if should_enable_auth(config) {
        super::session::init_sessions();
        app = app.nest("/api/auth", create_auth_router());
    }
    if config.args.allow_process_commands {
        app = app.nest("/api", create_process_commands_router());
    }
    #[cfg(feature = "screenshot")]
    if !config.args.blind && config.args.allow_screen_commands {
        app = app.nest("/api", create_screen_commands_router());
    }
    if config.args.allow_system_resources_commands {
        app = app.nest("/api", create_sr_commands_router());
    }
    if config.args.allow_systemd_commands {
        app = app.nest("/api", create_systemd_commands_router())
    }
    if config.args.allow_docker_commands {
        app = app.nest("/api", create_docker_commands_router())
    }
    app
}
