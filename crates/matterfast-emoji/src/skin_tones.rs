/// Skin-tone modifiers Mattermost appends to a base name.
pub(crate) const SKIN_TONES: &[&str] = &[
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
pub(crate) fn strip_skin_tone(name: &str) -> &str {
    // Longest match wins. `_light_skin_tone` is a suffix of
    // `_medium_light_skin_tone`, so taking the first match would leave
    // `+1_medium` behind and resolve nothing.
    SKIN_TONES
        .iter()
        .filter_map(|tone| name.strip_suffix(*tone))
        .min_by_key(|base| base.len())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{label, resolve, Rendered};

    #[test]
    fn skin_tone_variants_fall_back_to_the_base_emoji() {
        for tone in SKIN_TONES {
            assert_eq!(label(&format!("+1{tone}")), "👍", "tone {tone}");
        }
        // A name that merely contains a tone-ish word must not be mangled.
        assert_eq!(resolve("skin_tone_chart"), Rendered::Custom);
    }
}
