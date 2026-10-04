use mattermost_api::models::{Channel, ChannelType};

use super::kit::Lucide;

/// The icon for a channel in a flat list, where there is no "#" column.
pub(crate) fn channel_icon(channel: &Channel) -> Lucide {
    match channel.r#type {
        ChannelType::Open => Lucide::Hash,
        ChannelType::Private => Lucide::Lock,
        ChannelType::Direct => Lucide::User,
        _ => Lucide::Users,
    }
}
