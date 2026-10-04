use std::rc::Rc;

use gpui_kit::App;

use super::trailing::Trailing;
use crate::ui::dialogs::callback::Callback;
use crate::ui::kit::Lucide;

/// One row of a list dialog.
pub struct Row {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) subtitle: String,
    /// Longer text under the title, shown whole rather than on one line.
    pub(super) body: String,
    pub(super) trailing: Vec<Trailing>,
    /// What pressing the row itself does, when the whole row is the target.
    pub(super) activate: Option<Callback>,
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
