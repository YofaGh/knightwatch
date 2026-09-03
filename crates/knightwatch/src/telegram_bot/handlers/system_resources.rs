use std::fmt::Write as _;
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
    let mut message = system_snapshot.map_or_else(
        || "*No System Snapshot found*".to_string(),
        |snap| TelegramDisplay(&snap).to_string(),
    );
    let thresholds = get_thresholds().await;
    let mask = get_refresh_mask().await;
    let _ = write!(message, "\n\n⚠️ *Thresholds*\n{thresholds}");
    let _ = write!(message, "\n🔄 *Refresh Mask*: `{mask}`");
    bot.send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(system_resources_keyboard())
        .await?;
    Ok(())
}

pub async fn handle_system_resources_alarms(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let alarms_snapshot = system_resources::get_alarms().await;
    let message = alarms_snapshot.map_or_else(
        || "*No Alarms Snapshot found*".to_string(),
        |snap| TelegramDisplay(&snap).to_string(),
    );
    bot.send_message(msg.chat.id, message)
        .parse_mode(ParseMode::MarkdownV2)
        .await?;
    Ok(())
}

pub async fn handle_system_resources_callback(
    bot: Bot,
    chat_id: ChatId,
    action: SystemResourcesCallbackAction,
    user: DisplayUser,
) -> Result<()> {
    match action {
        SystemResourcesCallbackAction::SetThresholds(thresholds) => {
            let reply = match system_resources::set_thresholds(user, thresholds).await {
                Ok(()) => "✅ Alert thresholds updated\\.".to_string(),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        SystemResourcesCallbackAction::SetRefreshMask(mask) => {
            let reply = match system_resources::set_refresh_mask(user, mask).await {
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

async fn get_thresholds() -> String {
    let thresholds = system_resources::get_thresholds().await;
    let Some(t) = thresholds else {
        return "└ `unavailable`".to_string();
    };
    format!(
        "├ CPU: `{cpu:.1}%`\n├ Mem: `{mem:.1}%`\n├ Disk: `{disk:.1}%`\n└ Battery: `{batt:.1}%`",
        cpu = t.cpu_warn,
        mem = t.memory_warn,
        disk = t.disk_warn,
        batt = t.battery_low,
    )
}

async fn get_refresh_mask() -> String {
    let mask = system_resources::get_refresh_mask().await;
    let Some(mask) = mask else {
        return "unavailable".to_string();
    };
    mask.to_string()
}
