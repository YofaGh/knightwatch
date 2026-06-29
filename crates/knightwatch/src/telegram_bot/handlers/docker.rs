use teloxide::{
    prelude::*,
    types::{ParseMode, ReplyMarkup},
};

use super::super::{
    display::TelegramDisplay,
    keyboards::{docker_container_keyboard, docker_keyboard},
    models::{DockerCallbackAction, State},
    utils::escape_mdv2,
};
use super::auth::send_auth_first_message;
use crate::{docker_tracker, prelude::*};

pub async fn handle_docker_menu(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    state.set_chat_state_idle(msg.chat.id);
    bot.send_message(msg.chat.id, "🐳 *Docker* — choose an action:")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(docker_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_docker_list(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let containers = docker_tracker::list_containers().await;
    if containers.is_empty() {
        bot.send_message(msg.chat.id, "🐳 No containers found\\.")
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(ReplyMarkup::Keyboard(docker_keyboard()))
            .await?;
        return Ok(());
    }
    bot.send_message(
        msg.chat.id,
        format!("🐳 *All Containers* \\({}\\)", containers.len()),
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(docker_keyboard()))
    .await?;
    for container in &containers {
        let body = TelegramDisplay(container).to_string();
        bot.send_message(msg.chat.id, body)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(docker_container_keyboard(&container.short_id))
            .await?;
    }
    Ok(())
}

pub async fn handle_docker_top(
    bot: Bot,
    msg: Message,
    state: State,
    by: docker_tracker::DockerSortKey,
) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let label = by.to_string();
    bot.send_message(
        msg.chat.id,
        format!("⏳ Fetching top containers by {label}…"),
    )
    .await?;
    let containers = docker_tracker::get_top_containers(by, 0).await;
    if containers.is_empty() {
        bot.send_message(msg.chat.id, "🐳 No containers found\\.")
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(ReplyMarkup::Keyboard(docker_keyboard()))
            .await?;
        return Ok(());
    }
    bot.send_message(
        msg.chat.id,
        format!("🐳 *Top Containers by {}*", escape_mdv2(&label)),
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(docker_keyboard()))
    .await?;
    for container in &containers {
        let body = TelegramDisplay(container).to_string();
        bot.send_message(msg.chat.id, body)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(docker_container_keyboard(&container.short_id))
            .await?;
    }
    Ok(())
}

pub async fn handle_docker_callback(
    bot: Bot,
    chat_id: ChatId,
    action: DockerCallbackAction,
) -> Result<()> {
    match action {
        DockerCallbackAction::Stop { ref id } => {
            let reply = match docker_tracker::stop_container(id.clone(), None).await {
                Ok(()) => format!("✅ Stop signal sent to `{}`\\.", escape_mdv2(id)),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        DockerCallbackAction::Start { ref id } => {
            let reply = match docker_tracker::start_container(id.clone()).await {
                Ok(()) => format!("✅ Start signal sent to `{}`\\.", escape_mdv2(id)),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        DockerCallbackAction::Kill { ref id } => {
            let reply = match docker_tracker::kill_container(id.clone(), None).await {
                Ok(()) => format!("✅ Kill signal sent to `{}`\\.", escape_mdv2(id)),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        DockerCallbackAction::Restart { ref id } => {
            let reply = match docker_tracker::restart_container(id.clone(), None).await {
                Ok(()) => format!("✅ Restart signal sent to `{}`\\.", escape_mdv2(id)),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        DockerCallbackAction::Pause { ref id } => {
            let reply = match docker_tracker::pause_container(id.clone()).await {
                Ok(()) => format!("⏸️ Paused container `{}`\\.", escape_mdv2(id)),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        DockerCallbackAction::Unpause { ref id } => {
            let reply = match docker_tracker::unpause_container(id.clone()).await {
                Ok(()) => format!("▶️ Unpaused container `{}`\\.", escape_mdv2(id)),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
    }
    Ok(())
}
