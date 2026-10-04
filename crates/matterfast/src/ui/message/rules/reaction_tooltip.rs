use super::plural::plural;

/// "Anna, Bob and you reacted with :thumbsup:" — the web's tooltip wording
/// (`reaction_tooltip.tsx`'s `tooltipTitle`), collapsed the way it collapses:
/// everyone we can name, then a trailing count for the rest once there are
/// more reactors than we have names for.
pub(crate) fn reaction_tooltip(names: &[String], unresolved: usize, emoji_name: &str) -> String {
    let who = match unresolved {
        0 => match names {
            [] => String::new(),
            [only] => only.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        },
        n if names.is_empty() => format!("{n} {}", plural(n as i64, "user", "users")),
        n => format!(
            "{} and {n} other {}",
            names.join(", "),
            plural(n as i64, "user", "users")
        ),
    };
    format!("{who} reacted with :{emoji_name}:")
}

#[cfg(test)]
mod tests {
    use super::reaction_tooltip;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn one_named_reactor() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna"]), 0, "thumbsup"),
            "Anna reacted with :thumbsup:"
        );
    }

    #[test]
    fn two_named_reactors_get_an_and_not_a_comma() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna", "You"]), 0, "thumbsup"),
            "Anna and You reacted with :thumbsup:"
        );
    }

    #[test]
    fn three_or_more_are_comma_joined_before_the_last_and() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna", "Bob", "You"]), 0, "tada"),
            "Anna, Bob and You reacted with :tada:"
        );
    }

    #[test]
    fn reactors_with_no_loaded_profile_become_a_trailing_count() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna"]), 3, "fire"),
            "Anna and 3 other users reacted with :fire:"
        );
        // Singular agreement, and no named reactors at all.
        assert_eq!(
            reaction_tooltip(&[], 1, "fire"),
            "1 user reacted with :fire:"
        );
    }
}
