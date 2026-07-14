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
}