/// "Anna is typing…", for however many people are.
pub(crate) fn typing_text(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("{one} is typing…"),
        [one, two] => format!("{one} and {two} are typing…"),
        [one, two, ..] => format!("{one}, {two} and others are typing…"),
    }
}

/// The banner over a channel with a call in it.
pub(crate) fn call_banner_text(participants: usize) -> String {
    match participants {
        0 => "A call is starting".to_string(),
        1 => "1 person is in a call".to_string(),
        n => format!("{n} people are in a call"),
    }
}

/// The line under the channel name: its topic, and how many people are in it.
/// The count answers "who can see this" without opening the member list.
pub(crate) fn subtitle(header: &str, members: Option<i64>) -> String {
    let topic = header.lines().next().unwrap_or("").trim();
    match members {
        Some(n) if !topic.is_empty() => format!("{topic} · {n} members"),
        Some(n) => format!("{n} members"),
        None => topic.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_around_the_feed_read_naturally() {
        assert_eq!(typing_text(&[]), "");
        assert_eq!(typing_text(&["Anna".into()]), "Anna is typing…");
        assert_eq!(
            typing_text(&["Anna".into(), "Bob".into(), "Carol".into()]),
            "Anna, Bob and others are typing…"
        );
        assert_eq!(call_banner_text(0), "A call is starting");
        assert_eq!(call_banner_text(3), "3 people are in a call");
        assert_eq!(subtitle("Release talk\nsecond line", Some(4)), "Release talk · 4 members");
        assert_eq!(subtitle("", Some(4)), "4 members");
        assert_eq!(subtitle("Release talk", None), "Release talk");
    }
}
