use super::day_number::day_number;
use super::search_terms::SearchTerms;

pub(super) fn parse_search(terms: &str) -> SearchTerms {
    let mut parsed = SearchTerms::default();
    for token in terms.to_lowercase().split_whitespace() {
        let token = token.trim_matches('"');
        if let Some(name) = token.strip_prefix("from:") {
            parsed.from.push(name.trim_start_matches('@').to_string());
        } else if let Some(name) = token.strip_prefix("in:") {
            parsed.channels.push(name.trim_start_matches('~').to_string());
        } else if let Some(day) = token.strip_prefix("before:") {
            parsed.before = day_number(day);
        } else if let Some(day) = token.strip_prefix("after:") {
            parsed.after = day_number(day);
        } else if let Some(day) = token.strip_prefix("on:") {
            parsed.on = day_number(day);
        } else if let Some(word) = token.strip_prefix('-').filter(|word| !word.is_empty()) {
            parsed.excluded.push(word.to_string());
        } else if !token.trim_start_matches('@').is_empty() {
            parsed.words.push(token.trim_start_matches('@').to_string());
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_is_words_and_modifiers() {
        let terms = parse_search("Release -draft from:@Lena in:development after:2026-10-01");
        assert_eq!(terms.words, ["release"]);
        assert_eq!(terms.excluded, ["draft"]);
        assert_eq!(terms.from, ["lena"]);
        assert_eq!(terms.channels, ["development"]);
        assert_eq!(terms.after, Some(20_727));
    }
}
