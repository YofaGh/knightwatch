mod auth;
mod docker;
mod process;
mod screenshot;
mod settings;
mod system_resources;
mod systemd;

pub use auth::*;
pub use docker::*;
pub use process::*;
pub use screenshot::*;
pub use settings::*;
pub use system_resources::*;
pub use systemd::*;

use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{ParseMode, ReplyMarkup},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    keyboards::main_keyboard,
    models::{AuthState, ChatState, State, Subsystem},
    utils::escape_mdv2,
};
use crate::{docker_tracker::DockerSortKey, prelude::*};

pub async fn handle_start(
    bot: Bot,
    msg: Message,
    chat_state_tx: Arc<mpsc::Sender<(ChatId, AuthState)>>,
    state: State,
) -> Result<()> {
    super::broadcast::send_chat_state(
        &chat_state_tx,
        (msg.chat.id, state.get_chat_auth(msg.chat.id)),
    )
    .await;
    bot.send_message(
        msg.chat.id,
        "🤖 *Knight Watch BOT*\n\nChoose an action below:",
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
    .await?;
    Ok(())
}

pub async fn handle_help(bot: Bot, msg: Message) -> Result<()> {
    bot.send_message(
        msg.chat.id,
        "🚀 *This is a bot ran by Knight Watch:*\n\
         • Receive Screenshot of Monitors\n\
         • Get Process Info\n\
         • Get System Resources\n\
         • Monitor Systemd Units\n\
         • Stop the knight Watch",
    )
    .parse_mode(ParseMode::MarkdownV2)
    .await?;
    Ok(())
}

pub async fn handle_stop(bot: Bot, msg: Message, cancel_token: CancellationToken) -> Result<()> {
    bot.send_message(msg.chat.id, "🛑 Stopping Knight Watch…")
        .await?;
    cancel_token.cancel();
    Ok(())
}

pub async fn handle_plain_message(
    bot: Bot,
    msg: Message,
    chat_state_tx: Arc<mpsc::Sender<(ChatId, AuthState)>>,
    cancel_token: CancellationToken,
    state: State,
) -> Result<()> {
    let msg_clone = msg.clone();
    let text = match msg_clone.text() {
        Some(t) => t,
        None => return Ok(()),
    };
    let chat_state = state.get_chat_state(msg.chat.id);
    if chat_state == ChatState::AwaitingUnitName && text != "❌ Cancel" {
        return handle_systemd_unit_lookup(bot, msg, state, text.to_string()).await;
    }
    if chat_state == ChatState::AwaitingAuthToken && text != "❌ Cancel" {
        return handle_auth_token(bot, msg, chat_state_tx, state, text.to_string()).await;
    }
    if let ChatState::AwaitingPollInterval { subsystem } = chat_state
        && text != "❌ Cancel"
    {
        return handle_poll_interval_input(bot, msg, state, subsystem, text.to_string()).await;
    }

    match text {
        "📋 Help" => handle_help(bot, msg).await?,
        "🖼️ Screenshot" => handle_screenshot(bot, msg, state).await?,
        "🖥️ System Resources" => handle_system_resources(bot, msg, state).await?,
        "📊 Process" => handle_process(bot, msg, state).await?,
        "📊 Top Processes" => handle_top_processes_menu(bot, msg, state).await?,
        "🔥 By CPU" => {
            handle_top_processes_by(bot, msg, crate::process_tracker::SortKey::Cpu).await?
        }
        "🧠 By Memory" => {
            handle_top_processes_by(bot, msg, crate::process_tracker::SortKey::Memory).await?
        }
        "💾 By Disk" => {
            handle_top_processes_by(bot, msg, crate::process_tracker::SortKey::Disk).await?
        }
        "🔧 Systemd" => handle_systemd_menu(bot, msg, state).await?,
        "📋 Systemd Overview" => handle_systemd_overview(bot, msg).await?,
        "🔴 Failed Units" => handle_systemd_failed(bot, msg).await?,
        "🔍 Unit Status" => handle_systemd_unit_prompt(bot, msg, state).await?,
        "🐳 Docker" => handle_docker_menu(bot, msg, state).await?,
        "📋 Docker Containers" => handle_docker_list(bot, msg, state).await?,
        "🔥 By CPU (Docker)" => handle_docker_top(bot, msg, state, DockerSortKey::Cpu).await?,
        "🧠 By Memory (Docker)" => {
            handle_docker_top(bot, msg, state, DockerSortKey::Memory).await?
        }
        "⚙️ Settings" => handle_settings_menu(bot, msg, state).await?,
        "⏱️ Polling" => handle_polling_menu(bot, msg, state).await?,
        // Subsystem polling pickers
        "⏱️ Process Tracker Polling" => {
            handle_subsystem_polling_menu(bot, msg, state, Subsystem::ProcessTracker).await?
        }
        "⏱️ Screen Capture Polling" => {
            handle_subsystem_polling_menu(bot, msg, state, Subsystem::ScreenCapture).await?
        }
        "⏱️ System Resources Polling" => {
            handle_subsystem_polling_menu(bot, msg, state, Subsystem::SystemResources).await?
        }
        "⏱️ Systemd Polling" => {
            handle_subsystem_polling_menu(bot, msg, state, Subsystem::Systemd).await?
        }
        "⏱️ Docker Tracker Polling" => {
            handle_subsystem_polling_menu(bot, msg, state, Subsystem::DockerTracker).await?
        }
        // Per-subsystem pause
        "⏸️ Pause Process Tracker" => {
            handle_pause_polling(bot, msg, state, Subsystem::ProcessTracker).await?
        }
        "⏸️ Pause Screen Capture" => {
            handle_pause_polling(bot, msg, state, Subsystem::ScreenCapture).await?
        }
        "⏸️ Pause System Resources" => {
            handle_pause_polling(bot, msg, state, Subsystem::SystemResources).await?
        }
        "⏸️ Pause Systemd" => handle_pause_polling(bot, msg, state, Subsystem::Systemd).await?,
        "⏸️ Pause Docker Tracker" => {
            handle_pause_polling(bot, msg, state, Subsystem::DockerTracker).await?
        }
        // Per-subsystem resume
        "▶️ Resume Process Tracker" => {
            handle_resume_polling(bot, msg, state, Subsystem::ProcessTracker).await?
        }
        "▶️ Resume Screen Capture" => {
            handle_resume_polling(bot, msg, state, Subsystem::ScreenCapture).await?
        }
        "▶️ Resume System Resources" => {
            handle_resume_polling(bot, msg, state, Subsystem::SystemResources).await?
        }
        "▶️ Resume Systemd" => {
            handle_resume_polling(bot, msg, state, Subsystem::Systemd).await?
        }
        "▶️ Resume Docker Tracker" => {
            handle_resume_polling(bot, msg, state, Subsystem::DockerTracker).await?
        }
        // Per-subsystem set interval
        "🕐 Set Process Tracker Interval" => {
            handle_poll_interval_prompt(bot, msg, state, Subsystem::ProcessTracker).await?
        }
        "🕐 Set Screen Capture Interval" => {
            handle_poll_interval_prompt(bot, msg, state, Subsystem::ScreenCapture).await?
        }
        "🕐 Set System Resources Interval" => {
            handle_poll_interval_prompt(bot, msg, state, Subsystem::SystemResources).await?
        }
        "🕐 Set Systemd Interval" => {
            handle_poll_interval_prompt(bot, msg, state, Subsystem::Systemd).await?
        }
        "🕐 Set Docker Tracker Interval" => {
            handle_poll_interval_prompt(bot, msg, state, Subsystem::DockerTracker).await?
        }
        "🔑 Authenticate" => handle_auth_prompt(bot, msg, state).await?,
        "❌ Cancel" => {
            state.set_chat_state_idle(msg.chat.id);
            bot.send_message(msg.chat.id, "Cancelled")
                .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
                .await?;
        }
        "🔴 Stop" => handle_stop(bot, msg, cancel_token).await?,
        text => {
            bot.send_message(
                msg.chat.id,
                escape_mdv2(&format!(
                    "You said: \"{text}\"\n\nUse the buttons below or type /start."
                )),
            )
            .await?;
        }
    }
    Ok(())
}
