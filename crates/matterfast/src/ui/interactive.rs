//! The form an integration asks for: Mattermost interactive dialogs.
//!
//! Like [`super::dialogs`], nothing here talks to the server or to
//! [`crate::state`]. [`present`] renders a [`Dialog`] and hands the filled-in
//! values to a callback; the caller POSTs them to
//! `/api/v4/actions/dialogs/submit`.

use gpui_adaptive::{
    ActiveColors as _, Choice, ChoiceOption, ChoiceState, Command, FieldEvent, Menu, Purpose,
    TextField, TextFieldState, Toggle, prim,
};
use std::collections::HashMap;
use std::rc::Rc;

use gpui_adaptive::prim::Scrolls as _;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, Subscription, Window};
use mattermost_api::models::dialog::{Dialog, DialogElement};
use serde_json::Value;

use super::Ui;

/// What a field is drawn as, and what it currently holds.
enum Kind {
    /// A single-line field. Also where a `select` backed by `users` or
    /// `channels` lands: those carry no options in the payload, the client is
    /// expected to search the server for them, and this file has no client to
    /// search with.
    Text(Entity<TextFieldState>),
    Area(Entity<TextFieldState>),
    /// A `select` whose options came in the payload.
    Select(usize),
    /// A `bool`. The default arrives as the *string* `"true"`, and the answer
    /// goes back as a real JSON boolean — the server's `map[string]any` keeps
    /// the type.
    Bool(bool),
    /// The chosen option's index is its key.
    Radio(Entity<ChoiceState>),
}

struct Element {
    spec: DialogElement,
    kind: Kind,
}

struct Interactive {
    introduction: String,
    elements: Vec<Element>,
    /// Set once the form has been handed in, so closing the dialog is not
    /// also reported as a cancel.
    submitted: bool,
    _subscriptions: Vec<Subscription>,
}

/// Which of an element's options is the default: the one named, or the first.
/// Nothing chosen would submit an empty string for a field the integration
/// expects one of its own values in.
fn default_option(element: &DialogElement) -> usize {
    element
        .options
        .iter()
        .position(|option| option.value == element.default)
        .unwrap_or(0)
}

/// The phone's keyboard that suits a text field, by the subtype the
/// integration gave it: digits for a number, no capitals or corrections for
/// an address or a secret.
fn purpose(element: &DialogElement) -> Purpose {
    match element.subtype.as_str() {
        "number" => Purpose::Number,
        "email" => Purpose::Login,
        "url" => Purpose::Address,
        "password" => Purpose::Password,
        _ => Purpose::Plain,
    }
}

/// Required-ness and the length bounds for a text field holding `len`
/// characters.
fn length_ok(element: &DialogElement, len: usize) -> bool {
    let min = element.min_length.max(0) as usize;
    let max = element.max_length.max(0) as usize;
    if len == 0 {
        return element.optional;
    }
    (min == 0 || len >= min) && (max == 0 || len <= max)
}

impl Interactive {
    fn new(dialog: &Dialog, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        let elements = dialog
            .elements
            .iter()
            .map(|element| {
                // The length bounds are checked as it is typed, so the form
                // has to be drawn again to say so.
                let mut text = |field: Entity<TextFieldState>, window: &mut Window, cx: &mut Context<Self>| {
                    field.update(cx, |field, cx| {
                        field.set_value(element.default.clone(), window, cx);
                        field.set_placeholder(element.placeholder.clone(), window, cx);
                    });
                    subscriptions.push(cx.subscribe(&field, |_, _, event: &FieldEvent, cx| {
                        if *event == FieldEvent::Change {
                            cx.notify();
                        }
                    }));
                    field
                };
                let kind = match element.element_type.as_str() {
                    "textarea" => {
                        let field = cx.new(|cx| TextFieldState::multiline(Purpose::Message, 4, 8, window, cx));
                        Kind::Area(text(field, window, cx))
                    }
                    "radio" if !element.options.is_empty() => {
                        let options = element
                            .options
                            .iter()
                            .enumerate()
                            .map(|(index, option)| ChoiceOption::new(index.to_string(), option.text.clone()))
                            .collect();
                        let selected = Some(default_option(element).to_string().into());
                        Kind::Radio(cx.new(|cx| ChoiceState::new(options, selected, window, cx)))
                    }
                    "select" if !element.options.is_empty() => {
                        Kind::Select(default_option(element))
                    }
                    "bool" => Kind::Bool(element.default == "true"),
                    // Everything else — `text`, a `select` whose options live
                    // on the server, and any type this client does not know
                    // yet — is a text field. An unknown type degrading to text
                    // is better than a form with a hole in it. A password is
                    // hidden even though the payload treats it as text.
                    _ => {
                        let field = cx.new(|cx| TextFieldState::new(purpose(element), window, cx));
                        Kind::Text(text(field, window, cx))
                    }
                };
                Element {
                    spec: element.clone(),
                    kind,
                }
            })
            .collect();
        Interactive {
            introduction: dialog.introduction_text.clone(),
            elements,
            submitted: false,
            _subscriptions: subscriptions,
        }
    }

    /// Whether one field is currently fit to submit.
    fn field_ok(element: &Element, cx: &App) -> bool {
        match &element.kind {
            Kind::Text(field) | Kind::Area(field) => {
                length_ok(&element.spec, field.read(cx).value(cx).chars().count())
            }
            // A select or a radio group always has something chosen, and a
            // switch is always one way or the other.
            Kind::Select(_) | Kind::Bool(_) | Kind::Radio(_) => true,
        }
    }

    fn valid(&self, cx: &App) -> bool {
        self.elements
            .iter()
            .all(|element| Self::field_ok(element, cx))
    }

    /// Every field's answer, in the JSON shape the integration expects, keyed
    /// by element name.
    fn submission(&self, cx: &App) -> HashMap<String, Value> {
        self.elements
            .iter()
            .map(|element| {
                let option = |index: &usize| {
                    Value::String(
                        element
                            .spec
                            .options
                            .get(*index)
                            .map(|option| option.value.clone())
                            .unwrap_or_default(),
                    )
                };
                let value = match &element.kind {
                    Kind::Text(field) | Kind::Area(field) => {
                        Value::String(field.read(cx).value(cx).to_string())
                    }
                    // ponytail: a `multiselect` select submits only the one
                    // option; swap in a check-list if an integration that
                    // uses it turns up.
                    Kind::Select(index) => option(index),
                    Kind::Radio(choice) => {
                        let index = choice.read(cx).selected().and_then(|key| key.parse().ok());
                        option(&index.unwrap_or(0))
                    }
                    Kind::Bool(on) => Value::Bool(*on),
                };
                (element.spec.name.clone(), value)
            })
            .collect()
    }
}

impl Render for Interactive {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let muted = colors.muted;
        let mut column = prim::column().gap_3();
        if !self.introduction.is_empty() {
            // Markdown on the wire, and drawn as such.
            column = column.child(super::message::markdown(
                "introduction",
                crate::markdown::prepare_with(
                    &self.introduction,
                    &|_| None,
                    crate::markdown::Sigil::Keep,
                ),
            ));
        }

        for (index, element) in self.elements.iter().enumerate() {
            let ok = Self::field_ok(element, cx);
            let label = element.spec.display_name.clone();
            let help = (!element.spec.help_text.is_empty())
                .then(|| div().text_xs().text_color(muted).child(element.spec.help_text.clone()));

            let field = match &element.kind {
                Kind::Text(field) | Kind::Area(field) => {
                    TextField::new(field).label(label).into_any_element()
                }
                Kind::Bool(on) => {
                    let entity = cx.entity().downgrade();
                    Toggle::new(("bool", index), label, *on)
                        .on_change(move |on, _, cx| {
                            let _ = entity.update(cx, |form, cx| {
                                if let Some(Element { kind: Kind::Bool(held), .. }) =
                                    form.elements.get_mut(index)
                                {
                                    *held = on;
                                }
                                cx.notify();
                            });
                        })
                        .into_any_element()
                }
                Kind::Select(selected) => {
                    let entity = cx.entity();
                    let options = element.spec.options.clone();
                    let current = *selected;
                    prim::row()
                        .gap_3()
                        .items_center()
                        .child(div().flex_1().min_w_0().text_sm().child(label))
                        .child(
                            Menu::new(Command::new(
                                ("select", index),
                                options.get(current).map(|option| option.text.clone()).unwrap_or_default(),
                            ))
                            .choice()
                            .commands(options.iter().enumerate().map(|(choice, option)| {
                                let entity = entity.clone();
                                Command::new(("option", choice), option.text.clone())
                                    .checked(choice == current)
                                    .on_trigger(move |_, cx| {
                                        entity.update(cx, |form, cx| {
                                            if let Some(Element { kind: Kind::Select(selected), .. }) =
                                                form.elements.get_mut(index)
                                            {
                                                *selected = choice;
                                            }
                                            cx.notify();
                                        });
                                    })
                            })),
                        )
                        .into_any_element()
                }
                Kind::Radio(choice) => {
                    Choice::new(choice).id(("radio", index)).label(label).into_any_element()
                }
            };

            column = column.child(
                prim::column()
                    .gap_1()
                    .child(field)
                    .when_some(help, |column, help| column.child(help))
                    // Said under the field rather than in an error after
                    // submitting: a form that says what it wants needs no
                    // error message.
                    .when(!ok, |column| {
                        column.child(
                            div()
                                .text_xs()
                                .text_color(colors.danger)
                                .child(requirement(&element.spec)),
                        )
                    }),
            );
        }

        div()
            .id("interactive")
            .max_h(px(520.))
            .scrolls_y()
            .child(column)
    }
}

/// What a text field is asking for, in words.
fn requirement(element: &DialogElement) -> String {
    let min = element.min_length.max(0);
    let max = element.max_length.max(0);
    match (min, max) {
        (0, 0) => "Required".to_string(),
        (min, 0) => format!("At least {min} characters"),
        (0, max) => format!("At most {max} characters"),
        (min, max) => format!("Between {min} and {max} characters"),
    }
}

/// Shows a dialog form. `on_submit` gets the values keyed by element name;
/// `on_cancel` fires when it is dismissed.
pub fn present(
    ui: &Rc<Ui>,
    cx: &mut App,
    dialog: &Dialog,
    on_submit: impl Fn(HashMap<String, Value>, &mut App) + 'static,
    on_cancel: impl Fn(&mut App) + 'static,
) {
    let on_submit = Rc::new(on_submit);
    let on_cancel = Rc::new(on_cancel);
    let dialog = dialog.clone();
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| Interactive::new(&dialog, window, cx));
        let title = gpui_kit::SharedString::from(dialog.title.clone());
        let submit_label = gpui_kit::SharedString::from(if dialog.submit_label.is_empty() {
            "Submit".to_string()
        } else {
            dialog.submit_label.clone()
        });
        // Submitting and dismissing both put the form away; what is left of
        // it says which of the two it was.
        cx.observe_release(&view, move |form, cx| {
            if !form.submitted {
                let on_cancel = on_cancel.clone();
                cx.defer(move |cx| on_cancel(cx));
            }
        })
        .detach();
        let submit = view.clone();
        gpui_adaptive::Form::new(title, view)
            .width(gpui_kit::rems(30.))
            .commit(submit_label, move |_, cx| {
                // The fields say what is wrong with them; an unfit form
                // simply stays up.
                if !submit.read(cx).valid(cx) {
                    return false;
                }
                let submission = submit.read(cx).submission(cx);
                submit.update(cx, |form, _| form.submitted = true);
                let on_submit = on_submit.clone();
                cx.defer(move |cx| on_submit(submission, cx));
                true
            })
            .open(window, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::dialog::PostActionOptions;

    fn element(optional: bool, min: i64, max: i64) -> DialogElement {
        DialogElement {
            optional,
            min_length: min,
            max_length: max,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_field_is_fine_only_when_it_is_optional() {
        assert!(length_ok(&element(true, 0, 0), 0));
        assert!(!length_ok(&element(false, 0, 0), 0));
        // The bounds do not apply to a field left empty on purpose.
        assert!(length_ok(&element(true, 5, 10), 0));
    }

    #[test]
    fn a_field_asks_for_the_keyboard_its_subtype_wants() {
        use gpui_android_glue::keyboard::Purpose;
        let typed = |subtype: &str| DialogElement { subtype: subtype.into(), ..Default::default() };
        assert_eq!(purpose(&typed("number")), Purpose::Number);
        assert_eq!(purpose(&typed("email")), Purpose::Login);
        assert_eq!(purpose(&typed("url")), Purpose::Address);
        assert_eq!(purpose(&typed("password")), Purpose::Password);
        assert_eq!(purpose(&typed("")), Purpose::Plain);
        assert_eq!(purpose(&typed("tel")), Purpose::Plain);
    }

    #[test]
    fn the_bounds_are_inclusive_and_zero_means_none() {
        let bounded = element(false, 3, 5);
        assert!(!length_ok(&bounded, 2));
        assert!(length_ok(&bounded, 3));
        assert!(length_ok(&bounded, 5));
        assert!(!length_ok(&bounded, 6));
        assert!(length_ok(&element(false, 0, 0), 10_000));
    }

    #[test]
    fn a_requirement_says_what_is_wanted() {
        assert_eq!(requirement(&element(false, 0, 0)), "Required");
        assert_eq!(requirement(&element(false, 3, 0)), "At least 3 characters");
        assert_eq!(requirement(&element(false, 0, 9)), "At most 9 characters");
        assert_eq!(
            requirement(&element(false, 3, 9)),
            "Between 3 and 9 characters"
        );
    }

    #[test]
    fn the_default_option_is_the_named_one_or_the_first() {
        let mut select = element(false, 0, 0);
        select.options = ["a", "b", "c"]
            .into_iter()
            .map(|value| PostActionOptions {
                text: value.to_uppercase(),
                value: value.to_string(),
            })
            .collect();
        select.default = "b".into();
        assert_eq!(default_option(&select), 1);
        select.default = "nothing-like-it".into();
        assert_eq!(default_option(&select), 0);
    }
}
