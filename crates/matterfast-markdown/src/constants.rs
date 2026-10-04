/// The URL scheme mentions are linked with. Not a real scheme — it exists so
/// the link handler can tell "open this person" from "open this web page".
pub const MENTION_SCHEME: &str = "mm-mention:";

/// The scheme a custom emoji's picture is addressed by inside prepared
/// Markdown: `![:name:](mm-emoji:name)`. Nothing is fetched from it; the
/// message view recognises it and draws the picture from the image cache.
pub const EMOJI_SCHEME: &str = "mm-emoji:";
