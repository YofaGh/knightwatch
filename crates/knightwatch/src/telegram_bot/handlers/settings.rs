use teloxide::{
    prelude::*,
    types::{ParseMode, ReplyMarkup},
};

use super::super::{
    keyboards::{self, subsystem_polling_keyboard},
    models::{ChatState, State, Subsystem},
    utils::escape_mdv2,
};
use super::auth::send_auth_first_message;
use crate::{
    docker_tracker, prelude::*, process_tracker, screen_capture, system_resources, systemd,
};

pub async fn handle_settings_menu(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized_to_commmand(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    state.set_chat_state_idle(msg.chat.id);
    bot.send_message(msg.chat.id, "⚙️ *Settings*")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(keyboards::settings_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_polling_menu(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized_to_commmand(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    state.set_chat_state_idle(msg.chat.id);
    bot.send_message(msg.chat.id, "⏱️ *Polling* — choose a subsystem:")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(
            keyboards::polling_subsystem_keyboard(),
        ))
        .await?;
    Ok(())
}

pub async fn handle_subsystem_polling_menu(
    bot: Bot,
    msg: Message,
    state: State,
    subsystem: Subsystem,
) -> Result<()> {
    if !state.is_authorized_to_commmand(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    state.set_chat_state_idle(msg.chat.id);
    let label = subsystem.label();
    bot.send_message(
        msg.chat.id,
        format!("⏱️ *{label} Polling*", label = escape_mdv2(label)),
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(subsystem_polling_keyboard(
        &subsystem,
    )))
    .await?;
    Ok(())
}

pub async fn handle_pause_polling(
    bot: Bot,
    msg: Message,
    state: State,
    subsystem: Subsystem,
) -> Result<()> {
    if !state.is_authorized_to_commmand(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let reply = dispatch_poll_op(&subsystem, "⏸️", "paused", |s| match s {
        Subsystem::ProcessTracker => Box::pin(process_tracker::pause_poll()),
        Subsystem::ScreenCapture => Box::pin(screen_capture::pause_poll()),
        Subsystem::SystemResources => Box::pin(system_resources::pause_poll()),
        Subsystem::Systemd => Box::pin(systemd::pause_poll()),
        Subsystem::DockerTracker => Box::pin(docker_tracker::pause_poll()),
    })
    .await;
    bot.send_message(msg.chat.id, reply)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(subsystem_polling_keyboard(
            &subsystem,
        )))
        .await?;
    Ok(())
}

pub async fn handle_resume_polling(
    bot: Bot,
    msg: Message,
    state: State,
    subsystem: Subsystem,
) -> Result<()> {
    if !state.is_authorized_to_commmand(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let reply = dispatch_poll_op(&subsystem, "▶️", "resumed", |s| match s {
        Subsystem::ProcessTracker => Box::pin(process_tracker::resume_poll()),
        Subsystem::ScreenCapture => Box::pin(screen_capture::resume_poll()),
        Subsystem::SystemResources => Box::pin(system_resources::resume_poll()),
        Subsystem::Systemd => Box::pin(systemd::resume_poll()),
        Subsystem::DockerTracker => Box::pin(docker_tracker::resume_poll()),
    })
    .await;
    bot.send_message(msg.chat.id, reply)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(subsystem_polling_keyboard(
            &subsystem,
        )))
        .await?;
    Ok(())
}

/// Shared logic for pause/resume: calls the right subsystem fn, formats the reply.
async fn dispatch_poll_op<F>(subsystem: &Subsystem, icon: &str, verb: &str, op: F) -> String
where
    F: FnOnce(
        &Subsystem,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>,
{
    let label = escape_mdv2(subsystem.label());
    match op(subsystem).await {
        Ok(()) => format!("{icon} *{label}* polling {verb}\\."),
        Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
    }
}

pub async fn handle_poll_interval_prompt(
    bot: Bot,
    msg: Message,
    state: State,
    subsystem: Subsystem,
) -> Result<()> {
    if !state.is_authorized_to_commmand(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let label = subsystem.label();
    state.set_chat_state(msg.chat.id, ChatState::AwaitingPollInterval { subsystem });
    bot.send_message(
        msg.chat.id,
        format!(
            "🕐 Enter the new poll interval for *{}* in seconds \\(e\\.g\\. `5`\\):",
            escape_mdv2(label)
        ),
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(keyboards::cancel_keyboard()))
    .await?;
    Ok(())
}

pub async fn handle_poll_interval_input(
    bot: Bot,
    msg: Message,
    state: State,
    subsystem: Subsystem,
    input: String,
) -> Result<()> {
    state.set_chat_state_idle(msg.chat.id);
    let reply = match input.trim().parse::<u64>() {
        Ok(secs) if secs > 0 => {
            let interval = std::time::Duration::from_secs(secs);
            let label = escape_mdv2(subsystem.label());
            let result = match &subsystem {
                Subsystem::ProcessTracker => process_tracker::set_poll_interval(interval).await,
                Subsystem::ScreenCapture => screen_capture::set_poll_interval(interval).await,
                Subsystem::SystemResources => system_resources::set_poll_interval(interval).await,
                Subsystem::Systemd => systemd::set_poll_interval(interval).await,
                Subsystem::DockerTracker => docker_tracker::set_poll_interval(interval).await,
            };
            match result {
                Ok(()) => format!("✅ *{label}* poll interval set to `{secs}s`\\."),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            }
        }
        _ => "⚠️ Invalid input\\. Please enter a positive integer number of seconds\\.".to_string(),
    };
    bot.send_message(msg.chat.id, reply)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(subsystem_polling_keyboard(
            &subsystem,
        )))
        .await?;
    Ok(())
}
