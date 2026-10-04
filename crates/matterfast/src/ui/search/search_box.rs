use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::component::input::InputState;
use gpui_kit::{App, Entity};

use super::hint::Hint;
use super::suggestion::Suggestion;
use crate::ui::WindowSlot;

/// The search box of the session.
pub struct SearchBox {
    pub(super) window: Rc<WindowSlot>,
    /// The field, while there is a window to put it in.
    pub(super) input: RefCell<Option<Entity<InputState>>>,
    /// What the word under the cursor asks for; `None` closes the list.
    pub(super) hint: RefCell<Option<Hint>>,
    pub(super) rows: RefCell<Vec<Suggestion>>,
    /// The row an arrow key went to. Nothing until one is pressed.
    pub(super) selected: Cell<Option<usize>>,
    /// Whether the cursor was in the box when it was last drawn. Asked of the
    /// window on every frame rather than kept from the field's focus events:
    /// those only come while the window itself has the keyboard, and a list
    /// that waits for them never opens in a window that was focused from a
    /// shortcut before the desktop said so.
    pub(super) focused: Cell<bool>,
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

    pub(crate) fn attach(&self, input: Entity<InputState>) {
        *self.input.borrow_mut() = Some(input);
    }

    pub(crate) fn detach(&self) {
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
    pub(crate) fn offered(&self) -> Vec<String> {
        if !self.is_open() {
            return Vec::new();
        }
        self.rows.borrow().iter().map(|row| row.label.clone()).collect()
    }

    /// What is in the box.
    pub(crate) fn text(&self, cx: &App) -> String {
        self.input
            .borrow()
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    pub(super) fn is_open(&self) -> bool {
        self.focused.get() && self.hint.borrow().is_some() && !self.rows.borrow().is_empty()
    }

    pub(super) fn close(&self) {
        self.hint.borrow_mut().take();
        self.rows.borrow_mut().clear();
        self.selected.set(None);
    }
}
