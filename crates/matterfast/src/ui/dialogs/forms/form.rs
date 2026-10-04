use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, AppContext, Context, Subscription, Window};

use super::field::Field;
use super::slot::Slot;
use super::value::Value;

/// The body of a form dialog: its fields, and what they currently hold.
pub struct Form {
    pub(super) slots: Vec<Slot>,
    /// Text under the fields, explaining something about them.
    pub(super) note: String,
    /// The first field feeds the second until someone edits the second
    /// themselves; from then on it is theirs, and we notice by remembering
    /// what we last wrote there.
    pub(super) autofill: Option<(fn(&str) -> String, String)>,
    pub(super) _subscriptions: Vec<Subscription>,
}

impl Form {
    pub(crate) fn new(fields: Vec<Field>, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
    pub(super) fn follow(
        &mut self,
        derive: fn(&str) -> String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(crate) fn set_note(&mut self, note: &str) {
        self.note = note.to_string();
    }

    /// Writes a text field, as though it had been typed.
    pub(crate) fn set_text(
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

    /// Empties every text field, ready for the next entry.
    pub(crate) fn clear_texts(&self, window: &mut Window, cx: &mut App) {
        for slot in &self.slots {
            if let Slot::Text { input, .. } = slot {
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
    }

    /// Picks one of a choice field's options.
    pub(crate) fn set_choice(&mut self, index: usize, option: usize, cx: &mut Context<Self>) {
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
    pub(crate) fn choice(&self, index: usize) -> usize {
        match self.slots.get(index) {
            Some(Slot::Choice { selected, .. }) => *selected,
            _ => 0,
        }
    }

    pub(crate) fn values(&self, cx: &App) -> Vec<Value> {
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
