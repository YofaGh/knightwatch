use std::sync::{OnceLock, Mutex};
use tokio::sync::{broadcast, mpsc};
use xcap::Monitor;

use kw_types::polling::Poll;

use super::{
    commands::{
        ScreenCaptureAction, ScreenCaptureChannels, ScreenCaptureCommand, ScreenCaptureQuery,
    },
    event::ScreenCaptureEvent,
    screenshot::Screenshot,
};
use crate::prelude::*;

struct ScreenCapture {
    last_captures: Vec<Screenshot>,
    channels: ScreenCaptureChannels,
    poll: Poll,
}

impl ScreenCapture {
    pub fn new() -> Self {
        Self {
            last_captures: Vec::new(),
            channels: ScreenCaptureChannels::new(),
            poll: Poll::new(5),
        }
    }

    fn emit_event(&self, event: ScreenCaptureEvent) {
        // Err means no subscribers — that's fine.
        let _ = self.channels.event_tx.send(event);
    }

    async fn start_capturing_loop(mut self) -> Result<()> {
        self.handle_tick().await;
        let mut query_rx = self.channels.take_query_rx()?;
        let mut command_rx = self.channels.take_command_rx()?;
        self.poll.resume();
        loop {
            let tick = async {
                match self.poll.interval_timer.as_mut() {
                    Some(timer) => timer.tick().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                Some(command) = command_rx.recv() => {
                    self.handle_command(command);
                }
                _ = tick => {
                    self.handle_tick().await;
                }
            }
        }
    }

    fn handle_query(&self, query: ScreenCaptureQuery) {
        match query {
            ScreenCaptureQuery::GetScreenshots { response } => {
                let _ = response.send(self.last_captures.clone());
            }
            ScreenCaptureQuery::PollStatus { response } => {
                let _ = response.send(Some((&self.poll).into()));
            }
        }
    }

    fn handle_command(&mut self, command: ScreenCaptureCommand) {
        match command {
            // ----------------------------------------------------------------
            // Polling control.
            // ----------------------------------------------------------------
            ScreenCaptureCommand::SetPollInterval {
                user,
                interval,
                response,
            } => {
                self.poll.set_interval(interval);
                info!(
                    ms = interval.as_millis(),
                    "screen capture poll interval updated"
                );
                let result = Ok(());
                self.emit_command_event(
                    user,
                    ScreenCaptureAction::SetPollInterval { interval },
                    &result,
                );
                let _ = response.send(result);
            }
            ScreenCaptureCommand::PausePoll { user, response } => {
                self.poll.pause();
                info!("screen capture polling paused");
                let result = Ok(());
                self.emit_command_event(user, ScreenCaptureAction::PausePoll, &result);
                let _ = response.send(result);
            }
            ScreenCaptureCommand::ResumePoll { user, response } => {
                self.poll.resume();
                info!("screen capture polling resumed");
                let result = Ok(());
                self.emit_command_event(user, ScreenCaptureAction::ResumePoll, &result);
                let _ = response.send(result);
            }
        }
    }

    async fn handle_tick(&mut self) {
        match Self::screenshot_monitors().await {
            Ok(captures) => self.last_captures = captures,
            Err(err) => error!("Failed to capture screenshots: {err}"),
        }
    }

    // Runs xcap (which calls zbus::blocking internally) on a dedicated
    // OS thread via spawn_blocking, so it never conflicts with the
    // Tokio runtime that owns the current thread.
    async fn screenshot_monitors() -> Result<Vec<Screenshot>> {
        tokio::task::spawn_blocking(Self::screenshot_monitors_blocking)
            .await
            .map_err(|e| Error::Screen(format!("spawn_blocking join error: {e}")))?
    }

    fn screenshot_monitors_blocking() -> Result<Vec<Screenshot>> {
        Self::get_monitors()?
            .into_iter()
            .map(|monitor| Self::take_screenshot(&monitor))
            .collect()
    }

    fn get_monitors() -> Result<Vec<Monitor>> {
        Monitor::all().map_err(|e| Error::Screen(format!("Failed to get monitors: {e}")))
    }

    fn take_screenshot(monitor: &Monitor) -> Result<Screenshot> {
        let rgba_img = monitor
            .capture_image()
            .map_err(|e| Error::Screen(format!("Failed to capture: {e}")))?;
        let timestamp = crate::utils::now_rfc3339();
        let width = rgba_img.width();
        let height = rgba_img.height();
        let mut buf = std::io::Cursor::new(Vec::new());
        rgba_img
            .write_to(&mut buf, xcap::image::ImageFormat::Png)
            .map_err(|e| Error::Screen(format!("Failed to encode PNG: {e}")))?;
        Ok(Screenshot {
            image: buf.into_inner(),
            monitor_name: monitor
                .name()
                .map_err(|e| Error::Screen(format!("Failed to get monitor name: {e}")))?,
            monitor_id: monitor
                .id()
                .map_err(|e| Error::Screen(format!("Failed to get monitor id: {e}")))?,
            width,
            height,
            timestamp,
        })
    }

    /// Emits a `CommandExecuted` event for any mutating command,
    /// or any target info already lives on `action`.
    fn emit_command_event(
        &self,
        user: DisplayUser,
        action: ScreenCaptureAction,
        result: &Result<()>,
    ) {
        let (success, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };

        if success {
            info!(
                %user,
                action = %action,
                "screen command executed"
            );
        } else {
            warn!(
                %user,
                action = %action,
                error,
                "screen command failed"
            );
        }

        self.emit_event(ScreenCaptureEvent::CommandExecuted {
            user,
            action,
            success,
            error,
        });
    }
}

pub static SCREEN_CAPTURE_QUERY_SENDER: OnceLock<mpsc::Sender<ScreenCaptureQuery>> =
    OnceLock::new();
pub static SCREEN_CAPTURE_EVENT_SENDER: OnceLock<broadcast::Sender<ScreenCaptureEvent>> =
    OnceLock::new();
pub static SCREEN_CAPTURE_COMMAND_SENDER: OnceLock<mpsc::Sender<ScreenCaptureCommand>> =
    OnceLock::new();

static SCREEN_CAPTURE: OnceLock<Mutex<Option<ScreenCapture>>> = OnceLock::new();

pub fn init_screen_capture() {
    let config = get_config();
    if config.args.blind {
        return;
    }
    let screen_capture = ScreenCapture::new();
    let _ = SCREEN_CAPTURE_QUERY_SENDER.set(screen_capture.channels.query_tx.clone());
    let _ = SCREEN_CAPTURE_EVENT_SENDER.set(screen_capture.channels.event_tx.clone());
    if config.args.allow_screen_commands {
        let _ = SCREEN_CAPTURE_COMMAND_SENDER.set(screen_capture.channels.command_tx.clone());
    }
    let _ = SCREEN_CAPTURE.set(Mutex::new(Some(screen_capture)));
}

pub fn start_screen_capture() {
    let Some(screen_capture) = SCREEN_CAPTURE
        .get()
        .and_then(|cell| cell.lock().ok())
        .and_then(|mut guard| guard.take())
    else {
        return;
    };
    tokio::spawn(async move {
        if let Err(e) = screen_capture.start_capturing_loop().await {
            error!(?e, "screen capture loop exited with error");
        }
    });
    info!("Screen Capture started");
}
