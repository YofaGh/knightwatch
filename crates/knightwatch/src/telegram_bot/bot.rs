use teloxide::prelude::*;

use super::{broadcast::*, handlers::*, models::*};
use crate::prelude::*;

pub fn init_bot(cancel_token: tokio_util::sync::CancellationToken) -> Option<TelegramBot> {
    let config = get_config();
    if !config.args.telegram {
        return None;
    }
    let Some(token) = &config.persistent.telegram_token else {
        error!("No telegram token is provided");
        return None;
    };
    let bot = Bot::new(token);
    let (sender, receiver) = tokio::sync::mpsc::channel(64);
    let sender = std::sync::Arc::new(sender);
    let state = State::new();
    let mut dispatcher = Dispatcher::builder(bot.clone(), schema())
        .dependencies(dptree::deps![cancel_token.clone(), sender, state.clone()])
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
    tokio::spawn(async move { event_notifier(bot, receiver, state, cancel_token).await });
    info!("Telegram Bot started");
    Some(TelegramBot { shutdown_token })
}

fn schema() -> teloxide::dispatching::UpdateHandler<Error> {
    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(dptree::case![Command::Start].endpoint(handle_start))
        .branch(dptree::case![Command::Menu].endpoint(handle_start))
        .branch(dptree::case![Command::Help].endpoint(handle_help))
        .branch(dptree::case![Command::Auth].endpoint(handle_auth_prompt))
        .branch(dptree::case![Command::Screenshot].endpoint(handle_screenshot))
        .branch(dptree::case![Command::SystemSnapshot].endpoint(handle_system_resources))
        .branch(dptree::case![Command::Process].endpoint(handle_process))
        .branch(dptree::case![Command::TopProcesses].endpoint(handle_top_processes_menu))
        .branch(dptree::case![Command::StopKnightWatch].endpoint(handle_stop));

    dptree::entry()
        .branch(
            Update::filter_message()
                .branch(command_handler)
                .branch(dptree::endpoint(handle_plain_message)),
        )
        .branch(Update::filter_callback_query().endpoint(handle_callback_query))
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline button callback handler
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_callback_query(bot: Bot, q: CallbackQuery, state: State) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;
    let chat_id = match q.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };
    if !state.is_authorized_to_commmand(chat_id) {
        return send_auth_first_message(bot, chat_id).await;
    }
    let data = match &q.data {
        Some(d) => d,
        None => return Ok(()),
    };
    if let Some(action) = ProcessCallbackAction::decode(data) {
        return handle_process_callback(bot, q, chat_id, action).await;
    }
    if let Some(action) = SystemResourcesCallbackAction::decode(data) {
        return handle_system_resources_callback(bot, chat_id, action).await;
    }
    if let Some(action) = DockerCallbackAction::decode(data) {
        return handle_docker_callback(bot, chat_id, action).await;
    }
    bot.send_message(chat_id, "❓ Unknown action\\.").await?;
    Ok(())
}
