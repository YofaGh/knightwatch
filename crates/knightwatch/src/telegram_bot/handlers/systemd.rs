use teloxide::{
    prelude::*,
    types::{ParseMode, ReplyMarkup},
};

use super::super::{
    display::TelegramDisplay,
    keyboards::{cancel_keyboard, systemd_keyboard},
    models::{ChatState, State},
    utils::escape_mdv2,
};
use crate::{prelude::*, systemd};

pub async fn handle_systemd_menu(bot: Bot, msg: Message, state: State) -> Result<()> {
    state.set_chat_state_idle(msg.chat.id);
    bot.send_message(msg.chat.id, "🔧 *Systemd* — choose an action:")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(systemd_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_systemd_overview(bot: Bot, msg: Message) -> Result<()> {
    let snapshot = systemd::get_snapshot().await;
    let message = match snapshot {
        Some(snap) => TelegramDisplay(&snap).to_string(),
        None => escape_mdv2("⚠️ No systemd snapshot available."),
    };
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

pub async fn handle_systemd_unit_lookup(
    bot: Bot,
    msg: Message,
    state: State,
    unit_name: String,
) -> Result<()> {
    state.set_chat_state_idle(msg.chat.id);
    let unit = systemd::get_unit(unit_name.clone()).await;
    let message = match unit {
        Some(u) => TelegramDisplay(&u).to_string(),
        None => format!("❓ Unit `{}` not found\\.", escape_mdv2(&unit_name)),
    };
    bot.send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(systemd_keyboard()))
        .await?;
    Ok(())
}
