use tokio::sync::{mpsc, oneshot};

use super::commands::{ScreenCaptureCommand, ScreenCaptureQuery};
use crate::prelude::*;

#[cfg(feature = "screenshot")]
fn get_screen_capture_query_sender() -> Option<&'static mpsc::Sender<ScreenCaptureQuery>> {
    super::capture::SCREEN_CAPTURE_QUERY_SENDER.get()
}

#[cfg(not(feature = "screenshot"))]
fn get_screen_capture_query_sender() -> Option<&'static mpsc::Sender<ScreenCaptureQuery>> {
    None
}

#[cfg(feature = "screenshot")]
fn get_screen_capture_command_sender() -> Option<&'static mpsc::Sender<ScreenCaptureCommand>> {
    super::capture::SCREEN_CAPTURE_COMMAND_SENDER.get()
}

#[cfg(not(feature = "screenshot"))]
fn get_screen_capture_command_sender() -> Option<&'static mpsc::Sender<ScreenCaptureCommand>> {
    None
}

pub async fn get_screenshots() -> Vec<super::screenshot::Screenshot> {
    let Some(tx_ref) = get_screen_capture_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureQuery::GetScreenshots { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

/// Change the polling interval and restart the tick timer immediately.
pub async fn set_poll_interval(interval: std::time::Duration) -> Result<()> {
    let tx_ref = get_screen_capture_command_sender().ok_or_else(Error::screen_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureCommand::SetPollInterval {
            interval,
            response: tx,
        })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Pause polling. The capture continues to handle queries and commands,
/// but `handle_tick` will not fire until `resume_poll` is called.
pub async fn pause_poll() -> Result<()> {
    let tx_ref = get_screen_capture_command_sender().ok_or_else(Error::screen_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureCommand::PausePoll { response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}

/// Resume polling at the current poll interval.
pub async fn resume_poll() -> Result<()> {
    let tx_ref = get_screen_capture_command_sender().ok_or_else(Error::screen_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureCommand::ResumePoll { response: tx })
        .await;
    rx.await.map_err(Error::channel_closed)?
}
