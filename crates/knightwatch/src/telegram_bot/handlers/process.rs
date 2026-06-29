use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyMarkup},
};

use super::super::{
    display::TelegramDisplay,
    keyboards::{
        main_keyboard, top_process_keyboard, top_processes_keyboard, tracked_process_keyboard,
    },
    models::{ProcessCallbackAction, State},
    utils::escape_mdv2,
};
use super::auth::send_auth_first_message;
use crate::{prelude::*, process_tracker};

pub async fn handle_process(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    let process_trees = process_tracker::get_all_process_trees().await;
    if process_trees.is_empty() {
        bot.send_message(msg.chat.id, "📊 No tracked processes found.")
            .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
            .await?;
        return Ok(());
    }
    for process_tree in process_trees {
        bot.send_message(msg.chat.id, TelegramDisplay(&process_tree).to_string())
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(tracked_process_keyboard(process_tree.root_pid))
            .await?;
    }
    Ok(())
}

pub async fn handle_top_processes_menu(bot: Bot, msg: Message, state: State) -> Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return send_auth_first_message(bot, msg.chat.id).await;
    }
    bot.send_message(msg.chat.id, "📊 *Top Processes* — sort by:")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(top_processes_keyboard()))
        .await?;
    Ok(())
}

pub async fn handle_top_processes_by(
    bot: Bot,
    msg: Message,
    by: process_tracker::SortKey,
) -> Result<()> {
    let label = by.to_string();
    bot.send_message(
        msg.chat.id,
        format!("⏳ Fetching top processes by {label}…"),
    )
    .await?;
    let snapshots = process_tracker::get_top_processes(by, 0).await;
    if snapshots.is_empty() {
        bot.send_message(msg.chat.id, "No processes found.")
            .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
            .await?;
        return Ok(());
    }
    let header = format!("📊 *Top Processes by {label}*\n\n");
    bot.send_message(msg.chat.id, header)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
        .await?;
    for snapshot in &snapshots {
        let body = TelegramDisplay(snapshot).to_string();
        bot.send_message(msg.chat.id, body)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(top_process_keyboard(snapshot.pid))
            .await?;
    }
    Ok(())
}

pub async fn handle_process_callback(
    bot: Bot,
    q: CallbackQuery,
    chat_id: ChatId,
    action: ProcessCallbackAction,
) -> Result<()> {
    match action {
        ProcessCallbackAction::Track { pid } => {
            let reply = match process_tracker::track_pid(pid).await {
                Ok(()) => format!("✅ Now tracking PID `{pid}`\\."),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            if let Some(msg) = &q.message {
                let _ = bot
                    .edit_message_reply_markup(chat_id, msg.id())
                    .reply_markup(InlineKeyboardMarkup::new([[
                        InlineKeyboardButton::callback("✅ Tracking", "noop"),
                    ]]))
                    .await;
            }
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        ProcessCallbackAction::Untrack { pid } => {
            let reply = match process_tracker::untrack_pid(pid).await {
                Ok(()) => format!("✅ Untracked PID `{pid}`\\."),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            if let Some(msg) = &q.message {
                let _ = bot
                    .edit_message_reply_markup(chat_id, msg.id())
                    .reply_markup(InlineKeyboardMarkup::new([[
                        InlineKeyboardButton::callback("🚫 Untracked", "noop"),
                    ]]))
                    .await;
            }
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        ProcessCallbackAction::KillTree { pid } => {
            let reply = match process_tracker::kill_tree(pid).await {
                Ok(killed) => {
                    let pids = killed
                        .iter()
                        .map(|p| format!("`{p}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("🌲 Kill tree sent to PID `{pid}`\\.\nSignalled: {pids}")
                }
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
        ProcessCallbackAction::Signal { pid, signal } => {
            let reply = match process_tracker::kill_process(pid, signal).await {
                Ok(true) => format!("✅ Signal sent to PID `{pid}`\\."),
                Ok(false) => format!("⚠️ OS rejected signal for PID `{pid}`\\."),
                Err(e) => format!("❌ Failed: `{}`", escape_mdv2(&e.to_string())),
            };
            bot.send_message(chat_id, reply)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
        }
    }
    Ok(())
}
