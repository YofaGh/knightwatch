use axum::{
    Router, middleware,
    routing::{get, post},
};
use tokio_util::sync::CancellationToken;

use super::{
    end_points::{
        common::{health, history, info, login, logout, shutdown},
        docker::{
            docker_pause_poll, docker_resume_poll, docker_set_poll_interval,
            docker_tracker_poll_status, get_docker_container, kill_container,
            list_docker_containers, pause_container, restart_container, start_container,
            stop_container, top_docker_containers, unpause_container,
        },
        process::{
            is_process_done, kill_process, kill_tree, process_children, process_root,
            process_status, process_tracker_pause_poll, process_tracker_poll_status,
            process_tracker_resume_poll, process_tracker_set_poll_interval, process_tree,
            process_trees, root_pids, supported_signals, top_processes, track_pid, untrack_pid,
        },
        screen::{
            screen_capture_pause_poll, screen_capture_poll_status, screen_capture_resume_poll,
            screen_capture_set_poll_interval, screenshot,
        },
        system_resources::{
            alarms_snapshot, battery_snapshot, cpu_snapshot, disks_snapshots, gpus_snapshots,
            host_info_snapshot, memory_snapshot, networks_snapshot, refresh_mask,
            resources_pause_poll, resources_resume_poll, resources_set_poll_interval,
            resources_set_refresh_mask, resources_set_thresholds, system_resources_poll_status,
            system_snapshot, temperatures_snapshots, thresholds,
        },
        systemd::{
            control_unit, failed_units, systemd_pause_poll, systemd_poll_status,
            systemd_resume_poll, systemd_set_poll_interval, systemd_snapshot, unit_snapshot,
            units_by_active_state,
        },
    },
    middleware::auth_middleware,
};

use crate::sse::handlers::{
    sse_stream, sse_stream_docker, sse_stream_process, sse_stream_screen,
    sse_stream_system_resources, sse_stream_systemd,
};

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

fn create_api_router(
    cancel_token: CancellationToken,
    enable_shutdown: bool,
    auth_layer: bool,
) -> Router {
    let mut api = Router::new()
        // ────────────────────────────────────────────────────
        .route("/history", get(history))
        // ── Screenshot ────────────────────────────────────────────────────
        .route("/screenshot", get(screenshot))
        .route("/screen/poll/status", get(screen_capture_poll_status)) // screen capture poll status
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
        .route("/process/poll/status", get(process_tracker_poll_status)) // process tracker poll status
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
        .route("/alarms", get(alarms_snapshot)) // alarms snapshot
        .route("/resources/poll/status", get(system_resources_poll_status)) // system resources poll status
        .route("/resources/thresholds", get(thresholds)) // system resources thresholds
        .route("/resources/refresh-mask", get(refresh_mask)) // system resources refresh mask
        // ── Systemd ───────────────────────────────────────────────────────
        .route("/systemd", get(systemd_snapshot)) // systemd snapshot
        .route("/unit/{unit_name}", get(unit_snapshot)) // unit snapshot
        .route("/units/{unit_state}", get(units_by_active_state)) // units by active state
        .route("/failed_units", get(failed_units)) // failed_units
        .route("/systemd/poll/status", get(systemd_poll_status)) // systemd poll status
        // ── Docker Containers ───────────────────────────────────────────────────────
        .route("/docker-containers", get(list_docker_containers)) // docker containers
        .route("/container/{id_or_name}", get(get_docker_container)) // container by name or id
        .route("/top-containers", get(top_docker_containers)) // top containers
        .route("/docker/poll/status", get(docker_tracker_poll_status)) // docker tracker poll status
        // ── SSE ───────────────────────────────────────────────────────
        .route("/sse", get(sse_stream))
        .route("/sse/screen-capture", get(sse_stream_screen))
        .route("/sse/process-tracker", get(sse_stream_process))
        .route("/sse/system-resources", get(sse_stream_system_resources))
        .route("/sse/systemd", get(sse_stream_systemd))
        .route("/sse/docker-tracker", get(sse_stream_docker));
    if enable_shutdown {
        api = api.route("/shutdown", post(shutdown));
    }
    if auth_layer {
        api = api.layer(middleware::from_fn(auth_middleware));
    }
    api.with_state(cancel_token)
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
        .route("/systemd/control-unit", post(control_unit))
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

const fn should_enable_auth(config: &crate::config::AppConfig) -> bool {
    config.args.enable_auth
        || config.args.allow_process_commands
        || (!config.args.is_blind() && config.args.is_screen_commands_allowed())
        || (config.args.system_resources && config.args.allow_system_resources_commands)
        || (config.args.systemd && config.args.allow_systemd_commands)
        || (config.args.docker && config.args.allow_docker_commands)
}

pub fn create_routers(
    config: &crate::config::AppConfig,
    cancel_token: CancellationToken,
) -> Router {
    let api_router = create_api_router(
        cancel_token,
        config.args.enable_shutdown,
        config.args.enable_auth,
    );
    let mut app = Router::new()
        .nest("/api", api_router)
        .nest("/api", create_common_router());
    if should_enable_auth(config) {
        app = app.nest("/api/auth", create_auth_router());
    }
    if config.args.allow_process_commands {
        app = app.nest("/api", create_process_commands_router());
    }
    if config.args.is_screen_commands_allowed() {
        app = app.nest("/api", create_screen_commands_router());
    }
    if config.args.allow_system_resources_commands {
        app = app.nest("/api", create_sr_commands_router());
    }
    if config.args.allow_systemd_commands {
        app = app.nest("/api", create_systemd_commands_router());
    }
    if config.args.allow_docker_commands {
        app = app.nest("/api", create_docker_commands_router());
    }
    app
}
