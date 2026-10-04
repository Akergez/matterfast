use crate::aliases::ALIASES;
use crate::rendered::Rendered;
use crate::skin_tones::strip_skin_tone;

/// Resolves a Mattermost emoji name.
pub fn resolve(name: &str) -> Rendered {
    for candidate in [name, strip_skin_tone(name)] {
        if let Some(emoji) = emojis::get_by_shortcode(candidate) {
            return Rendered::Unicode(emoji.as_str());
        }
        if let Some((_, alias)) = ALIASES.iter().find(|(mm, _)| *mm == candidate) {
            if !alias.is_empty() {
                if let Some(emoji) = emojis::get_by_shortcode(alias) {
                    return Rendered::Unicode(emoji.as_str());
                }
            }
        }
        // Some clients send the emoji itself rather than a name.
        if let Some(emoji) = emojis::get(candidate) {
            return Rendered::Unicode(emoji.as_str());
        }
    }
    Rendered::Custom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::label::label;

    #[test]
    fn mattermost_only_names_resolve_through_the_alias_table() {
        // gemoji indexes this as `thinking`, Mattermost calls it
        // `thinking_face` — without the alias it would render as `:thinking_face:`.
        assert_eq!(label("thinking_face"), "🤔");
        assert_eq!(label("rolling_on_the_floor_laughing"), "🤣");
    }

    #[test]
    fn custom_emoji_keep_their_colon_form() {
        assert_eq!(label("shipit_squirrel_2024"), ":shipit_squirrel_2024:");
        assert_eq!(resolve("shipit_squirrel_2024"), Rendered::Custom);
    }
}
