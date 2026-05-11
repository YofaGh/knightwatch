use std::sync::OnceLock;
use tokio::{sync::mpsc, time::Duration};
use xcap::Monitor;

use super::models::*;
use crate::prelude::*;

struct ScreenCapture {
    last_captures: Vec<Screenshot>,
    query_tx: mpsc::Sender<ScreenCaptureQuery>,
    query_rx: Option<mpsc::Receiver<ScreenCaptureQuery>>,
    poll_interval: Duration,
}

impl ScreenCapture {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        Self {
            last_captures: Vec::new(),
            query_tx,
            query_rx: Some(query_rx),
            poll_interval: Duration::from_secs(2),
        }
    }

    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    async fn start_capturing_loop(mut self) -> Result<()> {
        self.handle_tick().await;
        let mut query_rx = self.query_rx.take().expect("Failed to take query receiver");
        let mut poll_interval_timer = tokio::time::interval(self.poll_interval);
        loop {
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                _ = poll_interval_timer.tick() => {
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

    async fn handle_tick(&mut self) {
        match Self::screenshot_monitors() {
            Ok(captures) => self.last_captures = captures,
            Err(err) => error!("Failed to capture screenshots: {err}"),
        };
    }

    fn screenshot_monitors() -> Result<Vec<Screenshot>> {
        Self::get_monitors()?
            .into_iter()
            .map(|screen| Self::take_screenshot(&screen))
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
            .write_to(&mut buf, image::ImageFormat::Png)
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

pub fn init_screen_capture() {
    if get_config().args.blind {
        return;
    }
    let screen_capture = ScreenCapture::new();
    SCREEN_CAPTURE_QUERY_SENDER
        .set(screen_capture.query_tx.clone())
        .unwrap();
    tokio::spawn(async move {
        if let Err(e) = screen_capture.start_capturing_loop().await {
            error!(?e, "screen capture loop exited with error");
        }
    });
    info!("Screen Capture started");
}
