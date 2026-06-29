use std::sync::OnceLock;
use tokio::{sync::mpsc, time::Duration};
use xcap::Monitor;

use super::{commands::*, screenshot::*};
use crate::prelude::*;

struct ScreenCapture {
    last_captures: Vec<Screenshot>,
    channels: ScreenCaptureChannels,
    poll_interval: Duration,
    poll_interval_timer: Option<tokio::time::Interval>,
}

impl ScreenCapture {
    pub fn new() -> Self {
        Self {
            last_captures: Vec::new(),
            channels: ScreenCaptureChannels::new(),
            poll_interval: Duration::from_secs(2),
            poll_interval_timer: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    async fn start_capturing_loop(mut self) -> Result<()> {
        self.handle_tick().await;
        let mut query_rx = self
            .channels
            .take_query_rx()
            .expect("Failed to take query receiver");
        let mut command_rx = self
            .channels
            .take_command_rx()
            .expect("Failed to take command receiver");
        self.poll_interval_timer = Some(tokio::time::interval(self.poll_interval));
        loop {
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                Some(command) = command_rx.recv() => {
                    self.handle_command(command);
                }
                _ = async { self.poll_interval_timer.as_mut().unwrap().tick().await }, if self.poll_interval_timer.is_some() => {
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
        }
    }

    fn handle_command(&mut self, command: ScreenCaptureCommand) {
        match command {
            // ----------------------------------------------------------------
            // Polling control.
            // ----------------------------------------------------------------
            ScreenCaptureCommand::SetPollInterval { interval, response } => {
                self.poll_interval = interval;
                self.poll_interval_timer = Some(tokio::time::interval(interval));
                info!(ms = interval.as_millis(), "poll interval updated");
                let _ = response.send(Ok(()));
            }
            ScreenCaptureCommand::PausePoll { response } => {
                self.poll_interval_timer = None;
                info!("polling paused");
                let _ = response.send(Ok(()));
            }
            ScreenCaptureCommand::ResumePoll { response } => {
                self.poll_interval_timer = Some(tokio::time::interval(self.poll_interval));
                info!("polling resumed");
                let _ = response.send(Ok(()));
            }
        }
    }

    async fn handle_tick(&mut self) {
        match Self::screenshot_monitors().await {
            Ok(captures) => self.last_captures = captures,
            Err(err) => error!("Failed to capture screenshots: {err}"),
        };
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
            .map(Self::take_screenshot)
            .collect()
    }

    fn get_monitors() -> Result<Vec<Monitor>> {
        Monitor::all().map_err(|e| Error::Screen(format!("Failed to get monitors: {e}")))
    }

    fn take_screenshot(monitor: Monitor) -> Result<Screenshot> {
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
}

pub static SCREEN_CAPTURE_QUERY_SENDER: OnceLock<mpsc::Sender<ScreenCaptureQuery>> =
    OnceLock::new();
pub static SCREEN_CAPTURE_COMMAND_SENDER: OnceLock<mpsc::Sender<ScreenCaptureCommand>> =
    OnceLock::new();

pub fn init_screen_capture() {
    let config = get_config();
    if config.args.blind {
        return;
    }
    let screen_capture = ScreenCapture::new();
    SCREEN_CAPTURE_QUERY_SENDER
        .set(screen_capture.channels.query_tx.clone())
        .unwrap();
    if config.args.allow_screen_commands {
        SCREEN_CAPTURE_COMMAND_SENDER
            .set(screen_capture.channels.command_tx.clone())
            .unwrap();
    }
    tokio::spawn(async move {
        if let Err(e) = screen_capture.start_capturing_loop().await {
            error!(?e, "screen capture loop exited with error");
        }
    });
    info!("Screen Capture started");
}
