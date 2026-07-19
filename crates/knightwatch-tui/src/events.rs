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
}