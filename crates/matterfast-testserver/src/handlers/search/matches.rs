use serde_json::Value;

use super::search_terms::SearchTerms;
use crate::model::{channel, username};

impl SearchTerms {
    /// Whether a post is a hit. `offset` is the asker's time zone, in
    /// seconds east of UTC: a day is theirs, not Greenwich's.
    pub(super) fn matches(&self, post: &Value, any_word: bool, offset: i64) -> bool {
        let message = post["message"].as_str().unwrap_or_default().to_lowercase();
        let has = |word: &String| message.contains(word.as_str());
        let words = if any_word {
            self.words.is_empty() || self.words.iter().any(has)
        } else {
            self.words.iter().all(has)
        };
        if !words || self.excluded.iter().any(has) {
            return false;
        }
        let author = username(post["user_id"].as_str().unwrap_or_default());
        if !self.from.is_empty() && !self.from.iter().any(|name| name == author) {
            return false;
        }
        if !self.channels.is_empty() {
            let found = channel(post["channel_id"].as_str().unwrap_or_default());
            // A direct message is named by the other person, as `@handle`.
            let name = if found["type"] == "D" {
                "@lena".to_string()
            } else {
                found["name"].as_str().unwrap_or_default().to_string()
            };
            if !self.channels.iter().any(|wanted| *wanted == name) {
                return false;
            }
        }
        let day = (post["create_at"].as_i64().unwrap_or(0) / 1000 + offset).div_euclid(86_400);
        self.before.is_none_or(|before| day < before)
            && self.after.is_none_or(|after| day > after)
            && self.on.is_none_or(|on| day == on)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::parse_search::parse_search;
    use crate::constants::{DEV, LENA};

    #[test]
    fn a_post_is_a_hit_when_every_part_of_the_search_agrees() {
        let noon = 20_729_i64 * 86_400_000 + 12 * 3_600_000;
        let post = json!({"message": "Release notes draft is up", "user_id": LENA,
                          "channel_id": DEV, "create_at": noon});
        let hit = |terms: &str| parse_search(terms).matches(&post, false, 0);
        assert!(hit("release notes"));
        assert!(!hit("release tomorrow"));
        assert!(parse_search("release tomorrow").matches(&post, true, 0));
        assert!(hit("from:lena in:development"));
        assert!(!hit("from:mikk"));
        assert!(!hit("in:general"));
        assert!(!hit("notes -draft"));
        assert!(hit("on:2026-10-03"));
        assert!(hit("after:2026-10-02 before:2026-10-04"));
        assert!(!hit("before:2026-10-03"));
        // Half a day east of Greenwich, noon is already tomorrow.
        assert!(parse_search("on:2026-10-04").matches(&post, false, 13 * 3600));
    }
}
