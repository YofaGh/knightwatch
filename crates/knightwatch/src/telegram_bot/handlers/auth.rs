use teloxide::{
    prelude::*,
    types::{ParseMode, ReplyMarkup},
};

use super::super::{
    broadcast::send_chat_state,
    keyboards::{cancel_keyboard, main_keyboard},
    models::{AuthState, ChatState, State},
};
use crate::prelude::*;

pub async fn handle_auth_prompt(bot: Bot, msg: Message, state: State) -> Result<()> {
    state.set_chat_state(msg.chat.id, ChatState::AwaitingAuthToken);
    bot.send_message(
        msg.chat.id,
        "🔑 Please enter your authentication token \\(6–8 digits\\):",
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(ReplyMarkup::Keyboard(cancel_keyboard()))
    .await?;
    Ok(())
}

pub async fn handle_auth_token(
    bot: Bot,
    msg: Message,
    chat_state_tx: std::sync::Arc<tokio::sync::mpsc::Sender<(ChatId, AuthState)>>,
    state: State,
    token_input: String,
) -> Result<()> {
    let is_valid_format = token_input.len() >= 6
        && token_input.len() <= 8
        && token_input.chars().all(|c| c.is_ascii_digit());
    if !is_valid_format {
        bot.send_message(
            msg.chat.id,
            "⚠️ Invalid token\\. Please enter a numeric token between 6 and 8 digits:",
        )
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(ReplyMarkup::Keyboard(cancel_keyboard()))
        .await?;
        return Ok(());
    }
    if crate::config::set_user_chat_id(token_input, msg.chat.id.0)? {
        state.set_chat_state_idle(msg.chat.id);
        state.set_chat_auth(msg.chat.id, AuthState::Authenticated);
        state.set_chat_state_idle(msg.chat.id);
        send_chat_state(&chat_state_tx, (msg.chat.id, AuthState::Authenticated)).await;
        bot.send_message(msg.chat.id, "✅ You are now *authenticated*\\!")
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
            .await?;
    } else {
        state.set_chat_state_idle(msg.chat.id);
        bot.send_message(msg.chat.id, "❌ Authentication failed\\. Invalid token\\.")
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(ReplyMarkup::Keyboard(main_keyboard()))
            .await?;
    }
    Ok(())
}

pub async fn send_auth_first_message(bot: Bot, chat_id: ChatId) -> Result<()> {
    bot.send_message(
        chat_id,
        "🔒 Please authenticate first\\. Use /auth or the 🔑 Authenticate button\\.",
    )
    .parse_mode(ParseMode::MarkdownV2)
    .await?;
    Ok(())
}
