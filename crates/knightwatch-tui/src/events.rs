/// Everything that can happen while the app is running, from any source:
/// keyboard/mouse, background pollers, timers, etc.
///
/// The main loop only ever awaits on one channel of `AppEvent`s. Adding a
/// new tab with its own data source (processes, systemd, docker, ...) means
/// adding one variant here and one `tokio::spawn`'d producer in `main` that
/// sends into the shared `tx` — nothing else about the loop changes.
pub enum AppEvent {
    /// Raw terminal input (key presses, mouse, resize, paste, focus).
    Input(crossterm::event::Event),

    /// A fresh screenshot of the primary monitor, for the Screen tab.
    ScreenImage(image::DynamicImage),
}