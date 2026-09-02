//! The dialogs that ask a question and hand the answer back.
//!
//! Nothing here talks to the server or to [`crate::state`]: each function takes
//! a callback and calls it with what the user chose. That keeps the API calls
//! in [`super`], where the client and the action loop already live, and it
//! means these can be read (and moved) without tracing a request through them.
//!
//! libadwaita is pinned to 1.5, so the dialogs are [`adw::MessageDialog`] —
//! `AlertDialog`, which replaces it, is 1.6.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

/// Desktop notification levels for a single channel, in menu order. The ids
/// are Mattermost's own; `"default"` means "whatever the account says".
const CHANNEL_LEVELS: [(&str, &str); 4] = [
    ("default", "Global default"),
    ("all", "All new messages"),
    ("mention", "Mentions only"),
    ("none", "Nothing"),
];

/// The same list for the account, which has nothing to fall back to.
const ACCOUNT_LEVELS: [(&str, &str); 3] = [
    ("all", "All new messages"),
    ("mention", "Mentions only"),
    ("none", "Nothing"),
];

/// Create a channel. Answers (display_name, url_name, purpose, private).
pub fn create_channel(
    parent: &impl IsA<gtk::Window>,
    on_create: impl Fn(String, String, String, bool) + 'static,
) {
    let name = adw::EntryRow::builder().title("Name").build();
    let url = adw::EntryRow::builder().title("URL name").build();
    let purpose = adw::EntryRow::builder().title("Purpose (optional)").build();
    let private = adw::SwitchRow::builder()
        .title("Private channel")
        .subtitle("Only invited people can find it")
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&name);
    group.add(&url);
    group.add(&purpose);
    group.add(&private);

    let dialog = adw::MessageDialog::new(Some(parent), Some("Create a channel"), None);
    dialog.set_extra_child(Some(&group));
    dialog.add_responses(&[("cancel", "Cancel"), ("create", "Create")]);
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    // A channel with no name is the one mistake worth blocking outright.
    dialog.set_response_enabled("create", false);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("cancel");

    // The URL follows the name until someone edits it themselves; from then on
    // it is theirs, and we notice by remembering what we last wrote there.
    let autofilled = Rc::new(RefCell::new(String::new()));
    name.connect_changed({
        let url = url.clone();
        let dialog = dialog.clone();
        let autofilled = autofilled.clone();
        move |name| {
            let text = name.text();
            dialog.set_response_enabled("create", !text.trim().is_empty());
            let ours = url.text().as_str() == autofilled.borrow().as_str();
            if ours {
                let slug = slugify(text.trim());
                url.set_text(&slug);
                *autofilled.borrow_mut() = slug;
            }
        }
    });

    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        if response != "create" {
            return;
        }
        let display_name = name.text().trim().to_string();
        let mut url_name = url.text().trim().to_string();
        // Emptied by hand, or a name with nothing ASCII in it to slug.
        if url_name.is_empty() {
            url_name = slugify(&display_name);
        }
        on_create(
            display_name,
            url_name,
            purpose.text().trim().to_string(),
            private.is_active(),
        );
    });
    dialog.present();
}

/// Browse and join channels.
///
/// The window outlives this call, so the handle keeps the list around: the
/// caller searches the server on [`Self::present`]'s `on_search` and pours the
/// answer back in through [`Self::set_results`].
pub struct ChannelBrowser {
    list: gtk::ListBox,
    on_join: Rc<dyn Fn(String)>,
}

impl ChannelBrowser {
    pub fn present(
        parent: &impl IsA<gtk::Window>,
        on_search: impl Fn(String) + 'static,
        on_join: impl Fn(String) + 'static,
    ) -> Self {
        // Per keystroke, not per activate: GtkSearchEntry already holds the
        // signal back until typing pauses, and the caller sees a whole term.
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search channels")
            .width_request(280)
            .build();
        let on_search = Rc::new(on_search);
        search.connect_search_changed({
            let on_search = on_search.clone();
            move |entry| on_search(entry.text().trim().to_string())
        });
        // Ask once on open: a browser that is empty until you type is a search
        // box, and browsing is the point.
        on_search(String::new());

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .valign(gtk::Align::Start)
            .build();
        list.add_css_class("boxed-list");
        let empty = gtk::Label::builder()
            .label("No channels found")
            .margin_top(24)
            .build();
        empty.add_css_class("dim-label");
        // GtkListBox shows this only while there are no rows, which covers both
        // "nothing typed yet" and "nothing matched".
        list.set_placeholder(Some(&empty));

        let clamp = adw::Clamp::builder()
            .maximum_size(560)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .child(&list)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();

        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::builder().title_widget(&search).build());
        view.set_content(Some(&scroller));

        let window = adw::Window::builder()
            .title("Browse channels")
            .default_width(460)
            .default_height(620)
            .modal(true)
            .build();
        window.set_transient_for(Some(parent));
        window.set_content(Some(&view));
        window.present();

        ChannelBrowser {
            list,
            on_join: Rc::new(on_join),
        }
    }

    pub fn set_results(&self, channels: Vec<(String, String, String, bool)>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        for (id, display_name, purpose, already_member) in channels {
            // AdwActionRow reads its title as Pango markup, and a channel name
            // is free text — an ampersand in one would otherwise blank the row.
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&display_name))
                .build();
            if !purpose.is_empty() {
                row.set_subtitle(&glib::markup_escape_text(&purpose));
                row.set_subtitle_lines(2);
            }

            if already_member {
                let joined = gtk::Label::new(Some("Joined"));
                joined.add_css_class("dim-label");
                row.add_suffix(&joined);
            } else {
                let button = gtk::Button::builder()
                    .label("Join")
                    .valign(gtk::Align::Center)
                    .build();
                button.add_css_class("suggested-action");
                button.connect_clicked({
                    let on_join = self.on_join.clone();
                    // The window stays open so you can join several at once;
                    // the button acknowledges the click itself, because the
                    // caller's answer arrives over the network much later.
                    move |button| {
                        button.set_sensitive(false);
                        button.set_label("Joined");
                        on_join(id.clone());
                    }
                });
                row.add_suffix(&button);
            }

            self.list.append(&row);
        }
    }
}

/// Per-channel notification settings. `current` is (desktop_level,
/// mark_unread_all, ignore_channel_mentions) where desktop_level is one of
/// "default"|"all"|"mention"|"none". Answers the same triple.
pub fn channel_notifications(
    parent: &impl IsA<gtk::Window>,
    channel_name: &str,
    current: (String, bool, bool),
    on_save: impl Fn(String, bool, bool) + 'static,
) {
    let (level, mark_unread_all, ignore_channel_mentions) = current;

    let desktop = level_row("Desktop notifications", &CHANNEL_LEVELS, &level);
    let unread = adw::SwitchRow::builder()
        .title("Mark as unread")
        .subtitle("For every message, not only mentions")
        .active(mark_unread_all)
        .build();
    let ignore = adw::SwitchRow::builder()
        .title("Ignore @channel, @here and @all")
        .active(ignore_channel_mentions)
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&desktop);
    group.add(&unread);
    group.add(&ignore);

    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some(&format!("Notifications for {channel_name}")),
        None,
    );
    dialog.set_extra_child(Some(&group));
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        if response != "save" {
            return;
        }
        on_save(
            selected_level(&desktop, &CHANNEL_LEVELS),
            unread.is_active(),
            ignore.is_active(),
        );
    });
    dialog.present();
}

/// Account-wide notification settings: desktop level, sound on/off, mention
/// keywords (comma separated), and whether first name counts.
pub fn account_notifications(
    parent: &impl IsA<gtk::Window>,
    current: (String, bool, String, bool),
    on_save: impl Fn(String, bool, String, bool) + 'static,
) {
    let (level, sound, keywords, first_name) = current;

    let desktop = level_row("Desktop notifications", &ACCOUNT_LEVELS, &level);
    let sound_row = adw::SwitchRow::builder()
        .title("Notification sound")
        .active(sound)
        .build();
    let keywords_row = adw::EntryRow::builder()
        .title("Keywords that mention me")
        .text(keywords)
        .build();
    let first_name_row = adw::SwitchRow::builder()
        .title("My first name")
        .subtitle("Notify me when someone types it")
        .active(first_name)
        .build();

    let group = adw::PreferencesGroup::builder()
        .description("Keywords are separated by commas and ignore case.")
        .build();
    group.add(&desktop);
    group.add(&sound_row);
    group.add(&keywords_row);
    group.add(&first_name_row);

    let dialog = adw::MessageDialog::new(Some(parent), Some("Notifications"), None);
    dialog.set_extra_child(Some(&group));
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        if response != "save" {
            return;
        }
        on_save(
            selected_level(&desktop, &ACCOUNT_LEVELS),
            sound_row.is_active(),
            keywords_row.text().trim().to_string(),
            first_name_row.is_active(),
        );
    });
    dialog.present();
}

/// Confirm leaving a channel.
pub fn confirm_leave(
    parent: &impl IsA<gtk::Window>,
    channel_name: &str,
    on_leave: impl Fn() + 'static,
) {
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some(&format!("Leave {channel_name}?")),
        Some("You will stop receiving its messages. You can join a public channel again later."),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("leave", "Leave")]);
    dialog.set_response_appearance("leave", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        if response == "leave" {
            on_leave();
        }
    });
    dialog.present();
}

/// Schedule a message: answers a Unix millisecond timestamp.
pub fn schedule_message(parent: &impl IsA<gtk::Window>, on_schedule: impl Fn(i64) + 'static) {
    let calendar = gtk::Calendar::new();

    let hour = gtk::SpinButton::with_range(0.0, 23.0, 1.0);
    let minute = gtk::SpinButton::with_range(0.0, 59.0, 5.0);
    hour.set_value(9.0);
    minute.set_value(0.0);
    // Wrapping so 23 rolls to 00 rather than stopping dead at the end.
    hour.set_wrap(true);
    minute.set_wrap(true);

    let time = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk::Align::Center)
        .margin_top(12)
        .build();
    time.append(&hour);
    time.append(&gtk::Label::new(Some(":")));
    time.append(&minute);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    content.append(&calendar);
    content.append(&time);

    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Schedule message"),
        Some("Pick one of the usual times, or a date and time below."),
    );
    dialog.set_extra_child(Some(&content));
    dialog.add_responses(&[
        ("cancel", "Cancel"),
        ("tomorrow", "Tomorrow morning"),
        ("monday", "Monday morning"),
        ("custom", "Schedule"),
    ]);
    dialog.set_response_appearance("custom", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("custom"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        let millis = match response {
            "tomorrow" => morning_in(1),
            "monday" => next_monday_morning(),
            "custom" => glib::DateTime::from_local(
                calendar.year(),
                // GtkCalendar counts months from zero, GDateTime from one.
                calendar.month() + 1,
                calendar.day(),
                hour.value_as_int(),
                minute.value_as_int(),
                0.0,
            )
            .ok()
            .map(unix_millis),
            _ => None,
        };
        if let Some(millis) = millis {
            on_schedule(millis);
        }
    });
    dialog.present();
}

/// Set a reminder about a post: answers a Unix millisecond timestamp.
pub fn post_reminder(parent: &impl IsA<gtk::Window>, on_remind: impl Fn(i64) + 'static) {
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Remind me about this"),
        Some("The system bot will send you the message again."),
    );
    dialog.add_responses(&[
        ("cancel", "Cancel"),
        ("30m", "In 30 minutes"),
        ("1h", "In 1 hour"),
        ("2h", "In 2 hours"),
        ("tomorrow", "Tomorrow morning"),
    ]);
    dialog.set_default_response(Some("tomorrow"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        let millis = match response {
            "30m" => from_now(|now| now.add_minutes(30)),
            "1h" => from_now(|now| now.add_hours(1)),
            "2h" => from_now(|now| now.add_hours(2)),
            "tomorrow" => morning_in(1),
            _ => None,
        };
        if let Some(millis) = millis {
            on_remind(millis);
        }
    });
    dialog.present();
}

/// A combo row over one of the level tables, showing the labels and answering
/// with the ids.
fn level_row(title: &str, levels: &[(&str, &str)], current: &str) -> adw::ComboRow {
    let labels: Vec<&str> = levels.iter().map(|(_, label)| *label).collect();
    let row = adw::ComboRow::builder().title(title).build();
    row.set_model(Some(&gtk::StringList::new(&labels)));
    row.set_selected(level_index(levels, current));
    row
}

fn selected_level(row: &adw::ComboRow, levels: &[(&str, &str)]) -> String {
    levels
        .get(row.selected() as usize)
        .map_or(levels[0].0, |(id, _)| *id)
        .to_string()
}

/// Where `value` sits in `levels`, falling back to the first entry: the server
/// can send a level we do not offer, and there is nothing better to show.
fn level_index(levels: &[(&str, &str)], value: &str) -> u32 {
    levels.iter().position(|(id, _)| *id == value).unwrap_or(0) as u32
}

fn unix_millis(time: glib::DateTime) -> i64 {
    time.to_unix() * 1000
}

/// `offset` applied to now, in Unix milliseconds. Every glib date operation
/// can fail, and a reminder we cannot place is simply not set.
fn from_now(
    offset: impl Fn(&glib::DateTime) -> Result<glib::DateTime, glib::BoolError>,
) -> Option<i64> {
    let now = glib::DateTime::now_local().ok()?;
    offset(&now).ok().map(unix_millis)
}

/// 9am local time, `days` days from today.
fn morning_in(days: i32) -> Option<i64> {
    let day = glib::DateTime::now_local().ok()?.add_days(days).ok()?;
    glib::DateTime::from_local(day.year(), day.month(), day.day_of_month(), 9, 0, 0.0)
        .ok()
        .map(unix_millis)
}

fn next_monday_morning() -> Option<i64> {
    let today = glib::DateTime::now_local().ok()?;
    morning_in(days_until_next_monday(today.day_of_week()))
}

/// Days from `day_of_week` (glib numbers them 1 = Monday … 7 = Sunday) to the
/// next Monday. Always in the future: asked on a Monday it means the next one,
/// because "Monday morning" is never the morning you are already in.
fn days_until_next_monday(day_of_week: i32) -> i32 {
    ((7 - day_of_week) % 7) + 1
}

/// Mattermost channel URLs take lowercase letters, digits, dashes and
/// underscores, so everything else becomes a separator.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_url_safe() {
        assert_eq!(slugify("Release Planning"), "release-planning");
        assert_eq!(slugify("  Q3 / 2026 — plans!  "), "q3-2026-plans");
        assert_eq!(slugify("keep_underscores"), "keep_underscores");
        // Nothing ASCII to work with: the caller has to notice and ask again.
        assert_eq!(slugify("Привет"), "");
    }

    #[test]
    fn next_monday_is_always_ahead() {
        assert_eq!(days_until_next_monday(1), 7); // Monday -> the one after
        assert_eq!(days_until_next_monday(2), 6); // Tuesday
        assert_eq!(days_until_next_monday(5), 3); // Friday
        assert_eq!(days_until_next_monday(7), 1); // Sunday -> tomorrow
    }

    #[test]
    fn unknown_levels_fall_back_to_the_first() {
        assert_eq!(level_index(&CHANNEL_LEVELS, "mention"), 2);
        assert_eq!(level_index(&ACCOUNT_LEVELS, "mention"), 1);
        assert_eq!(level_index(&CHANNEL_LEVELS, "something_new"), 0);
    }
}
