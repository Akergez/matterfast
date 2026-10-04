use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, AppContext, Context, Entity, Subscription, Window};

use super::row::Row;
use super::section::{Empty, Section};
use crate::ui::dialogs::forms::{Form, Value};

/// The body of a list dialog.
pub struct List {
    pub(crate) sections: Vec<Section>,
    /// Buttons already pressed, as (row id, which button): each acknowledges
    /// its click once and then stays that way.
    pub(super) pressed: HashSet<(String, usize)>,
    /// A small form under the lists, for adding to them.
    pub(crate) form: Option<Entity<Form>>,
    pub(crate) form_title: String,
    pub(crate) form_action: Option<Rc<dyn Fn(Vec<Value>, &mut App)>>,
    _subscriptions: Vec<Subscription>,
}

impl List {
    pub(super) fn new() -> Self {
        List {
            sections: Vec::new(),
            pressed: HashSet::new(),
            form: None,
            form_title: String::new(),
            form_action: None,
            _subscriptions: Vec::new(),
        }
    }

    pub(crate) fn section(&mut self, title: &str, empty: Option<Empty>) -> usize {
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
    pub(crate) fn searchable(
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
    pub(crate) fn term(&self, section: usize, cx: &App) -> String {
        self.sections
            .get(section)
            .and_then(|section| section.search.as_ref())
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default()
    }

    pub(crate) fn set_rows(&mut self, section: usize, rows: Vec<Row>, cx: &mut Context<Self>) {
        if let Some(section) = self.sections.get_mut(section) {
            // A refilled list is a new answer: its buttons are new buttons.
            let ids: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
            self.pressed.retain(|(id, _)| !ids.contains(id.as_str()));
            section.rows = rows;
        }
        cx.notify();
    }
}
