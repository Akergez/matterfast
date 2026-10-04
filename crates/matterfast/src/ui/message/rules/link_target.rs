/// What a clicked link means: a person, a message on this server, or an
/// ordinary web page that the browser should have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A mention, by handle — that is all the text carries.
    Profile(String),
    /// A link to another message on this server: go there rather than to a
    /// browser, which would open the whole app again to show something
    /// already on screen.
    Permalink(String),
    Web(String),
}

pub fn link_target(url: &str) -> LinkTarget {
    if let Some(handle) = url.strip_prefix(crate::markdown::MENTION_SCHEME) {
        return LinkTarget::Profile(handle.to_string());
    }
    match permalink(url) {
        Some(post_id) => LinkTarget::Permalink(post_id),
        None => LinkTarget::Web(url.to_string()),
    }
}

/// The post id in a Mattermost permalink, if that is what this is.
///
/// The shape is `<site>/<team>/pl/<post id>`, and the team part is sometimes
/// `_redirect` — which is why the match is on the `/pl/` segment rather than
/// on the whole URL.
fn permalink(url: &str) -> Option<String> {
    let (_, tail) = url.split_once("/pl/")?;
    let id = tail.split(['/', '?', '#']).next()?;
    // Mattermost ids are 26 characters of lowercase alphanumerics; anything
    // else is a different site that happens to have /pl/ in its path.
    (id.len() == 26 && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::{link_target, permalink, LinkTarget};

    #[test]
    fn recognises_a_message_link() {
        assert_eq!(
            permalink("https://mm.example.com/team/pl/gedfji9g1pbjjehsngn9j17fzr").as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );
        // The double slash a reminder message produces, and _redirect.
        assert_eq!(
            permalink("https://mm.example.com//pl/gedfji9g1pbjjehsngn9j17fzr").as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );
        assert_eq!(
            permalink("https://mm.example.com/_redirect/pl/gedfji9g1pbjjehsngn9j17fzr?x=1")
                .as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );

        // Not ours.
        assert_eq!(permalink("https://example.com/pl/short"), None);
        assert_eq!(permalink("https://example.com/blog/post"), None);
    }

    #[test]
    fn a_link_is_a_person_a_message_or_a_web_page() {
        assert_eq!(
            link_target("mm-mention:anna"),
            LinkTarget::Profile("anna".into())
        );
        assert_eq!(
            link_target("https://mm.example.com/team/pl/gedfji9g1pbjjehsngn9j17fzr"),
            LinkTarget::Permalink("gedfji9g1pbjjehsngn9j17fzr".into())
        );
        assert_eq!(
            link_target("https://example.com/blog/post"),
            LinkTarget::Web("https://example.com/blog/post".into())
        );
    }
}
