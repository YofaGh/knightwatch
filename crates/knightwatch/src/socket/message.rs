use serde::{Deserialize, Serialize};
use std::time::Duration;

use kw_types::{
    polling::PollStatus,
    process::{self, ProcessSnapshot, ProcessTree},
};

use super::client::ClientId;
use crate::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketMessage {
    Handshake,
    HandshakeResponse,
    Action {
        action: SocketAction,
    },
    Query {
        query: SocketQuery,
    },
    QueryResponse {
        response: SocketQueryResponse,
    },
    Command {
        command: SocketCommand,
    },
    CommandResponse {
        success: bool,
        err: Option<Error>,
        response: SocketCommandResponse,
    },
    Event {
        event: crate::events::EventPayload,
    },
    AuthenticationFailed {
        reason: AuthFailedReason,
    },
    AuthenticationSucceed,
    ShutdownNotEnabled,
    Unauthorized,
    ShuttingDown,
    SetEventPreferences,
}

#[derive(Debug)]
pub struct SocketQueryRequset {
    pub client_id: ClientId,
    pub query: SocketQuery,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQuery {
    // Common
    Info,
    // Screen Capture
    Screenshots,
    ScreenCapturePollStatus,
    // Process Tracker
    RootPids,
    Root {
        root_pid: u32,
    },
    Children {
        root_pid: u32,
    },
    IsProcessDone {
        root_pid: u32,
    },
    ProcessTree {
        root_pid: u32,
    },
    AllProcessTrees,
    ProcessStatus {
        root_pid: u32,
    },
    TopProcesses {
        by: kw_types::process::ProcessesSortKey,
        limit: usize,
    },
    ProcessTrackerPollStatus,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQueryResponse {
    // Common
    Info {
        info: kw_types::api::InfoResponse,
    },
    // Screen
    Screenshots {
        screenshots: Vec<kw_types::screen::Screenshot>,
    },
    ScreenCapturePollStatus {
        status: Option<PollStatus>,
    },
    // Process Tracker
    RootPids {
        pids: Vec<u32>,
    },
    Root {
        snapshot: Option<ProcessSnapshot>,
    },
    Children {
        snapshots: Vec<ProcessSnapshot>,
    },
    IsProcessDone {
        done: Option<bool>,
    },
    ProcessTree {
        tree: Option<ProcessTree>,
    },
    AllProcessTrees {
        trees: Vec<ProcessTree>,
    },
    ProcessStatus {
        status: Option<process::ProcessStatus>,
    },
    TopProcesses {
        snapshots: Vec<ProcessSnapshot>,
    },
    ProcessTrackerPollStatus {
        status: Option<PollStatus>,
    },
}

#[derive(Debug)]
pub struct SocketActionRequset {
    pub client_id: ClientId,
    pub action: SocketAction,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketAction {
    Login {
        username: String,
        password: String,
    },
    Logout,
    Shutdown,
    SetEventPreferences {
        events_enabled: Option<bool>,
        ticks_enabled: Option<bool>,
    },
}

#[derive(Debug)]
pub struct SocketCommandRequset {
    pub client_id: ClientId,
    pub command: SocketCommand,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCommand {
    // Screen Capture
    ScreenCapturePollInterval {
        interval: Duration,
    },
    ScreenCapturePollPause,
    ScreenCapturePollResume,
    // Process Tracker
    KillProcess {
        pid: u32,
        signal: kw_types::process::ProcessSignal,
    },
    KillTree {
        root_pid: u32,
    },
    TrackPid {
        pid: u32,
    },
    UntrackPid {
        pid: u32,
    },
    ProcessTrackerPollInterval {
        interval: Duration,
    },
    ProcessTrackerPollPause,
    ProcessTrackerPollResume,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCommandResponse {
    // Screen
    ScreenPollInterval,
    ScreenPollPause,
    ScreenPollResume,
    // Process Tracker
    KillProcess { os_result: bool },
    KillTree { killed_pids: Vec<u32> },
    TrackPid,
    UntrackPid,
    ProcessTrackerPollInterval,
    ProcessTrackerPollPause,
    ProcessTrackerPollResume,
}

pub struct CorrelatedSocketMessage {
    pub message: SocketMessage,
    pub response_tx: tokio::sync::oneshot::Sender<Result<()>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AuthFailedReason {
    WrongCredentials,
    InternalServerError,
}
