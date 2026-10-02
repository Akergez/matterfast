//! The form an integration asks for: Mattermost interactive dialogs.
//!
//! Like [`super::dialogs`], nothing here talks to the server or to
//! [`crate::state`]. [`present`] renders a [`Dialog`] and hands the filled-in
//! values to a callback; the caller POSTs them to
//! `/api/v4/actions/dialogs/submit`.

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, FontWeight, Subscription, Window};
use mattermost_api::models::dialog::{Dialog, DialogElement};
use serde_json::Value;

use super::dialogs;
use super::Ui;

/// What a field is drawn as, and what it currently holds.
enum Kind {
    /// A single-line field. Also where a `select` backed by `users` or
    /// `channels` lands: those carry no options in the payload, the client is
    /// expected to search the server for them, and this file has no client to
    /// search with.
    Text(Entity<InputState>),
    Area(Entity<TextareaState>),
    /// A `select` whose options came in the payload.
    Select(usize),
    /// A `bool`. The default arrives as the *string* `"true"`, and the answer
    /// goes back as a real JSON boolean — the server's `map[string]any` keeps
    /// the type.
    Bool(bool),
    Radio(usize),
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
                let kind = match element.element_type.as_str() {
                    "textarea" => {
                        let area = cx.new(|cx| {
                            TextareaState::new(window, cx)
                                .rows(4)
                                .default_value(element.default.clone())
                                .placeholder(element.placeholder.clone())
                        });
                        // The length bounds are checked as it is typed, so
                        // the form has to be drawn again to say so.
                        subscriptions.push(cx.subscribe(
                            &area,
                            |_, _, event: &InputEvent, cx| {
                                if matches!(event, InputEvent::Change) {
                                    cx.notify();
                                }
                            },
                        ));
                        Kind::Area(area)
                    }
                    "radio" if !element.options.is_empty() => Kind::Radio(default_option(element)),
                    "select" if !element.options.is_empty() => {
                        Kind::Select(default_option(element))
                    }
                    "bool" => Kind::Bool(element.default == "true"),
                    // Everything else — `text`, a `select` whose options live
                    // on the server, and any type this client does not know
                    // yet — is a text field. An unknown type degrading to text
                    // is better than a form with a hole in it.
                    _ => {
                        let input = cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(element.default.clone())
                                .placeholder(element.placeholder.clone())
                                // A password is worth hiding even though the
                                // payload treats it as text.
                                .masked(element.subtype == "password")
                        });
                        subscriptions.push(cx.subscribe(
                            &input,
                            |_, _, event: &InputEvent, cx| {
                                if matches!(event, InputEvent::Change) {
                                    cx.notify();
                                }
                            },
                        ));
                        Kind::Text(input)
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
            Kind::Text(input) => {
                length_ok(&element.spec, input.read(cx).value().chars().count())
            }
            Kind::Area(area) => length_ok(&element.spec, area.read(cx).value().chars().count()),
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
                    Kind::Text(input) => Value::String(input.read(cx).value().to_string()),
                    Kind::Area(area) => Value::String(area.read(cx).value().to_string()),
                    // ponytail: a `multiselect` select submits only the one
                    // option; swap in a check-list if an integration that
                    // uses it turns up.
                    Kind::Select(index) | Kind::Radio(index) => option(index),
                    Kind::Bool(on) => Value::Bool(*on),
                };
                (element.spec.name.clone(), value)
            })
            .collect()
    }
}

impl Render for Interactive {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let mut column = v_flex().gap_3();
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
            let label = div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(element.spec.display_name.clone());
            let help = (!element.spec.help_text.is_empty()).then(|| {
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(element.spec.help_text.clone())
            });

            let field = match &element.kind {
                Kind::Text(input) => v_flex()
                    .gap_1()
                    .child(label)
                    .child(Input::new(input))
                    .into_any_element(),
                Kind::Area(area) => v_flex()
                    .gap_1()
                    .child(label)
                    .child(Textarea::new(area))
                    .into_any_element(),
                Kind::Bool(on) => h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(label))
                    .child(Switch::new(("bool", index)).checked(*on).on_click(
                        cx.listener(move |form, on: &bool, _, cx| {
                            if let Some(Element {
                                kind: Kind::Bool(held),
                                ..
                            }) = form.elements.get_mut(index)
                            {
                                *held = *on;
                            }
                            cx.notify();
                        }),
                    ))
                    .into_any_element(),
                Kind::Select(selected) => {
                    let entity = cx.entity();
                    let options = element.spec.options.clone();
                    let current = *selected;
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(div().flex_1().min_w_0().child(label))
                        .child(
                            Button::new(("select", index))
                                .label(
                                    options
                                        .get(current)
                                        .map(|option| option.text.clone())
                                        .unwrap_or_default(),
                                )
                                .small()
                                .dropdown_caret(true)
                                .dropdown_menu(move |mut menu, _, _| {
                                    for (choice, option) in options.iter().enumerate() {
                                        let entity = entity.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(option.text.clone())
                                                .checked(choice == current)
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |form, cx| {
                                                        if let Some(Element {
                                                            kind: Kind::Select(selected),
                                                            ..
                                                        }) = form.elements.get_mut(index)
                                                        {
                                                            *selected = choice;
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
                Kind::Radio(selected) => {
                    let mut group = v_flex().gap_1p5().child(label);
                    for (choice, option) in element.spec.options.iter().enumerate() {
                        group = group.child(
                            Radio::new(gpui_kit::ElementId::Name(
                                format!("radio-{index}-{choice}").into(),
                            ))
                            .label(option.text.clone())
                            .checked(choice == *selected)
                            .on_click(cx.listener(
                                move |form, _: &bool, _, cx| {
                                    if let Some(Element {
                                        kind: Kind::Radio(selected),
                                        ..
                                    }) = form.elements.get_mut(index)
                                    {
                                        *selected = choice;
                                    }
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    group.into_any_element()
                }
            };

            column = column.child(
                v_flex()
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
                                .text_color(theme.danger)
                                .child(requirement(&element.spec)),
                        )
                    }),
            );
        }

        div()
            .id("interactive")
            .max_h(px(520.))
            .overflow_y_scroll()
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
        window.open_dialog(cx, move |dialog, _, _| {
            let submit = view.clone();
            let closed = view.clone();
            let on_submit = on_submit.clone();
            let on_cancel = on_cancel.clone();
            dialog
                .title(title.clone())
                .w(px(480.))
                .child(view.clone())
                .footer(dialogs::footer(submit_label.clone()))
                .on_ok(move |_, _, cx| {
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
                // Submitting and dismissing both close the dialog, so the
                // callbacks hang off the one close handler and the flag says
                // which of the two it was.
                .on_close(move |_, _, cx| {
                    if !closed.read(cx).submitted {
                        let on_cancel = on_cancel.clone();
                        cx.defer(move |cx| on_cancel(cx));
                    }
                })
        });
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
