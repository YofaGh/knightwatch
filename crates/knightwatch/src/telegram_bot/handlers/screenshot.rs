use teloxide::{
    prelude::*,
    types::{InputFile, InputMedia, InputMediaPhoto},
};

pub async fn handle_screenshot(
    bot: Bot,
    msg: Message,
    state: super::super::models::State,
) -> crate::types::Result<()> {
    if !state.is_authorized(msg.chat.id) {
        return super::auth::send_auth_first_message(bot, msg.chat.id).await;
    }
    bot.send_message(msg.chat.id, "🖼️ Taking Screenshots...")
        .await?;
    let images = crate::screen_capture::get_screenshots().await;
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
