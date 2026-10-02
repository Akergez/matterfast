//! The dialogs that ask a question and hand the answer back.
//!
//! Nothing here talks to the server or to [`crate::state`]: each function takes
//! a callback and calls it with what the person chose. That keeps the API calls
//! in [`super`], where the client and the action queue already live, and it
//! means these can be read (and moved) without tracing a request through them.
//!
//! There are really only two shapes. A **form** is some fields and a button:
//! make a channel, set a status, pick a time. A **list** is rows that each
//! offer something, optionally under a search box: browse channels, see who is
//! here, look through what you scheduled. Everything else in this file is one
//! of those two with its fields or its rows filled in.
//!
//! Every callback is run *after* the click that caused it has finished being
//! handled, never during — the session code they call may want the window, and
//! a click is the window in the middle of something.

use std::collections::HashSet;
use std::rc::Rc;

use chrono::{Datelike, Days, Local, NaiveDate, NaiveTime, TimeZone};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, px, App, Context, ElementId, Entity, FontWeight, SharedString, Subscription,
    Window,
};

use super::kit::{self, Lucide};
use super::Ui;

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

/// Something to run once the click that asked for it is over.
type Callback = Rc<dyn Fn(&mut App)>;

fn run_later(callback: &Callback, cx: &mut App) {
    let callback = callback.clone();
    cx.defer(move |cx| callback(cx));
}

// ---------------------------------------------------------------------- forms

/// One field of a form.
pub enum Field {
    Text {
        label: String,
        value: String,
        placeholder: String,
    },
    Switch {
        label: String,
        subtitle: String,
        on: bool,
    },
    /// One of a fixed set, as (id, label). The value is the id.
    Choice {
        label: String,
        options: Vec<(String, String)>,
        selected: usize,
    },
}

impl Field {
    pub fn text(label: &str, value: &str) -> Field {
        Field::Text {
            label: label.to_string(),
            value: value.to_string(),
            placeholder: String::new(),
        }
    }

    pub fn switch(label: &str, subtitle: &str, on: bool) -> Field {
        Field::Switch {
            label: label.to_string(),
            subtitle: subtitle.to_string(),
            on,
        }
    }

    pub fn choice(label: &str, options: &[(&str, &str)], current: &str) -> Field {
        Field::Choice {
            label: label.to_string(),
            options: options
                .iter()
                .map(|(id, label)| (id.to_string(), label.to_string()))
                .collect(),
            selected: level_index(options, current),
        }
    }
}

/// What a form answered, field by field, in the order the fields were given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Bool(bool),
    Choice(String),
}

impl Value {
    pub fn text(&self) -> String {
        match self {
            Value::Text(text) | Value::Choice(text) => text.clone(),
            Value::Bool(on) => on.to_string(),
        }
    }

    pub fn bool(&self) -> bool {
        matches!(self, Value::Bool(true))
    }
}

enum Slot {
    Text {
        label: String,
        input: Entity<InputState>,
    },
    Switch {
        label: String,
        subtitle: String,
        on: bool,
    },
    Choice {
        label: String,
        options: Vec<(String, String)>,
        selected: usize,
    },
}

/// The body of a form dialog: its fields, and what they currently hold.
pub struct Form {
    slots: Vec<Slot>,
    /// Text under the fields, explaining something about them.
    note: String,
    /// The first field feeds the second until someone edits the second
    /// themselves; from then on it is theirs, and we notice by remembering
    /// what we last wrote there.
    autofill: Option<(fn(&str) -> String, String)>,
    _subscriptions: Vec<Subscription>,
}

impl Form {
    pub(super) fn new(fields: Vec<Field>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let slots = fields
            .into_iter()
            .map(|field| match field {
                Field::Text {
                    label,
                    value,
                    placeholder,
                } => Slot::Text {
                    label,
                    input: cx.new(|cx| {
                        InputState::new(window, cx)
                            .default_value(value)
                            .placeholder(placeholder)
                    }),
                },
                Field::Switch {
                    label,
                    subtitle,
                    on,
                } => Slot::Switch {
                    label,
                    subtitle,
                    on,
                },
                Field::Choice {
                    label,
                    options,
                    selected,
                } => Slot::Choice {
                    label,
                    options,
                    selected,
                },
            })
            .collect();
        Form {
            slots,
            note: String::new(),
            autofill: None,
            _subscriptions: Vec::new(),
        }
    }

    /// Makes the second text field follow the first, through `derive`.
    fn follow(&mut self, derive: fn(&str) -> String, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(Slot::Text { input: source, .. }), Some(Slot::Text { .. })) =
            (self.slots.first(), self.slots.get(1))
        else {
            return;
        };
        self.autofill = Some((derive, String::new()));
        let subscription = cx.subscribe_in(
            source,
            window,
            |form, source, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some((derive, written)) = form.autofill.clone() else {
                    return;
                };
                let Some(Slot::Text { input: target, .. }) = form.slots.get(1) else {
                    return;
                };
                // Still ours to write: nobody has typed over what we put.
                if target.read(cx).value().as_ref() != written {
                    return;
                }
                let derived = derive(source.read(cx).value().trim());
                target.update(cx, |target, cx| {
                    target.set_value(derived.clone(), window, cx)
                });
                form.autofill = Some((derive, derived));
            },
        );
        self._subscriptions.push(subscription);
    }

    /// Text under the fields, explaining something about them.
    pub(super) fn set_note(&mut self, note: &str) {
        self.note = note.to_string();
    }

    /// Writes a text field, as though it had been typed.
    pub(super) fn set_text(
        &self,
        index: usize,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Slot::Text { input, .. }) = self.slots.get(index) {
            let value = value.to_string();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    /// Picks one of a choice field's options.
    pub(super) fn set_choice(&mut self, index: usize, option: usize, cx: &mut Context<Self>) {
        if let Some(Slot::Choice {
            selected, options, ..
        }) = self.slots.get_mut(index)
        {
            if option < options.len() {
                *selected = option;
                cx.notify();
            }
        }
    }

    /// Which option a choice field is on.
    pub(super) fn choice(&self, index: usize) -> usize {
        match self.slots.get(index) {
            Some(Slot::Choice { selected, .. }) => *selected,
            _ => 0,
        }
    }

    pub(super) fn values(&self, cx: &App) -> Vec<Value> {
        self.slots
            .iter()
            .map(|slot| match slot {
                Slot::Text { input, .. } => Value::Text(input.read(cx).value().to_string()),
                Slot::Switch { on, .. } => Value::Bool(*on),
                Slot::Choice {
                    options, selected, ..
                } => Value::Choice(
                    options
                        .get(*selected)
                        .map(|(id, _)| id.clone())
                        .unwrap_or_default(),
                ),
            })
            .collect()
    }
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let mut column = v_flex().gap_3();
        for (index, slot) in self.slots.iter().enumerate() {
            column = column.child(match slot {
                Slot::Text { label, input } => v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label.clone()))
                    .child(Input::new(input))
                    .into_any_element(),
                Slot::Switch {
                    label,
                    subtitle,
                    on,
                } => h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().child(label.clone()))
                            .when(!subtitle.is_empty(), |column| {
                                column.child(
                                    div().text_xs().text_color(muted).child(subtitle.clone()),
                                )
                            }),
                    )
                    .child(
                        Switch::new(("switch", index))
                            .checked(*on)
                            .on_click(cx.listener(move |form, on: &bool, _, cx| {
                                if let Some(Slot::Switch { on: held, .. }) =
                                    form.slots.get_mut(index)
                                {
                                    *held = *on;
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
                Slot::Choice {
                    label,
                    options,
                    selected,
                } => {
                    let entity = cx.entity();
                    let menu_options = options.clone();
                    let current = *selected;
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(div().flex_1().min_w_0().child(label.clone()))
                        .child(
                            Button::new(("choice", index))
                                .label(
                                    options
                                        .get(current)
                                        .map(|(_, label)| label.clone())
                                        .unwrap_or_default(),
                                )
                                .small()
                                .dropdown_caret(true)
                                .dropdown_menu(move |mut menu, _, _| {
                                    for (option, (_, label)) in menu_options.iter().enumerate() {
                                        let entity = entity.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(label.clone())
                                                .checked(option == current)
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |form, cx| {
                                                        if let Some(Slot::Choice {
                                                            selected, ..
                                                        }) = form.slots.get_mut(index)
                                                        {
                                                            *selected = option;
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }),
                        )
                        .into_any_element()
                }
            });
        }
        if !self.note.is_empty() {
            column = column.child(div().text_xs().text_color(muted).child(self.note.clone()));
        }
        column
    }
}

/// What a form dialog looks like, apart from its fields.
pub struct FormSpec {
    pub title: String,
    pub note: String,
    pub ok: String,
    /// Derive the second text field from the first while it is untouched.
    pub follow: Option<fn(&str) -> String>,
}

impl FormSpec {
    pub fn new(title: &str, ok: &str) -> Self {
        FormSpec {
            title: title.to_string(),
            note: String::new(),
            ok: ok.to_string(),
            follow: None,
        }
    }
}

/// Opens a form. `on_ok` gets the values and answers whether they were
/// acceptable: `false` keeps the dialog up so the person can fix them.
pub fn form(
    ui: &Rc<Ui>,
    cx: &mut App,
    spec: FormSpec,
    fields: Vec<Field>,
    on_ok: impl Fn(Vec<Value>, &mut App) -> bool + 'static,
) {
    let on_ok = Rc::new(on_ok);
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let mut form = Form::new(fields, window, cx);
            form.note = spec.note.clone();
            if let Some(derive) = spec.follow {
                form.follow(derive, window, cx);
            }
            form
        });
        let title: SharedString = spec.title.clone().into();
        let ok: SharedString = spec.ok.clone().into();
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let submit = view.clone();
            let on_ok = on_ok.clone();
            dialog
                .title(title.clone())
                .child(view)
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().child(Button::new("cancel").label("Cancel")))
                        .child(
                            DialogAction::new()
                                .child(Button::new("ok").label(ok.clone()).primary()),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let values = submit.read(cx).values(cx);
                    // Whether to close is decided now; what it means is done
                    // once the click is over.
                    let accepted = Rc::new(std::cell::Cell::new(true));
                    let on_ok = on_ok.clone();
                    // The answer has to be known synchronously, and the
                    // callbacks here only read their arguments to decide it.
                    accepted.set(on_ok(values, cx));
                    accepted.get()
                })
        });
    });
}

/// Asks a yes-or-no question. `danger` paints the confirming button as
/// something that cannot be taken back.
pub fn confirm(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    body: &str,
    ok: &str,
    danger: bool,
    on_ok: impl Fn(&mut App) + 'static,
) {
    let title: SharedString = title.to_string().into();
    let body: SharedString = body.to_string().into();
    let ok: SharedString = ok.to_string().into();
    let on_ok: Callback = Rc::new(on_ok);
    ui.with_window(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            let on_ok = on_ok.clone();
            let confirm = Button::new("ok").label(ok.clone());
            dialog
                .title(title.clone())
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(body.clone()),
                )
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().child(Button::new("cancel").label("Cancel")))
                        .child(DialogAction::new().child(if danger {
                            confirm.danger()
                        } else {
                            confirm.primary()
                        })),
                )
                .on_ok(move |_, _, cx| {
                    run_later(&on_ok, cx);
                    true
                })
        });
    });
}

/// Offers a handful of answers as buttons, one of which is picked.
fn choose(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    body: &str,
    options: Vec<(&'static str, Callback)>,
) {
    let title: SharedString = title.to_string().into();
    let body: SharedString = body.to_string().into();
    let options = Rc::new(options);
    ui.with_window(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            let mut column = v_flex().gap_2().child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(body.clone()),
            );
            for (index, (label, action)) in options.iter().enumerate() {
                let action = action.clone();
                column = column.child(
                    Button::new(("option", index))
                        .label(*label)
                        .w_full()
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            run_later(&action, cx);
                        }),
                );
            }
            dialog.title(title.clone()).child(column)
        });
    });
}

// ---------------------------------------------------------------------- lists

/// Something a row offers at its right-hand end.
enum Trailing {
    /// Words: "Joined", "Admin".
    Label(String),
    /// A button that acknowledges its own click, because the caller's answer
    /// arrives over the network much later.
    Button {
        label: String,
        done: String,
        action: Callback,
    },
    Icon {
        icon: Lucide,
        tooltip: String,
        action: Callback,
    },
}

/// One row of a list dialog.
pub struct Row {
    id: String,
    title: String,
    subtitle: String,
    /// Longer text under the title, shown whole rather than on one line.
    body: String,
    trailing: Vec<Trailing>,
    /// What pressing the row itself does, when the whole row is the target.
    activate: Option<Callback>,
}

impl Row {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Row {
        Row {
            id: id.into(),
            title: title.into(),
            subtitle: String::new(),
            body: String::new(),
            trailing: Vec::new(),
            activate: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Row {
        self.subtitle = subtitle.into();
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Row {
        self.body = body.into();
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Row {
        self.trailing.push(Trailing::Label(label.into()));
        self
    }

    /// A button that turns into `done` once pressed.
    pub fn button(
        mut self,
        label: &str,
        done: &str,
        action: impl Fn(&mut App) + 'static,
    ) -> Row {
        self.trailing.push(Trailing::Button {
            label: label.to_string(),
            done: done.to_string(),
            action: Rc::new(action),
        });
        self
    }

    pub fn icon_button(
        mut self,
        icon: Lucide,
        tooltip: &str,
        action: impl Fn(&mut App) + 'static,
    ) -> Row {
        self.trailing.push(Trailing::Icon {
            icon,
            tooltip: tooltip.to_string(),
            action: Rc::new(action),
        });
        self
    }

    pub fn on_activate(mut self, action: impl Fn(&mut App) + 'static) -> Row {
        self.activate = Some(Rc::new(action));
        self
    }
}

/// Why a section has nothing in it.
type Empty = (Lucide, &'static str, &'static str);

struct Section {
    title: String,
    rows: Vec<Row>,
    empty: Option<Empty>,
    /// A search box over this section's rows.
    search: Option<Entity<InputState>>,
}

/// The body of a list dialog.
pub struct List {
    sections: Vec<Section>,
    /// Buttons already pressed, as (row id, which button): each acknowledges
    /// its click once and then stays that way.
    pressed: HashSet<(String, usize)>,
    /// A small form under the lists, for adding to them.
    form: Option<Entity<Form>>,
    form_title: String,
    form_action: Option<Rc<dyn Fn(Vec<Value>, &mut App)>>,
    _subscriptions: Vec<Subscription>,
}

impl List {
    fn new() -> Self {
        List {
            sections: Vec::new(),
            pressed: HashSet::new(),
            form: None,
            form_title: String::new(),
            form_action: None,
            _subscriptions: Vec::new(),
        }
    }

    fn section(&mut self, title: &str, empty: Option<Empty>) -> usize {
        self.sections.push(Section {
            title: title.to_string(),
            rows: Vec::new(),
            empty,
            search: None,
        });
        self.sections.len() - 1
    }

    /// Puts a search box over a section. `on_search` gets the trimmed term as
    /// it is typed.
    fn searchable(
        &mut self,
        section: usize,
        placeholder: &str,
        on_search: impl Fn(String, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let on_search = Rc::new(on_search);
        let subscription = cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let term = input.read(cx).value().trim().to_string();
                let on_search = on_search.clone();
                cx.defer(move |cx| on_search(term, cx));
            }
        });
        self._subscriptions.push(subscription);
        if let Some(section) = self.sections.get_mut(section) {
            section.search = Some(input);
        }
    }

    /// What a section's search box says right now.
    fn term(&self, section: usize, cx: &App) -> String {
        self.sections
            .get(section)
            .and_then(|section| section.search.as_ref())
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default()
    }

    fn set_rows(&mut self, section: usize, rows: Vec<Row>, cx: &mut Context<Self>) {
        if let Some(section) = self.sections.get_mut(section) {
            // A refilled list is a new answer: its buttons are new buttons.
            let ids: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
            self.pressed.retain(|(id, _)| !ids.contains(id.as_str()));
            section.rows = rows;
        }
        cx.notify();
    }
}

impl Render for List {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut column = v_flex().gap_4();
        for (section_index, section) in self.sections.iter().enumerate() {
            let mut block = v_flex().gap_1p5();
            if !section.title.is_empty() {
                block = block.child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(section.title.clone()),
                );
            }
            if let Some(search) = &section.search {
                block = block.child(Input::new(search).prefix(Lucide::Search).cleanable(true));
            }

            let mut rows = v_flex()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .overflow_hidden();
            if section.rows.is_empty() {
                rows = rows.child(match section.empty {
                    Some((icon, title, description)) => div()
                        .h(px(160.))
                        .child(kit::empty_state(icon, title, description, cx))
                        .into_any_element(),
                    None => div().h(px(8.)).into_any_element(),
                });
            }
            for (row_index, row) in section.rows.iter().enumerate() {
                let mut line = h_flex()
                    .id(ElementId::Name(
                        format!("row-{section_index}-{}", row.id).into(),
                    ))
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .items_center()
                    .when(row_index > 0, |line| {
                        line.border_t_1().border_color(theme.border)
                    });
                line = line.child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(row.title.clone()))
                        .when(!row.subtitle.is_empty(), |text| {
                            text.child(
                                div()
                                    .text_xs()
                                    .line_clamp(2)
                                    .text_color(theme.muted_foreground)
                                    .child(row.subtitle.clone()),
                            )
                        })
                        .when(!row.body.is_empty(), |text| {
                            text.child(div().text_sm().child(row.body.clone()))
                        }),
                );
                for (index, trailing) in row.trailing.iter().enumerate() {
                    line = line.child(match trailing {
                        Trailing::Label(text) => div()
                            .flex_none()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(text.clone())
                            .into_any_element(),
                        Trailing::Button {
                            label,
                            done,
                            action,
                        } => {
                            let key = (row.id.clone(), index);
                            let pressed = self.pressed.contains(&key);
                            let action = action.clone();
                            Button::new(("button", index))
                                .label(if pressed { done.clone() } else { label.clone() })
                                .small()
                                .primary()
                                .disabled(pressed)
                                .on_click(cx.listener(move |list, _, _, cx| {
                                    list.pressed.insert(key.clone());
                                    cx.notify();
                                    run_later(&action, cx);
                                }))
                                .into_any_element()
                        }
                        Trailing::Icon {
                            icon,
                            tooltip,
                            action,
                        } => {
                            let key = (row.id.clone(), index);
                            let action = action.clone();
                            kit::icon_button(("icon", index), icon.clone(), tooltip.clone())
                                // It only stops a second click on the way to
                                // the server; the row stays until the caller
                                // sends the list back.
                                .disabled(self.pressed.contains(&key))
                                .on_click(cx.listener(move |list, _, _, cx| {
                                    list.pressed.insert(key.clone());
                                    cx.notify();
                                    run_later(&action, cx);
                                }))
                                .into_any_element()
                        }
                    });
                }
                if let Some(action) = row.activate.clone() {
                    line = line
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.list_hover))
                        .on_click(move |_, _, cx| run_later(&action, cx));
                }
                rows = rows.child(line);
            }
            column = column.child(block.child(rows));
        }

        if let (Some(form), Some(action)) = (self.form.clone(), self.form_action.clone()) {
            let submit = form.clone();
            column = column.child(
                v_flex()
                    .gap_1p5()
                    .child(
                        h_flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.form_title.clone()),
                            )
                            .child(Button::new("add").label("Add").small().primary().on_click(
                                move |_, window, cx| {
                                    let values = submit.read(cx).values(cx);
                                    // Ready for the next one.
                                    submit.update(cx, |form, cx| {
                                        for slot in &form.slots {
                                            if let Slot::Text { input, .. } = slot {
                                                input.update(cx, |input, cx| {
                                                    input.set_value("", window, cx)
                                                });
                                            }
                                        }
                                    });
                                    let action = action.clone();
                                    cx.defer(move |cx| action(values, cx));
                                },
                            )),
                    )
                    .child(form),
            );
        }

        div()
            .id("list-dialog")
            .max_h(px(520.))
            .overflow_y_scroll()
            .child(column)
    }
}

/// Opens a list dialog around `build`'s sections and returns its body, which
/// is how rows get in afterwards.
fn open_list(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    build: impl FnOnce(&mut List, &mut Window, &mut Context<List>),
) -> Option<Entity<List>> {
    let title: SharedString = title.to_string().into();
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let mut list = List::new();
            build(&mut list, window, cx);
            list
        });
        let body = view.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(title.clone()).w(px(520.)).child(body.clone())
        });
        view
    })
}

/// Shows rows that are already known: an edit history, what is scheduled.
pub fn show_rows(ui: &Rc<Ui>, cx: &mut App, title: &str, rows: Vec<Row>, empty: Option<Empty>) {
    open_list(ui, cx, title, move |list, _, cx| {
        let section = list.section("", empty);
        list.set_rows(section, rows, cx);
    });
}

/// Create a channel. Answers (display_name, url_name, purpose, private).
pub fn create_channel(
    ui: &Rc<Ui>,
    cx: &mut App,
    on_create: impl Fn(String, String, String, bool, &mut App) + 'static,
) {
    let on_create = Rc::new(on_create);
    let mut spec = FormSpec::new("Create a channel", "Create");
    // The URL follows the name until someone edits it themselves.
    spec.follow = Some(|name| slugify(name));
    form(
        ui,
        cx,
        spec,
        vec![
            Field::text("Name", ""),
            Field::text("URL name", ""),
            Field::text("Purpose (optional)", ""),
            Field::switch("Private channel", "Only invited people can find it", false),
        ],
        move |values, cx| {
            let display_name = values[0].text().trim().to_string();
            // A channel with no name is the one mistake worth blocking
            // outright.
            if display_name.is_empty() {
                return false;
            }
            let mut url_name = values[1].text().trim().to_string();
            // Emptied by hand, or a name with nothing ASCII in it to slug.
            if url_name.is_empty() {
                url_name = slugify(&display_name);
            }
            let purpose = values[2].text().trim().to_string();
            let private = values[3].bool();
            let on_create = on_create.clone();
            cx.defer(move |cx| on_create(display_name, url_name, purpose, private, cx));
            true
        },
    );
}

/// Browse and join channels.
///
/// The dialog outlives this call, so the handle keeps the list around: the
/// caller searches the server on `on_search` and pours the answer back in
/// through [`Self::set_results`].
pub struct ChannelBrowser {
    list: Option<Entity<List>>,
    on_join: Rc<dyn Fn(String, &mut App)>,
}

impl ChannelBrowser {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        on_search: impl Fn(String, &mut App) + 'static,
        on_join: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let list = open_list(ui, cx, "Browse channels", move |list, window, cx| {
            let section = list.section(
                "",
                Some((Lucide::Hash, "No channels found", "Try a different name.")),
            );
            list.searchable(
                section,
                "Search channels",
                move |term, cx| typed(term, cx),
                window,
                cx,
            );
        });
        // Ask once on open: a browser that is empty until you type is a search
        // box, and browsing is the point.
        cx.defer(move |cx| on_search(String::new(), cx));
        ChannelBrowser {
            list,
            on_join: Rc::new(on_join),
        }
    }

    /// `channels` is (id, display_name, purpose, already_member), answering
    /// the search for `term`.
    pub fn set_results(
        &self,
        term: &str,
        channels: Vec<(String, String, String, bool)>,
        cx: &mut App,
    ) {
        let Some(list) = &self.list else { return };
        // A search is a round trip, and a narrow term can come back before
        // the broad one typed before it — without this, the broad answer
        // lands last and replaces the filtered list.
        if list.read(cx).term(0, cx) != term.trim() {
            return; // Answered a question nobody is asking any more.
        }
        let rows = channels
            .into_iter()
            .map(|(id, display_name, purpose, already_member)| {
                let row = Row::new(id.clone(), display_name).subtitle(purpose);
                if already_member {
                    row.label("Joined")
                } else {
                    // The dialog stays open so you can join several at once.
                    let on_join = self.on_join.clone();
                    row.button("Join", "Joined", move |cx| on_join(id.clone(), cx))
                }
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}

/// Pick one thing out of a directory too large to list: people, channels.
/// The caller answers each search through [`Self::set_results`].
pub struct Picker {
    list: Option<Entity<List>>,
    ui: Rc<Ui>,
    on_pick: Rc<dyn Fn(String, &mut App)>,
}

impl Picker {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        title: &str,
        placeholder: &'static str,
        on_search: impl Fn(String, &mut App) + 'static,
        on_pick: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let list = open_list(ui, cx, title, move |list, window, cx| {
            let section = list.section(
                "",
                Some((Lucide::Search, "Nothing found", "Try a different name.")),
            );
            list.searchable(section, placeholder, move |term, cx| typed(term, cx), window, cx);
        });
        // Typing is the whole point of this dialog, so the box has the focus
        // from the start — after the dialog has taken it for itself.
        if let Some(input) = list
            .as_ref()
            .and_then(|list| list.read(cx).sections.first()?.search.clone())
        {
            let ui = ui.clone();
            cx.defer(move |cx| {
                ui.with_window(cx, |window, cx| {
                    input.update(cx, |input, cx| input.focus(window, cx))
                });
            });
        }
        // Something to choose from before a letter is typed.
        cx.defer(move |cx| on_search(String::new(), cx));
        Picker {
            list,
            ui: ui.clone(),
            on_pick: Rc::new(on_pick),
        }
    }

    /// `entries` is (id, name, subtitle), answering the search for `term`.
    /// Picking one answers with its id and puts the dialog away.
    pub fn set_results(&self, term: &str, entries: Vec<(String, String, String)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        // The same guard as the channel browser: an answer to a question
        // nobody is asking any more must not replace the current one.
        if list.read(cx).term(0, cx) != term.trim() {
            return;
        }
        let rows = entries
            .into_iter()
            .map(|(id, name, subtitle)| {
                let (ui, on_pick) = (self.ui.clone(), self.on_pick.clone());
                Row::new(id.clone(), name)
                    .subtitle(subtitle)
                    .on_activate(move |cx| {
                        on_pick(id.clone(), cx);
                        ui.with_window(cx, |window, cx| window.close_dialog(cx));
                    })
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}

/// Per-channel notification settings. `current` is (desktop_level,
/// mark_unread_all, ignore_channel_mentions) where desktop_level is one of
/// "default"|"all"|"mention"|"none". Answers the same triple.
pub fn channel_notifications(
    ui: &Rc<Ui>,
    cx: &mut App,
    channel_name: &str,
    current: (String, bool, bool),
    on_save: impl Fn(String, bool, bool, &mut App) + 'static,
) {
    let (level, mark_unread_all, ignore_channel_mentions) = current;
    let on_save = Rc::new(on_save);
    form(
        ui,
        cx,
        FormSpec::new(&format!("Notifications for {channel_name}"), "Save"),
        vec![
            Field::choice("Desktop notifications", &CHANNEL_LEVELS, &level),
            Field::switch(
                "Mark as unread",
                "For every message, not only mentions",
                mark_unread_all,
            ),
            Field::switch(
                "Ignore @channel, @here and @all",
                "",
                ignore_channel_mentions,
            ),
        ],
        move |values, cx| {
            let (level, unread, ignore) = (values[0].text(), values[1].bool(), values[2].bool());
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(level, unread, ignore, cx));
            true
        },
    );
}

/// Account-wide notification settings: desktop level, sound on/off, mention
/// keywords (comma separated), and whether first name counts.
pub fn account_notifications(
    ui: &Rc<Ui>,
    cx: &mut App,
    current: (String, bool, String, bool),
    on_save: impl Fn(String, bool, String, bool, &mut App) + 'static,
) {
    let (level, sound, keywords, first_name) = current;
    let on_save = Rc::new(on_save);
    let mut spec = FormSpec::new("Notifications", "Save");
    spec.note = "Keywords are separated by commas and ignore case.".to_string();
    form(
        ui,
        cx,
        spec,
        vec![
            Field::choice("Desktop notifications", &ACCOUNT_LEVELS, &level),
            Field::switch("Notification sound", "", sound),
            Field::text("Keywords that mention me", &keywords),
            Field::switch(
                "My first name",
                "Notify me when someone types it",
                first_name,
            ),
        ],
        move |values, cx| {
            let level = values[0].text();
            let sound = values[1].bool();
            let keywords = values[2].text().trim().to_string();
            let first_name = values[3].bool();
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(level, sound, keywords, first_name, cx));
            true
        },
    );
}

/// Confirm leaving a channel.
pub fn confirm_leave(
    ui: &Rc<Ui>,
    cx: &mut App,
    channel_name: &str,
    on_leave: impl Fn(&mut App) + 'static,
) {
    confirm(
        ui,
        cx,
        &format!("Leave {channel_name}?"),
        "You will stop receiving its messages. You can join a public channel again later.",
        "Leave",
        true,
        on_leave,
    );
}

/// Schedule a message: answers a Unix millisecond timestamp.
pub fn schedule_message(ui: &Rc<Ui>, cx: &mut App, on_schedule: impl Fn(i64, &mut App) + 'static) {
    let on_schedule = Rc::new(on_schedule);
    // The usual answer, filled in: tomorrow at nine. Typing a date is quicker
    // than paging a calendar to it, and both fields say what they expect.
    let tomorrow = Local::now().date_naive() + Days::new(1);
    let mut spec = FormSpec::new("Schedule message", "Schedule");
    spec.note = "The server keeps it and posts it for you, even with this app closed.".to_string();
    form(
        ui,
        cx,
        spec,
        vec![
            Field::choice(
                "When",
                &[
                    ("tomorrow", "Tomorrow morning"),
                    ("monday", "Monday morning"),
                    ("custom", "The date and time below"),
                ],
                "tomorrow",
            ),
            Field::text("Date (YYYY-MM-DD)", &tomorrow.format("%Y-%m-%d").to_string()),
            Field::text("Time (HH:MM)", "09:00"),
        ],
        move |values, cx| {
            let millis = match values[0].text().as_str() {
                "tomorrow" => morning_in(1),
                "monday" => next_monday_morning(),
                _ => local_millis(&values[1].text(), &values[2].text()),
            };
            // A date that does not read as one keeps the dialog open.
            let Some(millis) = millis else { return false };
            let on_schedule = on_schedule.clone();
            cx.defer(move |cx| on_schedule(millis, cx));
            true
        },
    );
}

/// Set a reminder about a post: answers a Unix millisecond timestamp.
pub fn post_reminder(ui: &Rc<Ui>, cx: &mut App, on_remind: impl Fn(i64, &mut App) + 'static) {
    let on_remind = Rc::new(on_remind);
    let option = |when: fn() -> Option<i64>| -> Callback {
        let on_remind = on_remind.clone();
        Rc::new(move |cx| {
            // A reminder we cannot place is simply not set.
            if let Some(millis) = when() {
                on_remind(millis, cx);
            }
        })
    };
    choose(
        ui,
        cx,
        "Remind me about this",
        "The system bot will send you the message again.",
        vec![
            ("In 30 minutes", option(|| Some(from_now(30)))),
            ("In 1 hour", option(|| Some(from_now(60)))),
            ("In 2 hours", option(|| Some(from_now(120)))),
            ("Tomorrow morning", option(|| morning_in(1))),
        ],
    );
}

/// A footer with Cancel and one confirming button, which is what nearly every
/// dialog here ends in.
pub(super) fn footer(ok: impl Into<SharedString>) -> DialogFooter {
    DialogFooter::new()
        .child(DialogClose::new().child(Button::new("cancel").label("Cancel")))
        .child(DialogAction::new().child(Button::new("ok").label(ok).primary()))
}

/// Where `value` sits in `levels`, falling back to the first entry: the server
/// can send a level we do not offer, and there is nothing better to show.
fn level_index(levels: &[(&str, &str)], value: &str) -> usize {
    levels.iter().position(|(id, _)| *id == value).unwrap_or(0)
}

/// Now plus some minutes, in Unix milliseconds.
fn from_now(minutes: i64) -> i64 {
    (Local::now() + chrono::Duration::minutes(minutes)).timestamp_millis()
}

/// A local wall-clock moment in Unix milliseconds. `None` for a moment that
/// does not exist — the hour a clock change skips.
fn local_moment(day: NaiveDate, time: NaiveTime) -> Option<i64> {
    Local
        .from_local_datetime(&day.and_time(time))
        .earliest()
        .map(|moment| moment.timestamp_millis())
}

/// A typed date and time, in Unix milliseconds.
fn local_millis(date: &str, time: &str) -> Option<i64> {
    let day = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    local_moment(day, time)
}

/// 9am local time, `days` days from today.
fn morning_in(days: u64) -> Option<i64> {
    let day = Local::now().date_naive().checked_add_days(Days::new(days))?;
    local_moment(day, NaiveTime::from_hms_opt(9, 0, 0)?)
}

fn next_monday_morning() -> Option<i64> {
    let today = Local::now().date_naive();
    morning_in(days_until_next_monday(today.weekday().number_from_monday()) as u64)
}

/// Days from `day_of_week` (1 = Monday … 7 = Sunday) to the next Monday.
/// Always in the future: asked on a Monday it means the next one, because
/// "Monday morning" is never the morning you are already in.
fn days_until_next_monday(day_of_week: u32) -> u32 {
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

/// Who is in a channel, with a search for adding more.
///
/// Like [`ChannelBrowser`] the dialog outlives this call: the caller answers
/// `on_search` through [`Self::set_candidates`], and fills the roster with
/// [`Self::set_members`].
pub struct MemberList {
    list: Option<Entity<List>>,
    on_add: Rc<dyn Fn(String, &mut App)>,
    on_remove: Rc<dyn Fn(String, &mut App)>,
}

impl MemberList {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        channel_name: &str,
        on_search: impl Fn(String, &mut App) + 'static,
        on_add: impl Fn(String, &mut App) + 'static,
        on_remove: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let list = open_list(
            ui,
            cx,
            &format!("Members of {channel_name}"),
            move |list, window, cx| {
                list.section(
                    "Members",
                    Some((Lucide::Users, "Nobody here yet", "")),
                );
                let candidates = list.section(
                    "Add people",
                    Some((Lucide::Search, "No matching people", "")),
                );
                list.searchable(
                    candidates,
                    "Search people",
                    move |term, cx| typed(term, cx),
                    window,
                    cx,
                );
            },
        );
        // Ask once on open, as the channel browser does: the people you want
        // to add are usually the ones the server would have listed anyway.
        cx.defer(move |cx| on_search(String::new(), cx));
        MemberList {
            list,
            on_add: Rc::new(on_add),
            on_remove: Rc::new(on_remove),
        }
    }

    /// `members` is (user_id, display_name, @username, is_admin).
    pub fn set_members(&self, members: Vec<(String, String, String, bool)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        let rows = members
            .into_iter()
            .map(|(id, display_name, username, is_admin)| {
                let row = Row::new(id.clone(), display_name).subtitle(username);
                if is_admin {
                    // Removing an admin needs permissions we cannot check from
                    // here, so the row explains itself instead of offering a
                    // button the server would refuse.
                    row.label("Admin")
                } else {
                    let on_remove = self.on_remove.clone();
                    row.icon_button(Lucide::UserMinus, "Remove from channel", move |cx| {
                        on_remove(id.clone(), cx)
                    })
                }
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }

    /// `users` is (user_id, display_name, @username), answering the search
    /// for `term`.
    pub fn set_candidates(&self, term: &str, users: Vec<(String, String, String)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        if list.read(cx).term(1, cx) != term.trim() {
            return; // Answered a question nobody is asking any more.
        }
        let rows = users
            .into_iter()
            .map(|(id, display_name, username)| {
                let on_add = self.on_add.clone();
                Row::new(id.clone(), display_name)
                    .subtitle(username)
                    .button("Add", "Added", move |cx| on_add(id.clone(), cx))
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(1, rows, cx));
    }
}

/// Teams you could join, filled in by [`Self::set_teams`].
///
/// There is no search: an account sees the teams it is allowed to join and
/// that list is short enough to read.
pub struct TeamBrowser {
    list: Option<Entity<List>>,
    on_join: Rc<dyn Fn(String, &mut App)>,
}

impl TeamBrowser {
    pub fn present(ui: &Rc<Ui>, cx: &mut App, on_join: impl Fn(String, &mut App) + 'static) -> Self {
        let list = open_list(ui, cx, "Browse teams", |list, _, _| {
            list.section("", Some((Lucide::Users, "No teams to join", "")));
        });
        TeamBrowser {
            list,
            on_join: Rc::new(on_join),
        }
    }

    /// `teams` is (id, display_name, description, already_member).
    pub fn set_teams(&self, teams: Vec<(String, String, String, bool)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        let rows = teams
            .into_iter()
            .map(|(id, display_name, description, already_member)| {
                let row = Row::new(id.clone(), display_name).subtitle(description);
                if already_member {
                    row.label("Joined")
                } else {
                    let on_join = self.on_join.clone();
                    row.button("Join", "Joined", move |cx| on_join(id.clone(), cx))
                }
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}

/// A channel's bookmarks, filled in by [`Self::set_bookmarks`].
///
/// Adding one is a form in the same dialog rather than a second one: it is
/// two fields, and you usually add several in a row.
pub struct BookmarkList {
    list: Option<Entity<List>>,
    on_open: Rc<dyn Fn(String, &mut App)>,
    on_delete: Rc<dyn Fn(String, &mut App)>,
}

impl BookmarkList {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        on_add: impl Fn(String, String, &mut App) + 'static,
        on_open: impl Fn(String, &mut App) + 'static,
        on_delete: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let list = open_list(ui, cx, "Bookmarks", move |list, window, cx| {
            list.section(
                "Bookmarks",
                Some((Lucide::Bookmark, "No bookmarks yet", "")),
            );
            list.form_title = "Add a bookmark".to_string();
            list.form = Some(cx.new(|cx| {
                Form::new(
                    vec![Field::text("Name (optional)", ""), Field::text("Link", "")],
                    window,
                    cx,
                )
            }));
            list.form_action = Some(Rc::new(move |values, cx| {
                let name = values[0].text().trim().to_string();
                let link = values[1].text().trim().to_string();
                // A bookmark without a link is nothing to save; the name can
                // be filled in from the link by whoever handles this.
                if !link.is_empty() {
                    on_add(name, link, cx);
                }
            }));
        });
        BookmarkList {
            list,
            on_open: Rc::new(on_open),
            on_delete: Rc::new(on_delete),
        }
    }

    /// `bookmarks` is (id, display_name, link_url).
    pub fn set_bookmarks(&self, bookmarks: Vec<(String, String, String)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        let rows = bookmarks
            .into_iter()
            .map(|(id, display_name, link_url)| {
                let on_open = self.on_open.clone();
                let on_delete = self.on_delete.clone();
                let link = link_url.clone();
                // The whole row opens it, which is what a bookmark is for; the
                // button beside it is the only other thing you can do.
                Row::new(id.clone(), display_name)
                    .subtitle(link_url)
                    .on_activate(move |cx| on_open(link.clone(), cx))
                    .icon_button(Lucide::Trash, "Remove bookmark", move |cx| {
                        on_delete(id.clone(), cx)
                    })
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}

/// Rename a channel, or change its topic. Answers (display_name, header).
pub fn edit_channel(
    ui: &Rc<Ui>,
    cx: &mut App,
    current: (String, String),
    on_save: impl Fn(String, String, &mut App) + 'static,
) {
    let on_save = Rc::new(on_save);
    form(
        ui,
        cx,
        FormSpec::new("Channel details", "Save"),
        vec![
            Field::text("Name", &current.0),
            Field::text("Topic", &current.1),
        ],
        move |values, cx| {
            let name = values[0].text().trim().to_string();
            let header = values[1].text();
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(name, header, cx));
            true
        },
    );
}

/// Confirm archiving a channel. Archiving hides it for everyone, so it asks
/// in the same words the other clients use.
pub fn confirm_archive(
    ui: &Rc<Ui>,
    cx: &mut App,
    channel_name: &str,
    on_archive: impl Fn(&mut App) + 'static,
) {
    confirm(
        ui,
        cx,
        "Archive this channel?",
        &format!(
            "{channel_name} will be hidden for everyone. Its messages are kept, and an admin can \
             bring it back."
        ),
        "Archive",
        true,
        on_archive,
    );
}

/// Name a category, whether new or being renamed.
pub fn name_category(
    ui: &Rc<Ui>,
    cx: &mut App,
    heading: &str,
    current: &str,
    on_save: impl Fn(String, &mut App) + 'static,
) {
    let on_save = Rc::new(on_save);
    form(
        ui,
        cx,
        FormSpec::new(heading, "Save"),
        vec![Field::text("Name", current)],
        move |values, cx| {
            let name = values[0].text().trim().to_string();
            if name.is_empty() {
                return false;
            }
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(name, cx));
            true
        },
    );
}

/// Confirm removing someone from a channel.
pub fn confirm_remove_member(
    ui: &Rc<Ui>,
    cx: &mut App,
    name: &str,
    on_remove: impl Fn(&mut App) + 'static,
) {
    confirm(
        ui,
        cx,
        &format!("Remove {name}?"),
        "They will stop receiving its messages. You can add them back later.",
        "Remove",
        true,
        on_remove,
    );
}

/// How many emoji the picker shows at once. Filling the whole table costs a
/// few thousand buttons, so it is a page of whatever the search ranks first.
const EMOJI_PAGE: usize = 120;

/// The full emoji table, under a search box.
struct EmojiPicker {
    ui: Rc<Ui>,
    search: Entity<InputState>,
    on_pick: Rc<dyn Fn(String, &mut App)>,
    _subscription: Subscription,
}

impl Render for EmojiPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let term = self.search.read(cx).value().to_string();
        let found = crate::emoji::search(&term, &self.ui.state.borrow().custom_emoji, EMOJI_PAGE);
        let mut grid = h_flex().flex_wrap().gap_0p5();
        for found in found {
            let name = found.name().to_string();
            let on_pick = self.on_pick.clone();
            let button = Button::new(gpui_kit::ElementId::Name(format!("emoji-{name}").into()))
                .ghost()
                .tooltip(format!(":{name}:"));
            let button = match &found {
                crate::emoji::Found::Unicode(_, glyph) => button.label(*glyph),
                // The server's own: its picture, the size of the glyphs
                // beside it.
                crate::emoji::Found::Custom(name) => {
                    button.child(super::message::emoji_element(&self.ui, name, 20.))
                }
            };
            grid = grid.child(button.on_click(move |_, window, cx| {
                window.close_dialog(cx);
                let on_pick = on_pick.clone();
                let name = name.clone();
                cx.defer(move |cx| on_pick(name, cx));
            }));
        }
        v_flex()
            .gap_2()
            .child(Input::new(&self.search).prefix(Lucide::Search).cleanable(true))
            .child(
                div()
                    .id("emoji-grid")
                    .h(px(260.))
                    .overflow_y_scroll()
                    .child(grid),
            )
    }
}

/// Asks which emoji, from the whole table. Answers its shortcode.
pub fn pick_emoji(ui: &Rc<Ui>, cx: &mut App, on_pick: impl Fn(String, &mut App) + 'static) {
    let on_pick: Rc<dyn Fn(String, &mut App)> = Rc::new(on_pick);
    let ui = ui.clone();
    ui.clone().with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search emoji"));
            let subscription = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            });
            EmojiPicker {
                ui: ui.clone(),
                search,
                on_pick,
                _subscription: subscription,
            }
        });
        let search = view.read(cx).search.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Add reaction").child(view.clone())
        });
        search.update(cx, |search, cx| search.focus(window, cx));
    });
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

    #[test]
    fn a_typed_date_and_time_is_that_local_moment() {
        let millis = local_millis("2026-03-10", "14:30").expect("a real moment");
        let back = Local.timestamp_millis_opt(millis).single().unwrap();
        assert_eq!(
            back.format("%Y-%m-%d %H:%M").to_string(),
            "2026-03-10 14:30"
        );
        // Surrounding spaces are how a pasted value arrives.
        assert_eq!(local_millis(" 2026-03-10 ", " 14:30 "), Some(millis));

        // Not a date, not a time, and a day that does not exist.
        assert_eq!(local_millis("tomorrow", "14:30"), None);
        assert_eq!(local_millis("2026-03-10", "half past two"), None);
        assert_eq!(local_millis("2026-02-30", "09:00"), None);
    }

    #[test]
    fn the_usual_times_are_in_the_morning_and_ahead() {
        let now = Local::now().timestamp_millis();
        for millis in [morning_in(1).unwrap(), next_monday_morning().unwrap()] {
            assert!(millis > now);
            let at = Local.timestamp_millis_opt(millis).single().unwrap();
            assert_eq!(at.format("%H:%M").to_string(), "09:00");
        }
        assert!(from_now(30) > now);
    }

    #[test]
    fn a_form_value_reads_as_what_it_holds() {
        assert_eq!(Value::Text("a".into()).text(), "a");
        assert_eq!(Value::Choice("all".into()).text(), "all");
        assert!(Value::Bool(true).bool());
        assert!(!Value::Text("true".into()).bool());
    }
}
