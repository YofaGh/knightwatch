#![allow(dead_code)]

use tokio::sync::{broadcast, mpsc, oneshot};

use crate::prelude::*;

#[derive(Debug)]
pub enum ScreenCaptureQuery {
    GetScreenshots {
        response: oneshot::Sender<Vec<super::screenshot::Screenshot>>,
    },
    PollStatus {
        response: oneshot::Sender<Option<kw_types::polling::PollStatus>>,
    },
}

/// Mutating commands that alter capture state or act on live processes.
/// These require `&mut self` and travel on a separate channel from read-only queries.
#[derive(Debug)]
pub enum ScreenCaptureCommand {
    /// Replace the polling interval and restart the tick timer immediately.
    SetPollInterval {
        user: DisplayUser,
        interval: std::time::Duration,
        response: oneshot::Sender<Result<()>>,
    },
    /// Stop emitting ticks; the capture keeps running and still handles queries/commands.
    PausePoll {
        user: DisplayUser,
        response: oneshot::Sender<Result<()>>,
    },
    /// Resume ticking at the current poll interval.
    ResumePoll {
        user: DisplayUser,
        response: oneshot::Sender<Result<()>>,
    },
}

/// Describes which mutating command was executed, with its parameters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScreenCaptureAction {
    SetPollInterval {
        interval: std::time::Duration,
    },
    PausePoll,
    ResumePoll,
}

impl ScreenCaptureAction {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetPollInterval { .. } => "set_poll_interval",
            Self::PausePoll => "pause_poll",
            Self::ResumePoll => "resume_poll",
        }
    }
}

impl std::fmt::Display for ScreenCaptureAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetPollInterval { interval } => {
                write!(f, "set poll interval to {}ms", interval.as_millis())
            }
            Self::PausePoll => write!(f, "pause polling"),
            Self::ResumePoll => write!(f, "resume polling"),
        }
    }
}

pub struct ScreenCaptureChannels {
    pub query_tx: mpsc::Sender<ScreenCaptureQuery>,
    pub query_rx: Option<mpsc::Receiver<ScreenCaptureQuery>>,
    pub command_tx: mpsc::Sender<ScreenCaptureCommand>,
    pub command_rx: Option<mpsc::Receiver<ScreenCaptureCommand>>,
    pub event_tx: broadcast::Sender<super::event::ScreenCaptureEvent>,
}

impl ScreenCaptureChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(256);
        let (event_tx, _) = broadcast::channel(64);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            command_tx,
            command_rx: Some(command_rx),
            event_tx,
        }
    }

    pub fn take_query_rx(&mut self) -> Result<mpsc::Receiver<ScreenCaptureQuery>> {
        self.query_rx
            .take()
            .ok_or_else(|| Error::Screen("Query receiver already taken".into()))
    }

    pub fn take_command_rx(&mut self) -> Result<mpsc::Receiver<ScreenCaptureCommand>> {
        self.command_rx
            .take()
            .ok_or_else(|| Error::Screen("Command receiver already taken".into()))
    }
}
