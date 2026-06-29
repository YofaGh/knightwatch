use teloxide::prelude::*;
use tokio::sync::mpsc;

use super::models::{AuthState, State};
use crate::{prelude::*, utils::recv_or_pending};

pub async fn event_notifier(
    bot: Bot,
    mut chat_state_rx: mpsc::Receiver<(ChatId, AuthState)>,
    mut state: State,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let mut process_tracker_rx = crate::process_tracker::subscribe_events();
    let mut system_resources_rx = crate::system_resources::subscribe_events();
    let mut systemd_rx = crate::systemd::subscribe_events();
    let mut docker_tracker_rx = crate::docker_tracker::subscribe_events();
    if crate::all_none!(
        process_tracker_rx,
        system_resources_rx,
        systemd_rx,
        docker_tracker_rx
    ) {
        return;
    }
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Cancelled while waiting for events");
                return;
            }
            Some((chat_id, auth_state)) = chat_state_rx.recv() => {
                state.set_chat_auth(chat_id, auth_state);
                info!("Chat id registered or authenticated: {chat_id}");
            }
            event = recv_or_pending(&mut process_tracker_rx, "telegram: process tracker") => {
                let message = super::utils::format_process_tracker_event(&event);
                broadcast_message(&bot, &mut state, &message).await;
            }
            event = recv_or_pending(&mut system_resources_rx, "telegram: system resources") => {
                let message = super::utils::format_system_resources_event(&event);
                if let Some(msg) = message {
                    broadcast_message(&bot, &mut state, &msg).await;
                }
            }
            event = recv_or_pending(&mut systemd_rx, "telegram: systemd") => {
                let message = super::utils::format_systemd_event(&event);
                if let Some(msg) = message {
                    broadcast_message(&bot, &mut state, &msg).await;
                }
            }
            event = recv_or_pending(&mut docker_tracker_rx, "telegram: docker tracker") => {
                let message = super::utils::format_docker_tracker_event(&event);
                broadcast_message(&bot, &mut state, &message).await;
            }
        }
    }
}

async fn broadcast_message(bot: &Bot, state: &mut State, message: &str) {
    let mut dead = vec![];
    let chat_ids = state.get_relevant_chat_ids();
    for (i, &chat_id) in chat_ids.iter().enumerate() {
        if let Err(err) = bot
            .send_message(chat_id, message)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await
        {
            warn!("Failed to send event to chat {chat_id}: {err}");
            dead.push(i);
        }
    }
    for i in dead.iter().rev() {
        state.remove_chat(chat_ids[*i]);
    }
}

pub async fn send_chat_state(tx: &mpsc::Sender<(ChatId, AuthState)>, value: (ChatId, AuthState)) {
    if tx.is_closed() {
        return;
    }
    if let Err(err) = tx.send(value).await {
        warn!("Chat state channel closed unexpectedly: {err}");
    }
}
