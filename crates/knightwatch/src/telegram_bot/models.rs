use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use teloxide::types::ChatId;

use crate::{
    prelude::*,
    process_tracker::ProcessSignal,
    system_resources::{RefreshMask, Thresholds},
};

#[derive(teloxide::utils::command::BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "Start the bot and show the main menu")]
    Start,
    #[command(description = "Show the main menu")]
    Menu,
    #[command(description = "Show this help message")]
    Help,
    #[command(description = "Authenticate using token")]
    Auth,
    #[command(description = "Get Screenshot of all monitors")]
    Screenshot,
    #[command(description = "Get Process Info")]
    Process,
    #[command(description = "Get Top Processes Info")]
    TopProcesses,
    #[command(description = "Get System Snapshot")]
    SystemSnapshot,
    #[command(description = "Stop Knight Watch")]
    StopKnightWatch,
}

#[derive(Debug, Clone)]
pub struct State {
    chats: Arc<Mutex<HashMap<ChatId, Chat>>>,
    auth_enabled: bool,
}

impl State {
    pub fn new() -> Self {
        let chats = Arc::new(Mutex::new(HashMap::new()));
        let pre_auth_ids = get_users().get_telegram_chat_ids().into_iter().map(ChatId);
        if let Ok(mut g) = chats.lock() {
            for chat_id in pre_auth_ids {
                g.insert(chat_id, Chat::new_authed());
            }
        }
        Self {
            chats,
            auth_enabled: get_config().args.enable_auth,
        }
    }

    pub fn get_chat_ids(&self) -> Vec<ChatId> {
        self.chats
            .lock()
            .ok()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_authed_chat_ids(&self) -> Vec<ChatId> {
        self.chats
            .lock()
            .ok()
            .map(|g| {
                g.iter()
                    .filter(|(_, chat)| chat.auth_state == AuthState::Authenticated)
                    .map(|(&id, _)| id)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_relevant_chat_ids(&self) -> Vec<ChatId> {
        if self.auth_enabled {
            self.get_authed_chat_ids()
        } else {
            self.get_chat_ids()
        }
    }

    pub fn remove_chat(&mut self, chat_id: ChatId) {
        if let Ok(mut g) = self.chats.lock() {
            g.remove(&chat_id);
        }
    }

    pub fn get_chat_state(&self, chat_id: ChatId) -> ChatState {
        self.chats
            .lock()
            .ok()
            .and_then(|g| g.get(&chat_id).map(|c| c.chat_state.clone()))
            .unwrap_or(ChatState::Idle)
    }

    pub fn get_chat_auth(&self, chat_id: ChatId) -> AuthState {
        self.chats
            .lock()
            .ok()
            .and_then(|g| g.get(&chat_id).map(|c| c.auth_state.clone()))
            .unwrap_or(AuthState::Unauthenticated)
    }

    pub fn set_chat_state(&self, chat_id: ChatId, state: ChatState) {
        if let Ok(mut g) = self.chats.lock() {
            g.entry(chat_id).or_insert_with(Chat::new).chat_state = state;
        }
    }

    pub fn set_chat_auth(&self, chat_id: ChatId, auth: AuthState) {
        if let Ok(mut g) = self.chats.lock() {
            g.entry(chat_id).or_insert_with(Chat::new).auth_state = auth;
        }
    }

    pub fn set_chat_state_idle(&self, chat_id: ChatId) {
        self.set_chat_state(chat_id, ChatState::Idle);
    }

    pub fn is_authorized(&self, chat_id: ChatId) -> bool {
        if !self.auth_enabled {
            return true;
        }
        self.get_chat_auth(chat_id) == AuthState::Authenticated
    }

    pub fn is_authorized_to_commmand(&self, chat_id: ChatId) -> bool {
        self.get_chat_auth(chat_id) == AuthState::Authenticated
    }
}

#[derive(Debug, Clone)]
pub struct Chat {
    pub chat_state: ChatState,
    pub auth_state: AuthState,
}

impl Chat {
    pub fn new() -> Self {
        Self {
            chat_state: ChatState::Idle,
            auth_state: AuthState::Unauthenticated,
        }
    }

    pub fn new_authed() -> Self {
        Self {
            chat_state: ChatState::Idle,
            auth_state: AuthState::Authenticated,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Subsystem {
    ProcessTracker,
    ScreenCapture,
    SystemResources,
    Systemd,
    DockerTracker,
}

impl Subsystem {
    pub fn label(&self) -> &'static str {
        match self {
            Subsystem::ProcessTracker => "Process Tracker",
            Subsystem::ScreenCapture => "Screen Capture",
            Subsystem::SystemResources => "System Resources",
            Subsystem::Systemd => "Systemd",
            Subsystem::DockerTracker => "Docker Tracker",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatState {
    Idle,
    AwaitingUnitName,
    AwaitingAuthToken,
    AwaitingPollInterval { subsystem: Subsystem },
}

/// Inline-button callback actions, encoded as `"action:payload"` strings.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessCallbackAction {
    /// `"track:1234"`
    Track { pid: u32 },
    /// `"untrack:1234"`
    Untrack { pid: u32 },
    /// `"killtree:1234"`
    KillTree { pid: u32 },
    /// `"signal:1234:kill"` / `"signal:1234:term"` etc. — uses ProcessSignal's Display/TryFrom
    Signal { pid: u32, signal: ProcessSignal },
}

impl ProcessCallbackAction {
    pub fn encode(&self) -> String {
        match self {
            ProcessCallbackAction::Track { pid } => format!("track:{pid}"),
            ProcessCallbackAction::Untrack { pid } => format!("untrack:{pid}"),
            ProcessCallbackAction::KillTree { pid } => format!("killtree:{pid}"),
            ProcessCallbackAction::Signal { pid, signal } => format!("signal:{pid}:{signal}"),
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        match parts.as_slice() {
            ["track", pid] => Some(ProcessCallbackAction::Track {
                pid: pid.parse().ok()?,
            }),
            ["untrack", pid] => Some(ProcessCallbackAction::Untrack {
                pid: pid.parse().ok()?,
            }),
            ["killtree", pid] => Some(ProcessCallbackAction::KillTree {
                pid: pid.parse().ok()?,
            }),
            ["signal", pid, sig] => {
                let signal = ProcessSignal::try_from(*sig).ok()?;
                Some(ProcessCallbackAction::Signal {
                    pid: pid.parse().ok()?,
                    signal,
                })
            }
            _ => None,
        }
    }
}

/// Inline-button callback actions for system resources commands.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemResourcesCallbackAction {
    /// `"sr_set_thresholds:cpu_warn:memory_warn:disk_warn:battery_low"`
    SetThresholds(Thresholds),
    /// `"sr_set_refresh_mask:cpu:memory:disks:networks:temperatures:gpus"`
    SetRefreshMask(RefreshMask),
}

impl SystemResourcesCallbackAction {
    pub fn encode(&self) -> String {
        match self {
            SystemResourcesCallbackAction::SetThresholds(t) => format!(
                "sr_set_thresholds:{}:{}:{}:{}",
                t.cpu_warn, t.memory_warn, t.disk_warn, t.battery_low
            ),
            SystemResourcesCallbackAction::SetRefreshMask(m) => format!(
                "sr_set_refresh_mask:{}:{}:{}:{}:{}:{}",
                m.cpu as u8,
                m.memory as u8,
                m.disks as u8,
                m.networks as u8,
                m.temperatures as u8,
                m.gpus as u8,
            ),
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(7, ':').collect();
        match parts.as_slice() {
            [
                "sr_set_thresholds",
                cpu_warn,
                memory_warn,
                disk_warn,
                battery_low,
            ] => Some(SystemResourcesCallbackAction::SetThresholds(Thresholds {
                cpu_warn: cpu_warn.parse().ok()?,
                memory_warn: memory_warn.parse().ok()?,
                disk_warn: disk_warn.parse().ok()?,
                battery_low: battery_low.parse().ok()?,
            })),
            [
                "sr_set_refresh_mask",
                cpu,
                memory,
                disks,
                networks,
                temperatures,
                gpus,
            ] => Some(SystemResourcesCallbackAction::SetRefreshMask(RefreshMask {
                cpu: *cpu != "0",
                memory: *memory != "0",
                disks: *disks != "0",
                networks: *networks != "0",
                temperatures: *temperatures != "0",
                gpus: *gpus != "0",
            })),
            _ => None,
        }
    }
}

/// Inline-button callback actions for docker container commands.
/// Encoded as `"dc_<action>:<id>"`.
#[derive(Debug, Clone, PartialEq)]
pub enum DockerCallbackAction {
    Stop { id: String },
    Start { id: String },
    Kill { id: String },
    Restart { id: String },
    Pause { id: String },
    Unpause { id: String },
}

impl DockerCallbackAction {
    pub fn encode(&self) -> String {
        match self {
            DockerCallbackAction::Stop { id } => format!("dc_stop:{id}"),
            DockerCallbackAction::Start { id } => format!("dc_start:{id}"),
            DockerCallbackAction::Kill { id } => format!("dc_kill:{id}"),
            DockerCallbackAction::Restart { id } => format!("dc_restart:{id}"),
            DockerCallbackAction::Pause { id } => format!("dc_pause:{id}"),
            DockerCallbackAction::Unpause { id } => format!("dc_unpause:{id}"),
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        let (prefix, id) = s.split_once(':')?;
        let id = id.to_string();
        match prefix {
            "dc_stop" => Some(DockerCallbackAction::Stop { id }),
            "dc_start" => Some(DockerCallbackAction::Start { id }),
            "dc_kill" => Some(DockerCallbackAction::Kill { id }),
            "dc_restart" => Some(DockerCallbackAction::Restart { id }),
            "dc_pause" => Some(DockerCallbackAction::Pause { id }),
            "dc_unpause" => Some(DockerCallbackAction::Unpause { id }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    Unauthenticated,
    Authenticated,
}

pub struct TelegramBot {
    pub shutdown_token: teloxide::dispatching::ShutdownToken,
}

impl TelegramBot {
    pub async fn shutdown(self) {
        match self.shutdown_token.shutdown() {
            Ok(fut) => fut.await,
            Err(err) => tracing::error!(err = err.to_string(), "Failed to shutdown telegram bot"),
        }
    }
}
