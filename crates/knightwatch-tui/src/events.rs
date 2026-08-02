/// Everything that can happen while the app is running, from any source:
/// keyboard/mouse, background pollers, timers, etc.
pub enum AppEvent {
    /// Raw terminal input (key presses, mouse, resize, paste, focus).
    Input(crossterm::event::Event),

    /// A fresh screenshot of the primary monitor, for the Screen tab.
    ScreenImages(Vec<kw_types::api::ScreenshotImage>),

    /// A fresh system resources snapshot, for the System Resources tab.
    SystemSnapshot(kw_types::resources::SystemSnapshot),

    /// A fresh docker container snapshot, for the Docker tab.
    DockerContainers(Vec<kw_types::docker::ContainerSnapshot>),

    /// A fresh systemd snapshot, for the systemd tab.
    SystemdSnapshot(kw_types::systemd::SystemdSnapshot),

    /// A fresh process trees, for the systemd tab.
    ProcessTrees(Vec<kw_types::process::ProcessTree>),

    /// A fresh top processes snapshot, for the Top Processes tab.
    TopProcesses(Vec<kw_types::process::ProcessSnapshot>),

    /// login result from the login screen.
    LoginResult(Result<(), String>),

    /// logout result from the logout action.
    LogoutResult,

    // result of any fire-and-forget command a tab sent (poll
    // pause/resume/interval, kill, track, ...). `pid` is `None` for
    // poll-control commands, `Some(pid)` for process actions.
    CommandResult {
        tab: &'static str,
        label: &'static str,
        result: Result<CommandOutcome, String>,
    },

    /// result of the shutdown request.
    ShutdownResult(Result<(), String>),
}

/// Payload of a completed command. Most are `Ack`; a few endpoints return
/// data the tab needs (e.g. kill-tree reports which pids actually died).
pub enum CommandOutcome {
    Ack,
    /// pids that were actually killed, from `kill_process_tree`.
    KillTree(Vec<u32>),
}
