use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    docker::{self, ContainerSnapshot},
    polling::PollStatus,
    process::{self, ProcessSnapshot, ProcessTree},
    resources,
    systemd::{self, UnitSnapshot},
    event,
};

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
        err: Option<String>,
        response: SocketCommandResponse,
    },
    Event {
        event: event::EventPayload,
    },
    AuthenticationFailed {
        reason: AuthFailedReason,
    },
    AuthenticationSucceed,
    ShutdownNotEnabled,
    Unauthorized,
    AlreadyAuthenticated,
    ShuttingDown,
    SetEventPreferences,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQuery {
    // Common
    Info,
    History {
        query: crate::history::HistoryQuery,
    },
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
        by: process::ProcessesSortKey,
        limit: usize,
    },
    ProcessTrackerPollStatus,
    // System Resources
    SystemSnapshot,
    Cpu,
    Memory,
    Disks,
    Networks,
    Gpus,
    Battery,
    HostInfo,
    Temperatures,
    Alarms,
    Thresholds,
    RefreshMask,
    SystemResourcesPollStatus,
    // Systemd
    SystemdSnapshot,
    Unit {
        unit_name: String,
    },
    UnitsByActiveState {
        state: systemd::UnitActiveState,
    },
    FailedUnits,
    SystemdPollStatus,
    // Docker Tracker
    ListContainers,
    Container {
        id_or_name: String,
    },
    TopContainers {
        by: docker::DockerSortKey,
        limit: usize,
    },
    DockerTrackerPollStatus,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQueryResponse {
    // Common
    Info {
        info: crate::Info,
    },
    History {
        events: Vec<event::StoredEvent>,
    },
    // Screen Capture
    Screenshots {
        screenshots: Vec<crate::screen::Screenshot>,
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
    // System Resources
    SystemSnapshot {
        snapshot: Box<Option<resources::SystemSnapshot>>,
    },
    Cpu {
        snapshot: Option<resources::CpuSnapshot>,
    },
    Memory {
        snapshot: Option<resources::MemorySnapshot>,
    },
    Disks {
        snapshots: Vec<resources::DiskSnapshot>,
    },
    Networks {
        snapshots: Vec<resources::NetworkSnapshot>,
    },
    Gpus {
        snapshots: Vec<resources::GpuSnapshot>,
    },
    Battery {
        snapshot: Option<resources::BatterySnapshot>,
    },
    HostInfo {
        info: Option<resources::HostInfo>,
    },
    Temperatures {
        snapshots: Vec<resources::ThermalSnapshot>,
    },
    Alarms {
        snapshot: Option<resources::AlarmSnapshot>,
    },
    Thresholds {
        thresholds: Option<resources::Thresholds>,
    },
    RefreshMask {
        refresh_mask: Option<resources::RefreshMask>,
    },
    SystemResourcesPollStatus {
        status: Option<PollStatus>,
    },
    // Systemd
    SystemdSnapshot {
        snapshot: Option<systemd::SystemdSnapshot>,
    },
    Unit {
        snapshot: Option<UnitSnapshot>,
    },
    UnitsByActiveState {
        snapshots: Vec<UnitSnapshot>,
    },
    FailedUnits {
        snapshots: Vec<UnitSnapshot>,
    },
    SystemdPollStatus {
        status: Option<PollStatus>,
    },
    // Docker Tracker
    ListContainers {
        snapshots: Vec<ContainerSnapshot>,
    },
    Container {
        snapshot: Option<ContainerSnapshot>,
    },
    TopContainers {
        snapshots: Vec<ContainerSnapshot>,
    },
    DockerTrackerPollStatus {
        status: Option<PollStatus>,
    },
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
        signal: process::ProcessSignal,
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
    // System Resources
    SetThresholds {
        thresholds: resources::Thresholds,
    },
    SetRefreshMask {
        refresh_mask: resources::RefreshMask,
    },
    SystemResourcesPollInterval {
        interval: Duration,
    },
    SystemResourcesPollPause,
    SystemResourcesPollResume,
    // Systemd
    ControlUnit {
        unit_name: String,
        action: systemd::ServiceAction,
    },
    SystemdPollInterval {
        interval: Duration,
    },
    SystemdPollPause,
    SystemdPollResume,
    // Docker Tracker
    StopContainer {
        id_or_name: String,
        timeout_secs: Option<i32>,
    },
    KillContainer {
        id_or_name: String,
        signal: Option<String>,
    },
    StartContainer {
        id_or_name: String,
    },
    RestartContainer {
        id_or_name: String,
        timeout_secs: Option<i32>,
    },
    PauseContainer {
        id_or_name: String,
    },
    UnpauseContainer {
        id_or_name: String,
    },
    DockerTrackerPollInterval {
        interval: Duration,
    },
    DockerTrackerPollPause,
    DockerTrackerPollResume,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SocketCommandResponse {
    // Screen Capture
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
    // System Resources
    SetThresholds,
    SetRefreshMask,
    SystemResourcesPollInterval,
    SystemResourcesPollPause,
    SystemResourcesPollResume,
    // Systemd
    ControlUnit,
    SystemdPollInterval,
    SystemdPollPause,
    SystemdPollResume,
    // Docker Tracker
    StopContainer,
    KillContainer,
    StartContainer,
    RestartContainer,
    PauseContainer,
    UnpauseContainer,
    DockerTrackerPollInterval,
    DockerTrackerPollPause,
    DockerTrackerPollResume,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AuthFailedReason {
    WrongCredentials,
    InternalServerError,
}
