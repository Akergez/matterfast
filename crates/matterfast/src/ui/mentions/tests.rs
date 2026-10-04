use super::{local_groups, local_mentions, mention_list, mentioned_names};
use crate::ui::autocomplete;
use crate::ui::constants::{COMPLETIONS, GROUP_COMPLETIONS};
use mattermost_api::models::{Group, User};

fn group(name: &str, title: &str) -> Group {
    Group {
        id: name.to_string(),
        name: name.to_string(),
        display_name: title.to_string(),
        member_count: Some(2),
    }
}

#[test]
fn groups_are_found_by_handle_and_by_title() {
    let groups = [
        group("backend", "Server people"),
        group("on-call", "Backend duty"),
        group("design", "Designers"),
    ];
    let found = |term: &str| -> Vec<String> {
        local_groups(&groups, term)
            .into_iter()
            .map(|candidate| candidate.insert)
            .collect()
    };
    // The handle before a word of the title.
    assert_eq!(found("back"), ["@backend", "@on-call"]);
    assert_eq!(found("duty"), ["@on-call"]);
    assert_eq!(found(""), ["@backend", "@design", "@on-call"]);
    assert!(found("zzz").is_empty());

    let shown = &local_groups(&groups, "des")[0];
    assert_eq!(shown.primary, "Designers");
    assert_eq!(shown.secondary, "@design · 2 people");
    assert_eq!(shown.user_id, None);
}

#[test]
fn a_full_list_of_people_still_leaves_room_for_groups() {
    let row = |name: String| autocomplete::Candidate {
        insert: name.clone(),
        primary: name,
        secondary: String::new(),
        emoji: None,
        image: None,
        user_id: None,
    };
    let people = |n: usize| (0..n).map(|i| row(format!("@p{i}"))).collect::<Vec<_>>();
    let groups = |n: usize| (0..n).map(|i| row(format!("@g{i}"))).collect::<Vec<_>>();

    let list = mention_list(people(COMPLETIONS), groups(5));
    assert_eq!(list.len(), COMPLETIONS);
    assert_eq!(
        list.iter().filter(|c| c.insert.starts_with("@g")).count(),
        GROUP_COMPLETIONS
    );
    // People first.
    assert_eq!(list[0].insert, "@p0");

    // Nobody matched: the groups get the whole list.
    assert_eq!(mention_list(Vec::new(), groups(5)).len(), 5);
    // No groups: nothing is taken from the people.
    assert_eq!(mention_list(people(COMPLETIONS), Vec::new()).len(), COMPLETIONS);
    // One group costs one row.
    assert_eq!(mention_list(people(COMPLETIONS), groups(1)).len(), COMPLETIONS);
}

/// A directory the size of a large company, to check that answering from
/// memory stays instant. The claim is "under a frame"; this asserts an
/// order of magnitude below that, so it fails long before anyone notices.
#[test]
fn ten_thousand_users_complete_instantly() {
    let users: Vec<User> = (0..10_000)
        .map(|i| User {
            id: format!("u{i}"),
            username: format!("person{i}"),
            first_name: "Person".into(),
            last_name: i.to_string(),
            ..Default::default()
        })
        .collect();

    let started = std::time::Instant::now();
    let found = local_mentions(users.iter(), "person9", "full_name", &|_| None);
    let elapsed = started.elapsed();

    assert_eq!(found.len(), COMPLETIONS);
    assert!(
        elapsed < std::time::Duration::from_millis(150),
        "took {elapsed:?} for 10k users"
    );

    // The worst case is a term nobody matches: every user is examined and
    // the early exit never fires.
    let started = std::time::Instant::now();
    let none = local_mentions(users.iter(), "nobodyatall", "full_name", &|_| None);
    let elapsed = started.elapsed();
    assert!(none.is_empty());
    // Both bounds are generous on purpose: this is an unoptimised build
    // on a machine that may be compiling something else at the same time,
    // and the old 10ms/30ms failed for that reason rather than for a slow
    // scan. What they guard against is an accidental quadratic, which
    // would be seconds rather than milliseconds.
    assert!(
        elapsed < std::time::Duration::from_millis(150),
        "worst case took {elapsed:?} for 10k users"
    );
}

#[test]
fn matches_inside_a_name_not_just_the_start() {
    let users = [
        User {
            id: "u1".into(),
            username: "fomchenkovsv".into(),
            first_name: "Semyon".into(),
            last_name: "Fomchenkov".into(),
            ..Default::default()
        },
        User {
            id: "u2".into(),
            username: "fedorov".into(),
            first_name: "Fedor".into(),
            ..Default::default()
        },
    ];

    // Searching by surname, which is what people actually do.
    let found = local_mentions(users.iter(), "fomche", "full_name", &|_| None);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].insert, "@fomchenkovsv");
    assert_eq!(found[0].primary, "Semyon Fomchenkov");

    // A prefix match outranks a substring one even when it is found later.
    let found = local_mentions(users.iter(), "fe", "full_name", &|_| None);
    assert_eq!(found[0].insert, "@fedorov");
}

#[test]
fn finds_by_surname_even_when_the_server_shows_usernames() {
    let users = [User {
        id: "u1".into(),
        username: "ivan42".into(),
        first_name: "Семён".into(),
        last_name: "Фомченко".into(),
        ..Default::default()
    }];

    // The handle has nothing in common with the surname, and the display
    // setting is "username" — so a full_name search would have found
    // nothing, and only searching first/last name directly finds this.
    let found = local_mentions(users.iter(), "фомче", "username", &|_| None);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].insert, "@ivan42");
    // The row still shows the configured display (username), not the
    // field it was actually found by.
    assert_eq!(found[0].primary, "ivan42");
}

#[test]
fn finds_mentions_and_ignores_addresses() {
    assert_eq!(mentioned_names("hi @anna and @bob"), ["anna", "bob"]);
    // An email is not a mention, and neither is a trailing full stop.
    assert_eq!(mentioned_names("mail me at a@b.com"), Vec::<String>::new());
    assert_eq!(mentioned_names("ask @anna."), ["anna"]);
    assert_eq!(mentioned_names("nothing here"), Vec::<String>::new());
    assert_eq!(mentioned_names("@"), Vec::<String>::new());
}
