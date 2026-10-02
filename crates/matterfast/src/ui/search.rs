//! The message search box in the title bar, and what it suggests.
//!
//! Mattermost searches by a line of text with modifiers in it — `from:anna
//! in:town-square before:2026-10-03 release` — and the server is what reads
//! that line; nothing here parses a search. What this module does is make the
//! grammar findable: an empty box lists the modifiers, a modifier lists what
//! can follow it (people, channels, a few dates), and picking a row writes it
//! into the line the way it would have been typed.
//!
//! Like the composer's completion list ([`super::autocomplete`]) it never
//! talks to the network: the box reports the word under the cursor, the
//! session answers with rows. Working out what is being typed and writing a
//! pick back are plain functions, tested without a window.
//!
//! Unlike that list, nothing is selected until an arrow key says so: Enter in
//! a search box means "search", and must not turn into "from:" because a list
//! happened to be open.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{Days, Months, NaiveDate};
use gpui_kit::component::input::{
    Enter, Escape, IndentInline, Input, InputEvent, InputState, MoveDown, MoveUp,
};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{
    deferred, div, px, relative, AnyElement, App, Entity, Focusable, FontWeight, MouseButton,
    Subscription, Window,
};
use mattermost_api::models::ChannelType;

use super::kit::Lucide;
use super::{Action, Ui, WindowSlot};
use crate::state::AppState;

/// How many rows a list of people or channels shows.
const ROWS: usize = 8;

/// The modifiers the server understands, and what each is for.
const MODIFIERS: [(&str, &str); 5] = [
    ("from:", "Messages from a person"),
    ("in:", "Messages in a channel"),
    ("before:", "Messages before a date"),
    ("after:", "Messages after a date"),
    ("on:", "Messages on a date"),
];

/// This application's own: it sends the whole line to the file search
/// instead, so it only means something as the first word.
const FILES: (&str, &str) = ("file:", "Search attachments instead");

/// What the word under the cursor is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hint {
    /// The beginning of a word that may become a modifier; `first` when it
    /// is the first word of the line.
    Modifiers { typed: String, first: bool },
    /// After `from:` — whose messages.
    From(String),
    /// After `in:` — which channel's.
    In(String),
    /// After a date modifier, which is carried along so the row can say it.
    Date(&'static str, String),
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// What replaces the word under the cursor.
    pub insert: String,
    pub label: String,
    /// Dimmed, to the right.
    pub detail: String,
    /// A modifier is half a word: the cursor stays right behind it, and the
    /// list goes on to what can follow.
    pub open_ended: bool,
}

/// Where the word that ends at the end of `before` begins.
fn word_start(before: &str) -> usize {
    before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8())
}

/// What to suggest for the word that ends at `cursor`, a byte offset into
/// `text`. Nothing for an ordinary word: the list is for the grammar, and
/// must stay out of the way of somebody typing what they are looking for.
pub fn hint_at(text: &str, cursor: usize) -> Option<Hint> {
    let before = text.get(..cursor)?;
    let start = word_start(before);
    let word = before[start..].to_lowercase();
    let after = |modifier: &str| word.strip_prefix(modifier).map(str::to_string);

    if let Some(name) = after("from:") {
        return Some(Hint::From(name.trim_start_matches('@').to_string()));
    }
    if let Some(name) = after("in:") {
        return Some(Hint::In(name.trim_start_matches('~').to_string()));
    }
    for modifier in ["before:", "after:", "on:"] {
        if let Some(day) = after(modifier) {
            return Some(Hint::Date(modifier, day));
        }
    }
    let first = before[..start].trim().is_empty();
    let begins = |(name, _): &(&str, &str)| name.starts_with(word.as_str());
    (MODIFIERS.iter().any(begins) || (first && begins(&FILES)))
        .then_some(Hint::Modifiers { typed: word, first })
}

/// Writes a picked row over the word under the cursor. Answers the new text
/// and where the cursor belongs in it.
pub fn accept(text: &str, cursor: usize, picked: &Suggestion) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let Some(before) = text.get(..cursor) else {
        return (text.to_string(), cursor);
    };
    let start = word_start(before);
    let mut out = String::with_capacity(text.len() + picked.insert.len() + 1);
    out.push_str(&text[..start]);
    out.push_str(&picked.insert);
    if !picked.open_ended {
        out.push(' ');
    }
    let caret = out.len();
    out.push_str(&text[cursor..]);
    (out, caret)
}

fn modifiers(typed: &str, first: bool) -> Vec<Suggestion> {
    MODIFIERS
        .iter()
        .chain(first.then_some(&FILES))
        .filter(|(name, _)| name.starts_with(typed))
        .map(|(name, purpose)| Suggestion {
            insert: (*name).to_string(),
            label: (*name).to_string(),
            detail: (*purpose).to_string(),
            open_ended: true,
        })
        .collect()
}

/// The days people mean most often, for the date modifiers. The server wants
/// `YYYY-MM-DD`; a row is a way not to have to work one out. Nothing once a
/// whole date has been typed — there is nothing left to suggest.
fn dates(modifier: &str, typed: &str, today: NaiveDate) -> Vec<Suggestion> {
    if NaiveDate::parse_from_str(typed, "%Y-%m-%d").is_ok() {
        return Vec::new();
    }
    let days = [
        ("Today", Some(today)),
        ("Yesterday", today.checked_sub_days(Days::new(1))),
        ("A week ago", today.checked_sub_days(Days::new(7))),
        ("A month ago", today.checked_sub_months(Months::new(1))),
    ];
    days.into_iter()
        .filter_map(|(label, day)| Some((label, day?.format("%Y-%m-%d").to_string())))
        .filter(|(label, day)| day.starts_with(typed) || label.to_lowercase().starts_with(typed))
        .map(|(label, day)| Suggestion {
            insert: format!("{modifier}{day}"),
            label: label.to_string(),
            detail: day,
            open_ended: false,
        })
        .collect()
}

/// A person as a `from:` row.
pub fn person(username: &str, shown: &str) -> Suggestion {
    Suggestion {
        insert: format!("from:{username}"),
        label: shown.to_string(),
        detail: format!("@{username}"),
        open_ended: false,
    }
}

/// The people already known here whose handle or name has `typed` in it,
/// those it begins first.
fn people(st: &AppState, typed: &str) -> Vec<Suggestion> {
    let display = st.teammate_name_display();
    let mut found: Vec<(bool, Suggestion)> = st
        .users
        .values()
        .filter(|user| user.delete_at == 0)
        .filter_map(|user| {
            let handle = user.username.to_lowercase();
            let shown = user.display_name(display);
            let begins = handle.starts_with(typed) || shown.to_lowercase().starts_with(typed);
            let names = [&user.first_name, &user.last_name, &user.nickname];
            let has = begins
                || handle.contains(typed)
                || names.iter().any(|name| name.to_lowercase().contains(typed));
            has.then(|| (!begins, person(&user.username, &shown)))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    found.into_iter().map(|(_, row)| row).take(ROWS).collect()
}

/// The channels of this team, and the direct messages, that `typed` is in
/// the name of. A channel is searched by its URL name; a direct message by
/// the other person's handle, which the server spells `@handle`.
fn channels(st: &AppState, typed: &str) -> Vec<Suggestion> {
    let team = st.current_team.as_deref().unwrap_or_default();
    let mut found: Vec<(bool, Suggestion)> = st
        .channels
        .values()
        .filter(|channel| channel.delete_at == 0)
        .filter_map(|channel| {
            let (name, title) = match channel.r#type {
                ChannelType::Direct => {
                    let other = st.users.get(channel.dm_teammate_id(&st.me.id)?)?;
                    (format!("@{}", other.username), st.channel_title(channel))
                }
                ChannelType::Group => return None,
                _ if channel.team_id == team => {
                    (channel.name.clone(), channel.display_name.clone())
                }
                _ => return None,
            };
            let plain = name.trim_start_matches('@').to_string();
            let typed = typed.trim_start_matches('@');
            let begins = plain.starts_with(typed) || title.to_lowercase().starts_with(typed);
            let has = begins || plain.contains(typed) || title.to_lowercase().contains(typed);
            has.then(|| {
                let row = Suggestion {
                    insert: format!("in:{name}"),
                    label: title,
                    detail: name,
                    open_ended: false,
                };
                (!begins, row)
            })
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    found.into_iter().map(|(_, row)| row).take(ROWS).collect()
}

/// The rows for a hint, from what is already known here.
fn suggestions(st: &AppState, hint: &Hint, today: NaiveDate) -> Vec<Suggestion> {
    match hint {
        Hint::Modifiers { typed, first } => modifiers(typed, *first),
        Hint::From(typed) => people(st, typed),
        Hint::In(typed) => channels(st, typed),
        Hint::Date(modifier, typed) => dates(modifier, typed, today),
    }
}

/// What the list is headed with.
fn heading(hint: &Hint) -> &'static str {
    match hint {
        Hint::Modifiers { .. } => "Search options",
        Hint::From(_) => "From",
        Hint::In(_) => "In",
        Hint::Date(..) => "Date, as YYYY-MM-DD",
    }
}

/// The search box of the session.
pub struct SearchBox {
    window: Rc<WindowSlot>,
    /// The field, while there is a window to put it in.
    input: RefCell<Option<Entity<InputState>>>,
    /// What the word under the cursor asks for; `None` closes the list.
    hint: RefCell<Option<Hint>>,
    rows: RefCell<Vec<Suggestion>>,
    /// The row an arrow key went to. Nothing until one is pressed.
    selected: Cell<Option<usize>>,
    /// Whether the cursor was in the box when it was last drawn. Asked of the
    /// window on every frame rather than kept from the field's focus events:
    /// those only come while the window itself has the keyboard, and a list
    /// that waits for them never opens in a window that was focused from a
    /// shortcut before the desktop said so.
    focused: Cell<bool>,
}

impl SearchBox {
    pub fn new(window: Rc<WindowSlot>) -> Self {
        SearchBox {
            window,
            input: RefCell::new(None),
            hint: RefCell::new(None),
            rows: RefCell::new(Vec::new()),
            selected: Cell::new(None),
            focused: Cell::new(false),
        }
    }

    pub(super) fn attach(&self, input: Entity<InputState>) {
        *self.input.borrow_mut() = Some(input);
    }

    pub(super) fn detach(&self) {
        self.input.borrow_mut().take();
        self.close();
    }

    /// Puts the cursor in the box.
    pub fn focus(&self, cx: &mut App) {
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        self.window.update(cx, move |window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
        });
    }

    /// What the list under the box offers right now, as the scripted checks
    /// read it: the rows' labels, or nothing while it is closed.
    pub(super) fn offered(&self) -> Vec<String> {
        if !self.is_open() {
            return Vec::new();
        }
        self.rows.borrow().iter().map(|row| row.label.clone()).collect()
    }

    /// What is in the box.
    pub(super) fn text(&self, cx: &App) -> String {
        self.input
            .borrow()
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    fn is_open(&self) -> bool {
        self.focused.get() && self.hint.borrow().is_some() && !self.rows.borrow().is_empty()
    }

    fn close(&self) {
        self.hint.borrow_mut().take();
        self.rows.borrow_mut().clear();
        self.selected.set(None);
    }

    /// What `from:` is being completed with, if that is what is being typed:
    /// the session looks the same name up on the server.
    pub(super) fn asking_for_person(&self) -> Option<String> {
        match &*self.hint.borrow() {
            Some(Hint::From(typed)) => Some(typed.clone()),
            _ => None,
        }
    }

    /// People the server found for the name being typed, added under the
    /// ones already listed.
    pub(super) fn add_people(&self, found: Vec<Suggestion>, cx: &mut App) {
        let mut rows = self.rows.borrow_mut();
        for row in found {
            if rows.len() >= ROWS {
                break;
            }
            if !rows.iter().any(|known| known.insert == row.insert) {
                rows.push(row);
            }
        }
        drop(rows);
        cx.refresh_windows();
    }

    /// The text or the cursor moved: works out what to suggest now.
    fn changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        let (text, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let hint = hint_at(&text, cursor.min(text.len()));
        let rows = hint.as_ref().map_or_else(Vec::new, |hint| {
            let today = chrono::Local::now().date_naive();
            suggestions(&ui.state.borrow(), hint, today)
        });
        let person = matches!(hint, Some(Hint::From(_)));
        *self.hint.borrow_mut() = hint;
        *self.rows.borrow_mut() = rows;
        self.selected.set(None);
        if person {
            ui.dispatch(Action::SearchPeople, cx);
        }
        cx.refresh_windows();
    }

    fn step(&self, delta: i32) {
        let len = self.rows.borrow().len();
        if len == 0 {
            return;
        }
        let next = match self.selected.get() {
            None if delta < 0 => len - 1,
            None => 0,
            Some(at) => super::autocomplete::next_index(at, delta, len),
        };
        self.selected.set(Some(next));
    }

    /// Writes the row at `index` into the line and goes on suggesting from
    /// where that leaves the cursor.
    fn pick(&self, index: usize, ui: &Rc<Ui>, cx: &mut App) {
        let Some(picked) = self.rows.borrow().get(index).cloned() else {
            return;
        };
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        let (text, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let (text, caret) = accept(&text, cursor, &picked);
        self.window.update(cx, move |window, cx| {
            input.update(cx, |input, cx| {
                input.set_value(text.clone(), window, cx);
                input.set_selected_range(caret..caret, cx);
                input.focus(window, cx);
            });
        });
        self.changed(ui, cx);
    }

    /// Enter: searches for what is in the box.
    fn submit(&self, ui: &Rc<Ui>, cx: &mut App) {
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        let terms = input.read(cx).value().trim().to_string();
        if terms.is_empty() {
            return;
        }
        self.close();
        ui.dispatch(Action::Search(terms), cx);
        cx.refresh_windows();
    }
}

/// Builds the search box and wires what it reports to the session.
pub(super) fn build(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<InputState>, Subscription) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search messages"));
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&input, move |_, event: &InputEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            InputEvent::Change => ui.later(cx, |ui, cx| ui.search_box.changed(ui, cx)),
            // On Enter, not on every keystroke: a post search is a round trip
            // to the server, and searching per character would be a request
            // per character.
            InputEvent::PressEnter { .. } => ui.later(cx, |ui, cx| ui.search_box.submit(ui, cx)),
            // Where the cursor is, is read off the window when the box is
            // drawn: see `SearchBox::focused`.
            InputEvent::Focus | InputEvent::Blur => {}
        }
    });
    (input, subscription)
}

/// The box as the title bar shows it, `width` wide, with its list under it.
pub fn render(ui: &Rc<Ui>, width: f32, window: &Window, cx: &mut App) -> AnyElement {
    let search = &ui.search_box;
    let Some(input) = search.input.borrow().clone() else {
        return div().into_any_element();
    };
    // The cursor arriving in the box is what opens the list, before anything
    // is typed; leaving it closes the list by `is_open` alone.
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    if search.focused.replace(focused) != focused && focused {
        ui.later(cx, |ui, cx| ui.search_box.changed(ui, cx));
    }
    v_flex()
        .id("title-search")
        .relative()
        .flex_none()
        .w(px(width))
        // The bar under it is what drags the window.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        // The list has first refusal on these keys while a row of it is
        // chosen, and on the arrows whenever it is open.
        .capture_action({
            let ui = ui.clone();
            move |_: &MoveUp, _, cx| {
                if ui.search_box.is_open() {
                    ui.search_box.step(-1);
                    cx.stop_propagation();
                    cx.refresh_windows();
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &MoveDown, _, cx| {
                if ui.search_box.is_open() {
                    ui.search_box.step(1);
                    cx.stop_propagation();
                    cx.refresh_windows();
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &Enter, _, cx| {
                let chosen = ui.search_box.selected.get();
                if let Some(index) = chosen.filter(|_| ui.search_box.is_open()) {
                    cx.stop_propagation();
                    ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                }
            }
        })
        // Tab takes the chosen row, or the first: it is the key for "yes,
        // that one" and has nothing else to do in a one-line box.
        .capture_action({
            let ui = ui.clone();
            move |_: &IndentInline, _, cx| {
                if ui.search_box.is_open() {
                    let index = ui.search_box.selected.get().unwrap_or(0);
                    cx.stop_propagation();
                    ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &Escape, _, cx| {
                if ui.search_box.is_open() {
                    ui.search_box.close();
                    cx.stop_propagation();
                    cx.refresh_windows();
                }
            }
        })
        .child(
            Input::new(&input)
                .small()
                .prefix(Lucide::Search)
                .cleanable(true),
        )
        .when(search.is_open(), |field| {
            // Deferred, so that it is drawn over the panes under the bar
            // rather than under them.
            field.child(deferred(list(ui, cx)).with_priority(1))
        })
        .into_any_element()
}

/// The list under the box.
fn list(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let search = &ui.search_box;
    let theme = cx.theme();
    let title = search.hint.borrow().as_ref().map_or("", heading);
    let mut rows = v_flex()
        .id("search-hints")
        .absolute()
        // Under the box: its place is counted from the box's own top.
        .top(relative(1.))
        .mt_1()
        .left_0()
        .w_full()
        .min_w(px(320.))
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_md()
        .occlude()
        .child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(title),
        );
    let selected = search.selected.get();
    for (index, row) in search.rows.borrow().iter().enumerate() {
        rows = rows.child(
            h_flex()
                .id(("hint", index))
                .gap_2()
                .px_2()
                .h(px(28.))
                .items_center()
                .rounded_sm()
                .text_sm()
                .cursor_pointer()
                .when(selected == Some(index), |row| row.bg(theme.accent))
                .hover(|style| style.bg(theme.list_hover))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::MEDIUM)
                        .child(row.label.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_right()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(row.detail.clone()),
                )
                // On the press, not the click: the press is also what takes
                // the focus out of the box, and with it the list.
                .on_mouse_down(MouseButton::Left, {
                    let ui = ui.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        window.prevent_default();
                        ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                    }
                }),
        );
    }
    rows.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The usual case: the cursor is at the end of what has been typed.
    fn typed(text: &str) -> Option<Hint> {
        hint_at(text, text.len())
    }

    fn labels(rows: &[Suggestion]) -> Vec<&str> {
        rows.iter().map(|row| row.label.as_str()).collect()
    }

    #[test]
    fn an_empty_box_offers_the_modifiers() {
        assert_eq!(typed(""), Some(Hint::Modifiers { typed: String::new(), first: true }));
        assert_eq!(
            labels(&modifiers("", true)),
            ["from:", "in:", "before:", "after:", "on:", "file:"],
        );
        // `file:` turns the whole line into a file search, so it is only
        // offered where it would do that.
        assert_eq!(
            typed("release "),
            Some(Hint::Modifiers { typed: String::new(), first: false }),
        );
        assert!(!labels(&modifiers("", false)).contains(&"file:"));
    }

    #[test]
    fn the_beginning_of_a_modifier_narrows_the_list() {
        assert_eq!(typed("f"), Some(Hint::Modifiers { typed: "f".into(), first: true }));
        assert_eq!(labels(&modifiers("f", true)), ["from:", "file:"]);
        assert_eq!(labels(&modifiers("be", false)), ["before:"]);
    }

    #[test]
    fn an_ordinary_word_is_left_alone() {
        assert_eq!(typed("release"), None);
        assert_eq!(typed("from the start"), None);
        // `file` is the beginning of a modifier only as the first word.
        assert_eq!(typed("notes fil"), None);
    }

    #[test]
    fn a_modifier_asks_for_what_follows_it() {
        assert_eq!(typed("from:"), Some(Hint::From(String::new())));
        assert_eq!(typed("notes From:@An"), Some(Hint::From("an".into())));
        assert_eq!(typed("in:~town"), Some(Hint::In("town".into())));
        assert_eq!(typed("on:2026-1"), Some(Hint::Date("on:", "2026-1".into())));
        assert_eq!(typed("x after:"), Some(Hint::Date("after:", String::new())));
    }

    #[test]
    fn only_the_word_up_to_the_cursor_counts() {
        assert_eq!(hint_at("from:anna release", 7), Some(Hint::From("an".into())));
    }

    #[test]
    fn a_modifier_is_written_without_a_space_and_a_value_with_one() {
        let modifier = &modifiers("fr", true)[0];
        assert_eq!(accept("fr", 2, modifier), ("from:".to_string(), 5));
        let anna = person("anna", "Anna Berg");
        assert_eq!(
            accept("release from:an notes", 15, &anna),
            ("release from:anna  notes".to_string(), 18),
        );
    }

    #[test]
    fn the_days_people_mean_are_offered_until_a_date_is_typed() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let rows = dates("before:", "", today);
        assert_eq!(labels(&rows), ["Today", "Yesterday", "A week ago", "A month ago"]);
        assert_eq!(rows[0].insert, "before:2026-03-01");
        assert_eq!(rows[1].insert, "before:2026-02-28");
        assert_eq!(rows[3].insert, "before:2026-02-01");
        // By the name of the day or by the beginning of the date.
        assert_eq!(labels(&dates("on:", "yes", today)), ["Yesterday"]);
        assert_eq!(labels(&dates("on:", "2026-03", today)), ["Today"]);
        assert!(dates("on:", "2025-12-24", today).is_empty());
    }
}
