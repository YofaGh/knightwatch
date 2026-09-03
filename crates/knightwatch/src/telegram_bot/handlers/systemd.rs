use teloxide::{
    prelude::*,
    types::{ParseMode, ReplyMarkup},
};

use super::super::{
    display::TelegramDisplay,
    keyboards::{cancel_keyboard, systemd_keyboard, unit_control_keyboard},
    models::{ChatState, State, SystemdCallbackAction},
    utils::escape_mdv2,
};
use crate::{prelude::*, systemd};

pub async fn handle_systemd_menu(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return super::send_auth_first_message(bot, msg.chat.id).await;
    }
    state.set_chat_state_idle(msg.chat.id);
    bot.send_message(msg.chat.id, "🔧 *Systemd* — choose an action:")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(systemd_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_systemd_overview(bot: Bot, msg: Message) -> Result<()> {
    let snapshot = systemd::get_snapshot().await;
    let message = snapshot.map_or_else(
        || escape_mdv2("⚠️ No systemd snapshot available."),
        |snap| TelegramDisplay(&snap).to_string(),
    );
    bot.send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(systemd_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_systemd_failed(bot: Bot, msg: Message) -> Result<()> {
    let units = systemd::get_failed_units().await;
    let message = if units.is_empty() {
        "✅ *No failed units*".to_string()
    } else {
        let body = units
            .iter()
            .map(|u| TelegramDisplay(u).to_string())
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("🔴 *Failed Units* \\({}\\)\n\n{body}", units.len())
    };
    bot.send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(systemd_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_systemd_unit_prompt(bot: Bot, msg: Message, state: State) -> Result<()> {
    state.set_chat_state(msg.chat.id, ChatState::AwaitingUnitName);
    bot.send_message(msg.chat.id, "🔍 Type the unit name (e.g. nginx.service):")
        .reply_markup(ReplyMarkup::Keyboard(cancel_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_systemd_callback(
    bot: Bot,
    chat_id: ChatId,
    action: SystemdCallbackAction,
    user: DisplayUser
) -> Result<()> {
    match action {
        SystemdCallbackAction::Control { unit_name, action } => {
            let action_str = escape_mdv2(action.as_str());
            let unit_str = escape_mdv2(&unit_name);
            let reply = match systemd::control_unit(user, unit_name, action).await {
                Ok(()) => format!("✅ `{unit_str}` — `{action_str}` sent\\."),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
    }
    Ok(())
}

pub async fn handle_systemd_unit_lookup(
    bot: Bot,
    msg: Message,
    state: State,
    unit_name: String,
) -> Result<()> {
    state.set_chat_state_idle(msg.chat.id);
    let unit = systemd::get_unit(unit_name.clone()).await;
    let found = unit.is_some();
    let message = unit.map_or_else(
        || format!("❓ Unit `{}` not found\\.", escape_mdv2(&unit_name)),
        |u| TelegramDisplay(&u).to_string(),
    );
    let req = bot
        .send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2);
    if found {
        req.reply_markup(unit_control_keyboard(&unit_name)).await?;
    } else {
        req.reply_markup(ReplyMarkup::Keyboard(systemd_keyboard()))
            .await?;
    }
    Ok(())
}
