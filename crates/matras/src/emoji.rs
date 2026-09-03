//! Turning Mattermost emoji shortcodes into something a person can read.
//!
//! Mattermost stores reactions as bare names — `tada`, `+1`, `eyes` — and it is
//! the client's job to render them. Most names come from `emoji-data` and match
//! the gemoji shortcodes the [`emojis`] crate indexes, but not all: Mattermost
//! kept the longer Unicode CLDR names for a handful of faces where gemoji uses
//! a shorter alias. Those are listed in [`ALIASES`].
//!
//! Anything still unresolved is a **custom** emoji the server hosts as an
//! image; there is no Unicode for it, so the caller falls back to the image or
//! to the literal `:name:`.

/// Names Mattermost uses that gemoji indexes under a different shortcode.
///
/// Not exhaustive — it covers what actually turns up in reactions. Extend it
/// when something renders as `:name:` that should not.
const ALIASES: &[(&str, &str)] = &[
    ("thinking_face", "thinking"),
    ("slightly_smiling_face", "slightly_smiling"),
    ("upside_down_face", "upside_down"),
    ("rolling_on_the_floor_laughing", "rofl"),
    ("face_with_rolling_eyes", "roll_eyes"),
    ("grinning_face_with_star_eyes", "star_struck"),
    ("shushing_face", "shushing"),
    ("face_with_hand_over_mouth", "hand_over_mouth"),
    ("exploding_head", "exploding_head"),
    ("hugging_face", "hugs"),
    ("nerd_face", "nerd"),
    ("clap", "clap"),
    ("raised_hands", "raised_hands"),
    ("man-shrugging", "man_shrugging"),
    ("woman-shrugging", "woman_shrugging"),
    ("mattermost", ""), // the brand emoji: custom, no Unicode
];

/// How a shortcode should be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    /// A Unicode emoji, ready to put in a label.
    Unicode(&'static str),
    /// Nothing standard matched — the server may host it as a custom image.
    Custom,
}

/// Skin-tone modifiers Mattermost appends to a base name.
const SKIN_TONES: &[&str] = &[
    "_light_skin_tone",
    "_medium_light_skin_tone",
    "_medium_skin_tone",
    "_medium_dark_skin_tone",
    "_dark_skin_tone",
];

/// Strips a skin-tone suffix, so `+1_light_skin_tone` falls back to `+1`.
///
/// The toned variants are real Unicode sequences, but gemoji does not index
/// them by these names, and the base emoji is a far better answer than
/// `:+1_light_skin_tone:`.
fn strip_skin_tone(name: &str) -> &str {
    // Longest match wins. `_light_skin_tone` is a suffix of
    // `_medium_light_skin_tone`, so taking the first match would leave
    // `+1_medium` behind and resolve nothing.
    SKIN_TONES
        .iter()
        .filter_map(|tone| name.strip_suffix(*tone))
        .min_by_key(|base| base.len())
        .unwrap_or(name)
}

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

/// What to show for a reaction chip: the emoji if we have one, else `:name:`
/// so the reader at least knows which reaction it was.
pub fn label(name: &str) -> String {
    match resolve(name) {
        Rendered::Unicode(e) => e.to_string(),
        Rendered::Custom => format!(":{name}:"),
    }
}

/// A short menu of reactions for the quick-react popover, in the order
/// Mattermost itself offers them.
pub const QUICK_REACTIONS: &[&str] = &[
    "+1",
    "-1",
    "smile",
    "tada",
    "eyes",
    "heart",
    "rocket",
    "white_check_mark",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_reactions_resolve_to_unicode() {
        assert_eq!(label("tada"), "🎉");
        assert_eq!(label("eyes"), "👀");
        assert_eq!(label("+1"), "👍");
        assert_eq!(label("-1"), "👎");
        assert_eq!(label("rocket"), "🚀");
    }

    #[test]
    fn mattermost_only_names_resolve_through_the_alias_table() {
        // gemoji indexes this as `thinking`, Mattermost calls it
        // `thinking_face` — without the alias it would render as `:thinking_face:`.
        assert_eq!(label("thinking_face"), "🤔");
        assert_eq!(label("rolling_on_the_floor_laughing"), "🤣");
    }

    #[test]
    fn skin_tone_variants_fall_back_to_the_base_emoji() {
        for tone in SKIN_TONES {
            assert_eq!(label(&format!("+1{tone}")), "👍", "tone {tone}");
        }
        // A name that merely contains a tone-ish word must not be mangled.
        assert_eq!(resolve("skin_tone_chart"), Rendered::Custom);
    }

    #[test]
    fn custom_emoji_keep_their_colon_form() {
        assert_eq!(label("shipit_squirrel_2024"), ":shipit_squirrel_2024:");
        assert_eq!(resolve("shipit_squirrel_2024"), Rendered::Custom);
    }

    #[test]
    fn every_quick_reaction_renders() {
        for name in QUICK_REACTIONS {
            assert!(
                matches!(resolve(name), Rendered::Unicode(_)),
                "{name} should have a Unicode rendering"
            );
        }
    }
}
