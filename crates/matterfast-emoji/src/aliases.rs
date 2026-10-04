/// Names Mattermost uses that gemoji indexes under a different shortcode.
///
/// Not exhaustive — it covers what actually turns up in reactions. Extend it
/// when something renders as `:name:` that should not.
pub(crate) const ALIASES: &[(&str, &str)] = &[
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
