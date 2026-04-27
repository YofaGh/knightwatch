use std::sync::Arc;
use teloxide::{
    dispatching,
    prelude::*,
    types::{
        ChatId, InputFile, InputMedia, InputMediaPhoto, KeyboardButton, KeyboardMarkup, ParseMode,
        ReplyMarkup,
    },
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::models::{Command, TelegramDisplay};
use crate::{
    prelude::*,
    process_tracker::{self, utils::snapshot_to_response},
    telegram_bot::utils::escape_mdv2,
};

pub fn init_bot(cancel_token: CancellationToken) -> Option<dispatching::ShutdownToken> {
    let config = get_config();
    if !config.args.telegram {
        return None;
    }
    let Some(token) = &config.persistent.telegram_token else {
        error!("No telegram token is provided");
        return None;
    };
    let bot = Bot::new(token);
    let (sender, receiver) = mpsc::channel(64);
    let sender = Arc::new(sender);
    let mut dispatcher = Dispatcher::builder(bot.clone(), schema())
        .dependencies(dptree::deps![cancel_token.clone(), sender])
        .build();
    let shutdown_token = dispatcher.shutdown_token();
    let bot_clone = bot.clone();
    let cancel_clone = cancel_token.clone();
    tokio::spawn(async move {
        loop {
            match bot_clone.get_me().await {
                Ok(_) => {
                    info!("Telegram connection established, starting dispatcher");
                    break;
                }
                Err(err) => {
                    warn!("Telegram unreachable, retrying in 10s: {err}");
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {}
                        _ = cancel_clone.cancelled() => {
                            info!("Cancelled while waiting for Telegram");
                            return;
                        }
                    }
                }
            }
        }
        dispatcher.dispatch().await;
    });
    tokio::spawn(async move { process_tracker_event_notifier(bot, receiver).await });
    info!("Telegram Bot started");
    Some(shutdown_token)
}

pub async fn process_tracker_event_notifier(
    bot: Bot,
    mut new_chat_id_receiver: mpsc::Receiver<ChatId>,
) {
    let Some(mut receiver) = crate::process_tracker::subscribe_events() else {
        return;
    };
    let mut chat_ids = vec![];
    loop {
        tokio::select! {
            Some(chat_id) = new_chat_id_receiver.recv() => {
                chat_ids.push(chat_id);
                info!("New chat id registered: {chat_id}");
            }
            event = receiver.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(err) => {
                        error!("Failed to receive process tracker event: {err}");
                        continue;
                    }
                };
                let message = super::utils::format_event(&event);
                let mut dead = vec![];
                for (i, &chat_id) in chat_ids.iter().enumerate() {
                    if let Err(err) = bot
                        .send_message(chat_id, &message)
                        .parse_mode(ParseMode::MarkdownV2)
                        .await
                    {
                        warn!("Failed to send event to chat {chat_id}: {err}");
                        dead.push(i);
                    }
                }
                for i in dead.into_iter().rev() {
                    chat_ids.swap_remove(i);
                }
            }
        }
    }
}

fn main_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("📊 Process"),
            KeyboardButton::new("📊 Top Processes"),
        ],
        vec![KeyboardButton::new("🖼️ Screenshot")],
        vec![
            KeyboardButton::new("📋 Help"),
            KeyboardButton::new("🔴 Stop"),
        ],
    ])
    .resize_keyboard()
}

fn top_processes_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("🔥 By CPU"),
            KeyboardButton::new("🧠 By Memory"),
        ],
        vec![KeyboardButton::new("❌ Cancel")],
    ])
    .resize_keyboard()
}

fn schema() -> dispatching::UpdateHandler<Error> {
    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(dptree::case![Command::Start].endpoint(handle_start))
        .branch(dptree::case![Command::Menu].endpoint(handle_start))
        .branch(dptree::case![Command::Help].endpoint(handle_help))
        .branch(dptree::case![Command::Screenshot].endpoint(handle_screenshot))
        .branch(dptree::case![Command::Process].endpoint(handle_process))
        .branch(dptree::case![Command::TopProcesses].endpoint(handle_top_processes_menu))
        .branch(dptree::case![Command::StopKnightWatch].endpoint(handle_stop));

    Update::filter_message()
        .branch(command_handler)
        .branch(dptree::endpoint(handle_plain_message))
}

async fn handle_start(
    bot: Bot,
    msg: Message,
    new_chat_id_sender: Arc<mpsc::Sender<ChatId>>,
) -> Result<()> {
    if let Err(err) = new_chat_id_sender.send(msg.chat.id).await {
        error!("Failed to send new chat id to notifier: {err}");
    }
    bot.send_message(
        msg.chat.id,
        "🤖 *Knight Watch BOT*\n\nChoose an action below:",
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
    .await?;
    Ok(())
}

async fn handle_help(bot: Bot, msg: Message) -> Result<()> {
    bot.send_message(
        msg.chat.id,
        "🚀 *This is a bot ran by Knight Watch:*\n\
         • Receive Screenshot of Monitors\n\
         • Get Process Info\n\
         • Stop the knight Watch",
    )
    .parse_mode(ParseMode::MarkdownV2)
    .await?;
    Ok(())
}

async fn handle_screenshot(bot: Bot, msg: Message) -> Result<()> {
    bot.send_message(msg.chat.id, "🖼️ Taking Screenshots...")
        .await?;
    let images = crate::screen_capture::screenshot_all_screens().unwrap_or_default();
    if images.is_empty() {
        bot.send_message(msg.chat.id, "🖼️ No Images were provided.")
            .await?;
        return Ok(());
    }
    for (chunk_idx, chunk) in images.chunks(10).enumerate() {
        if chunk.len() == 1 {
            let s = &chunk[0];
            bot.send_photo(msg.chat.id, InputFile::memory(s.image.clone()))
                .caption(format!(
                    "🖼️ {} | {}x{} | {}",
                    s.monitor_name, s.width, s.height, s.timestamp
                ))
                .await?;
        } else {
            let media: Vec<InputMedia> = chunk
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let mut photo = InputMediaPhoto::new(InputFile::memory(s.image.clone()));
                    if i == 0 {
                        photo = photo.caption(format!("🖼️ Screenshot — batch {}", chunk_idx + 1));
                    } else {
                        photo = photo.caption(format!(
                            "{} | {}x{} | {}",
                            s.monitor_name, s.width, s.height, s.timestamp
                        ));
                    }
                    InputMedia::Photo(photo)
                })
                .collect();
            bot.send_media_group(msg.chat.id, media).await?;
        }
    }
    Ok(())
}

async fn handle_process(bot: Bot, msg: Message) -> Result<()> {
    let (root_snap, children_snaps, work_done) = tokio::join!(
        process_tracker::get_root(),
        process_tracker::get_children(),
        process_tracker::is_work_done(),
    );

    let child_count = children_snaps.len();
    let process_tree_snapshot =
        super::models::TelegramDisplay(&process_tracker::structs::ProcessTree {
            root: root_snap.map(|s| snapshot_to_response(&s)),
            children: children_snaps
                .into_iter()
                .map(|s| snapshot_to_response(&s))
                .collect(),
            child_count,
            work_done,
            timestamp: crate::utils::now_rfc3339(),
        });
    bot.send_message(msg.chat.id, process_tree_snapshot.to_string())
        .await?;
    Ok(())
}

async fn handle_top_processes_menu(bot: Bot, msg: Message) -> Result<()> {
    bot.send_message(msg.chat.id, "📊 *Top Processes* — sort by:")
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(top_processes_keyboard()))
        .await?;
    Ok(())
}

async fn handle_top_processes_by(
    bot: Bot,
    msg: Message,
    by: process_tracker::enums::SortKey,
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
    let body = snapshots
        .iter()
        .map(|s| TelegramDisplay(&process_tracker::utils::snapshot_to_response(s)).to_string())
        .collect::<Vec<_>>()
        .join("\n\n");
    let header = format!("📊 Top Processes by {label}\n\n");
    bot.send_message(msg.chat.id, format!("{header}{body}"))
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
        .await?;
    Ok(())
}

async fn handle_stop(bot: Bot, msg: Message, cancel_token: CancellationToken) -> Result<()> {
    bot.send_message(msg.chat.id, "🛑 Stopping Knight Watch…")
        .await?;
    cancel_token.cancel();
    Ok(())
}

async fn handle_plain_message(
    bot: Bot,
    msg: Message,
    cancel_token: CancellationToken,
) -> Result<()> {
    match msg.text() {
        Some("📋 Help") => handle_help(bot, msg).await?,
        Some("🖼️ Screenshot") => handle_screenshot(bot, msg).await?,
        Some("📊 Process") => handle_process(bot, msg).await?,
        Some("📊 Top Processes") => handle_top_processes_menu(bot, msg).await?,
        Some("🔥 By CPU") => {
            handle_top_processes_by(bot, msg, process_tracker::enums::SortKey::Cpu).await?
        }
        Some("🧠 By Memory") => {
            handle_top_processes_by(bot, msg, process_tracker::enums::SortKey::Memory).await?
        }
        Some("❌ Cancel") => {
            bot.send_message(msg.chat.id, "Cancelled")
                .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
                .await?;
        }
        Some("🔴 Stop") => handle_stop(bot, msg, cancel_token).await?,
        Some(text) => {
            bot.send_message(
                msg.chat.id,
                escape_mdv2(&format!(
                    "You said: \"{text}\"\n\nUse the buttons below or type /start\\."
                )),
            )
            .await?;
        }
        None => {}
    }
    Ok(())
}
