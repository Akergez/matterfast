use mattermost_api::models::Post;

use super::markdown_text::system_markdown;
use super::mention_sigils::with_mention_sigils;
use crate::state::AppState;

/// The channel/team activity types the webapp combines client-side when
/// several happen back to back (`combineUserActivityPosts` in
/// `post_list.ts`): joins, leaves, adds and removes. A header change, a
/// rename or anything else stays its own row — there is nothing to list for
/// those.
fn is_combinable_system(post: &Post) -> bool {
    matches!(
        post.r#type.as_str(),
        "system_join_channel"
            | "system_leave_channel"
            | "system_add_to_channel"
            | "system_remove_from_channel"
            | "system_join_team"
            | "system_leave_team"
            | "system_add_to_team"
            | "system_remove_from_team"
    )
}

/// "@anna", "@anna and @bob", or "@anna and 3 others" past the small
/// limit — the same threshold `combined_system_message`'s `LastUsers`
/// collapses at (two named, then a count).
fn mention_list(handles: &[String]) -> String {
    match handles {
        [] => String::new(),
        [a] => format!("@{a}"),
        [a, b] => format!("@{a} and @{b}"),
        [a, rest @ ..] => format!("@{a} and {} others", rest.len()),
    }
}

/// One combined line per (post type, actor) pair found in the run — "Anna
/// and 3 others joined the channel." — mirroring `CombinedSystemMessage`'s
/// `postTypeMessage` table, minus the "you"/expand-in-place wrinkles: this is
/// a chat row, not a redux-connected React tree.
fn system_sentence(post_type: &str, handles: &[String], actor: Option<&str>) -> String {
    let who = mention_list(handles);
    let were = if handles.len() == 1 { "was" } else { "were" };
    match post_type {
        "system_join_channel" => format!("{who} joined the channel."),
        "system_leave_channel" => format!("{who} left the channel."),
        "system_join_team" => format!("{who} joined the team."),
        "system_leave_team" => format!("{who} left the team."),
        "system_add_to_channel" => match actor {
            Some(a) => format!("{who} {were} added to the channel by @{a}."),
            None => format!("{who} {were} added to the channel."),
        },
        "system_add_to_team" => match actor {
            Some(a) => format!("{who} {were} added to the team by @{a}."),
            None => format!("{who} {were} added to the team."),
        },
        "system_remove_from_channel" => format!("{who} {were} removed from the channel."),
        "system_remove_from_team" => format!("{who} {were} removed from the team."),
        _ => who,
    }
}

/// The prop key naming who the event happened to, and — where the event has
/// one — who did it. `remove_*` messages name no actor in the webapp either
/// (`props.username` on those posts identifies the admin's session, not
/// someone worth crediting in the sentence).
fn system_subject_and_actor(post_type: &str) -> (&'static str, Option<&'static str>) {
    match post_type {
        "system_add_to_channel" | "system_add_to_team" => ("addedUsername", Some("username")),
        "system_remove_from_channel" | "system_remove_from_team" => ("removedUsername", None),
        _ => ("username", None),
    }
}

/// One line per run of combinable system posts, per distinct (type, actor)
/// pair within it — so "Anna joined" and "Bob was added by Carol" arriving
/// back to back become two lines in one block rather than one line each with
/// its own avatar-width margin.
fn combined_system_text(posts: &[&Post]) -> String {
    let mut groups: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
    for post in posts {
        let (subject_key, actor_key) = system_subject_and_actor(&post.r#type);
        let Some(handle) = post
            .props
            .get(subject_key)
            .and_then(|v| v.as_str())
            .filter(|h| !h.is_empty())
        else {
            continue;
        };
        let actor = actor_key
            .and_then(|k| post.props.get(k))
            .and_then(|v| v.as_str())
            .filter(|a| !a.is_empty())
            .map(str::to_string);

        match groups
            .iter_mut()
            .find(|(t, a, _)| *t == post.r#type && *a == actor)
        {
            Some((_, _, handles)) => {
                if !handles.iter().any(|h| h == handle) {
                    handles.push(handle.to_string());
                }
            }
            None => groups.push((post.r#type.clone(), actor, vec![handle.to_string()])),
        }
    }

    groups
        .into_iter()
        .map(|(post_type, actor, handles)| system_sentence(&post_type, &handles, actor.as_deref()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The lines a run of consecutive system posts is drawn as: the combinable
/// ones (joins, leaves, adds, removes) collapse into one line per run,
/// everything else keeps its own. `posts` is expected to hold no more than one
/// such run interleaved with non-combinable posts — exactly what the feed
/// buffers between the messages that are not system posts at all.
pub fn system_lines(posts: &[Post], st: &AppState) -> Vec<String> {
    let mut lines = Vec::new();
    let mut run: Vec<&Post> = Vec::new();
    for post in posts {
        if is_combinable_system(post) {
            run.push(post);
            continue;
        }
        if !run.is_empty() {
            lines.push(system_markdown(&combined_system_text(&run), st));
            run.clear();
        }
        lines.push(system_markdown(&with_mention_sigils(post), st));
    }
    if !run.is_empty() {
        lines.push(system_markdown(&combined_system_text(&run), st));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::message::rules::test_support::{post_with, state_with};

    #[test]
    fn a_run_of_system_posts_becomes_one_line_per_kind() {
        let st = state_with("anna", "Anna", "Petrova");
        let posts = vec![
            post_with("system_join_channel", "", &[("username", "anna")]),
            post_with("system_join_channel", "", &[("username", "bob")]),
            post_with(
                "system_header_change",
                "anna changed the header",
                &[("username", "anna")],
            ),
        ];
        assert_eq!(
            system_lines(&posts, &st),
            vec![
                // Known people by name, unknown ones as written.
                "[Anna Petrova](mm-mention:anna) and @bob joined the channel.".to_string(),
                "[Anna Petrova](mm-mention:anna) changed the header".to_string(),
            ]
        );
    }

    #[test]
    fn combinable_types_are_exactly_the_ones_the_webapp_merges() {
        for combinable in [
            "system_join_channel",
            "system_leave_channel",
            "system_add_to_channel",
            "system_remove_from_channel",
            "system_join_team",
            "system_leave_team",
            "system_add_to_team",
            "system_remove_from_team",
        ] {
            assert!(is_combinable_system(&post_with(combinable, "", &[])));
        }
        // A header change has nobody to list, so it stays its own row.
        assert!(!is_combinable_system(&post_with(
            "system_header_change",
            "",
            &[]
        )));
    }

    #[test]
    fn mention_list_collapses_past_two_names() {
        let names = |n: usize| (0..n).map(|i| format!("u{i}")).collect::<Vec<_>>();
        assert_eq!(mention_list(&names(1)), "@u0");
        assert_eq!(mention_list(&names(2)), "@u0 and @u1");
        assert_eq!(mention_list(&names(3)), "@u0 and 2 others");
        assert_eq!(mention_list(&names(5)), "@u0 and 4 others");
    }

    #[test]
    fn system_sentence_agrees_singular_and_plural() {
        assert_eq!(
            system_sentence("system_join_channel", &["a".into()], None),
            "@a joined the channel."
        );
        assert_eq!(
            system_sentence(
                "system_add_to_channel",
                &["a".into(), "b".into()],
                Some("admin")
            ),
            "@a and @b were added to the channel by @admin."
        );
        assert_eq!(
            system_sentence("system_remove_from_team", &["a".into()], None),
            "@a was removed from the team."
        );
    }

    #[test]
    fn a_run_combines_into_one_line_per_type_and_actor() {
        // Two joins and an add-by-someone-else, back to back — three system
        // posts, but only the joins belong on the same line.
        let joined_a = post_with("system_join_channel", "", &[("username", "anna")]);
        let joined_b = post_with("system_join_channel", "", &[("username", "bob")]);
        let added = post_with(
            "system_add_to_channel",
            "",
            &[("addedUsername", "carol"), ("username", "dave")],
        );
        let run = [&joined_a, &joined_b, &added];
        assert_eq!(
            combined_system_text(&run),
            "@anna and @bob joined the channel.\n@carol was added to the channel by @dave."
        );
    }

    #[test]
    fn the_same_person_named_twice_in_a_run_is_not_duplicated() {
        let a = post_with("system_join_channel", "", &[("username", "anna")]);
        let b = post_with("system_join_channel", "", &[("username", "anna")]);
        let run = [&a, &b];
        assert_eq!(combined_system_text(&run), "@anna joined the channel.");
    }
}
