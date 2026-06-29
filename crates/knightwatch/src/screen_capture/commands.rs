#![allow(dead_code)]

use tokio::sync::{mpsc, oneshot};

use crate::prelude::*;

#[derive(Debug)]
pub enum ScreenCaptureQuery {
    GetScreenshots {
        response: tokio::sync::oneshot::Sender<Vec<super::screenshot::Screenshot>>,
    },
}

/// Mutating commands that alter capture state or act on live processes.
/// These require `&mut self` and travel on a separate channel from read-only queries.
#[derive(Debug)]
pub enum ScreenCaptureCommand {
    /// Replace the polling interval and restart the tick timer immediately.
    SetPollInterval {
        interval: std::time::Duration,
        response: oneshot::Sender<Result<()>>,
    },
    /// Stop emitting ticks; the capture keeps running and still handles queries/commands.
    PausePoll {
        response: oneshot::Sender<Result<()>>,
    },
    /// Resume ticking at the current poll interval.
    ResumePoll {
        response: oneshot::Sender<Result<()>>,
    },
}

pub struct ScreenCaptureChannels {
    pub query_tx: mpsc::Sender<ScreenCaptureQuery>,
    pub query_rx: Option<mpsc::Receiver<ScreenCaptureQuery>>,
    pub command_tx: mpsc::Sender<ScreenCaptureCommand>,
    pub command_rx: Option<mpsc::Receiver<ScreenCaptureCommand>>,
}

impl ScreenCaptureChannels {
    pub fn new() -> Self {
        let (query_tx, query_rx) = mpsc::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(256);
        Self {
            query_tx,
            query_rx: Some(query_rx),
            command_tx,
            command_rx: Some(command_rx),
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
