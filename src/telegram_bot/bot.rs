use teloxide::{
    dispatching,
    prelude::*,
    types::{
        InputFile, InputMedia, InputMediaPhoto, KeyboardButton, KeyboardMarkup, ParseMode,
        ReplyMarkup,
    },
};

use super::models::Command;
use crate::{
    prelude::*,
    process_tracker::{self, utils::snapshot_to_response},
};

pub fn init_bot() -> Option<dispatching::ShutdownToken> {
    let config = get_config();
    if !config.args.telegram {
        return None;
    }
    let Some(token) = &config.persistent.telegram_token else {
        tracing::error!("No telegram token is provided");
        return None;
    };
    let bot = Bot::new(token);
    let mut dispatcher = Dispatcher::builder(bot, schema()).build();
    let shutdown_token = dispatcher.shutdown_token();
    tokio::spawn(async move { dispatcher.dispatch().await });
    Some(shutdown_token)
}

fn main_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("🖼️ Screenshot"),
            KeyboardButton::new("📊 Process"),
        ],
        vec![
            KeyboardButton::new("📋 Help"),
            KeyboardButton::new("🔴 Stop"),
        ],
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
        .branch(dptree::case![Command::StopKnightWatch].endpoint(handle_stop));

    Update::filter_message()
        .branch(command_handler)
        .branch(dptree::endpoint(handle_plain_message))
}

async fn handle_start(bot: Bot, msg: Message) -> Result<()> {
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
    let images = crate::core::screenshot_all_screens().unwrap_or_default();
    if images.is_empty() {
        bot.send_message(msg.chat.id, "🖼️ No Images were provided.")
            .await?;
        return Ok(());
    }
    for (chunk_idx, chunk) in images.chunks(10).enumerate() {
        if chunk.len() == 1 {
            bot.send_photo(msg.chat.id, InputFile::memory(chunk[0].clone()))
                .caption(format!("🖼️ Image {}", chunk_idx * 10 + 1))
                .await?;
        } else {
            let media: Vec<InputMedia> = chunk
                .iter()
                .enumerate()
                .map(|(i, img)| {
                    let mut photo = InputMediaPhoto::new(InputFile::memory(img.clone()));
                    if i == 0 {
                        photo = photo.caption(format!("🖼️ Screenshot — batch {}", chunk_idx + 1));
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
            root: root_snap.map(snapshot_to_response),
            children: children_snaps
                .into_iter()
                .map(snapshot_to_response)
                .collect(),
            child_count,
            work_done,
            timestamp: crate::core::utils::now_rfc3339(),
        });
    bot.send_message(msg.chat.id, process_tree_snapshot.to_string())
        .await?;
    Ok(())
}

async fn handle_stop(bot: Bot, msg: Message) -> Result<()> {
    bot.send_message(msg.chat.id, "🛑 Stopping Knight Watch…")
        .await?;
    // TODO: send event to main for app shutdown instead of exiting directly
    std::process::exit(0);
}

async fn handle_plain_message(bot: Bot, msg: Message) -> Result<()> {
    match msg.text() {
        Some("📋 Help") => handle_help(bot, msg).await?,
        Some("🖼️ Screenshot") => handle_screenshot(bot, msg).await?,
        Some("📊 Process") => handle_process(bot, msg).await?,
        Some("🔴 Stop") => handle_stop(bot, msg).await?,
        Some(text) => {
            bot.send_message(
                msg.chat.id,
                format!("You said: \"{text}\"\n\nUse the buttons below or type /start\\."),
            )
            .await?;
        }
        None => {}
    }
    Ok(())
}
