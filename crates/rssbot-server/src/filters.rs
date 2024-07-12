use teloxide::prelude::*;

#[tracing::instrument]
pub fn private_message_only(update: Update) -> bool {
    update.chat().map_or(true, |chat| chat.is_private())
}

#[tracing::instrument]
pub fn channel_or_group(update: Update) -> bool {
    update.chat().map_or(true, |chat| chat.is_channel() || chat.is_group() || chat.is_supergroup())
}
