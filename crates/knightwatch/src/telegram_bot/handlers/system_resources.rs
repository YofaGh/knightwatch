use teloxide::{prelude::*, types::ParseMode};

use super::super::{
    display::TelegramDisplay,
    keyboards::system_resources_keyboard,
    models::{State, SystemResourcesCallbackAction},
    utils::escape_mdv2,
};
use super::auth::send_auth_first_message;
use crate::{prelude::*, system_resources};

pub async fn handle_system_resources(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let system_snapshot = system_resources::get_snapshot().await;
    let message = match system_snapshot {
        Some(snap) => TelegramDisplay(&snap).to_string(),
        None => "*No System Snapshot found*".to_string(),
    };
    bot.send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(system_resources_keyboard())
        .await?;
    Ok(())
}

pub async fn handle_system_resources_callback(
    bot: Bot,
    chat_id: ChatId,
    action: SystemResourcesCallbackAction,
) -> Result<()> {
    match action {
        SystemResourcesCallbackAction::SetThresholds(thresholds) => {
            let reply = match system_resources::set_thresholds(thresholds).await {
                Ok(()) => "✅ Alert thresholds updated\\.".to_string(),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        SystemResourcesCallbackAction::SetRefreshMask(mask) => {
            let reply = match system_resources::set_refresh_mask(mask).await {
                Ok(()) => "✅ Refresh mask updated\\.".to_string(),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
    }
    Ok(())
}
