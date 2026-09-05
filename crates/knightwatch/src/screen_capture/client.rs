use tokio::sync::{broadcast, mpsc, oneshot};

use super::commands::{ScreenCaptureCommand, ScreenCaptureQuery};
use crate::prelude::*;

/// Subscribe to tracker events (e.g. from a Telegram bot or WebSocket handler).
/// Returns `None` if the tracker was not started.
#[cfg(feature = "screenshot")]
pub fn subscribe_events() -> Option<broadcast::Receiver<super::event::ScreenCaptureEvent>> {
    super::capture::SCREEN_CAPTURE_EVENT_SENDER
        .get()
        .map(tokio::sync::broadcast::Sender::subscribe)
}

#[cfg(not(feature = "screenshot"))]
pub const fn subscribe_events() -> Option<broadcast::Receiver<super::event::ScreenCaptureEvent>> {
    None
}

#[cfg(feature = "screenshot")]
fn get_screen_capture_query_sender() -> Option<&'static mpsc::Sender<ScreenCaptureQuery>> {
    super::capture::SCREEN_CAPTURE_QUERY_SENDER.get()
}

#[cfg(not(feature = "screenshot"))]
const fn get_screen_capture_query_sender() -> Option<&'static mpsc::Sender<ScreenCaptureQuery>> {
    None
}

#[cfg(feature = "screenshot")]
fn get_screen_capture_command_sender() -> Option<&'static mpsc::Sender<ScreenCaptureCommand>> {
    super::capture::SCREEN_CAPTURE_COMMAND_SENDER.get()
}

#[cfg(not(feature = "screenshot"))]
const fn get_screen_capture_command_sender() -> Option<&'static mpsc::Sender<ScreenCaptureCommand>>
{
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

pub async fn get_poll_status() -> Option<kw_types::polling::PollStatus> {
    let (tx, rx) = oneshot::channel();
    let _ = get_screen_capture_query_sender()?
        .send(ScreenCaptureQuery::PollStatus { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

/// Change the polling interval and restart the tick timer immediately.
pub async fn set_poll_interval(user: DisplayUser, interval: std::time::Duration) -> Result<()> {
    let tx_ref = get_screen_capture_command_sender().ok_or_else(Error::screen_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureCommand::SetPollInterval {
            user,
            interval,
            response: tx,
        })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Pause polling. The capture continues to handle queries and commands,
/// but `handle_tick` will not fire until `resume_poll` is called.
pub async fn pause_poll(user: DisplayUser) -> Result<()> {
    let tx_ref = get_screen_capture_command_sender().ok_or_else(Error::screen_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureCommand::PausePoll { user, response: tx })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}

/// Resume polling at the current poll interval.
pub async fn resume_poll(user: DisplayUser) -> Result<()> {
    let tx_ref = get_screen_capture_command_sender().ok_or_else(Error::screen_commands_disabled)?;
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(ScreenCaptureCommand::ResumePoll { user, response: tx })
        .await;
    rx.await.map_err(|err| Error::channel_closed(&err))?
}
