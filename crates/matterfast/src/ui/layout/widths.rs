use std::cell::Cell;

use gpui_kit::App;

use super::divider::Divider;

/// How wide the person dragged the side columns, in pixels; nothing for a
/// column left at its share of the window. It is about this machine's screen,
/// so it is kept with the other settings of the machine, and read once: the
/// settings are a file, and a width is asked for on every frame.
pub struct Widths {
    sidebar: Cell<Option<f32>>,
    panel: Cell<Option<f32>>,
    /// A drag moved a divider and the file does not know yet. Written when
    /// the drag is over rather than on every pixel of it.
    unsaved: Cell<bool>,
}

impl Default for Widths {
    fn default() -> Self {
        let stored = |divider: Divider| {
            crate::background::setting(divider.key())
                .and_then(|value| value.as_f64())
                .map(|width| width as f32)
                .filter(|width| width.is_finite() && *width > 0.0)
        };
        Widths {
            sidebar: Cell::new(stored(Divider::Sidebar)),
            panel: Cell::new(stored(Divider::Panel)),
            unsaved: Cell::new(false),
        }
    }
}

impl Widths {
    fn cell(&self, divider: Divider) -> &Cell<Option<f32>> {
        match divider {
            Divider::Sidebar => &self.sidebar,
            Divider::Panel => &self.panel,
        }
    }

    pub fn get(&self, divider: Divider) -> Option<f32> {
        self.cell(divider).get()
    }

    /// `None` puts the column back to its share of the window.
    pub fn set(&self, divider: Divider, width: Option<f32>, cx: &mut App) {
        if self.cell(divider).replace(width) != width {
            self.unsaved.set(true);
            crate::ui::refresh(cx);
        }
    }

    /// Writes what a drag changed, once the drag has let go.
    pub fn save_when_settled(&self, cx: &App) {
        if cx.has_active_drag() || !self.unsaved.replace(false) {
            return;
        }
        for divider in [Divider::Sidebar, Divider::Panel] {
            let value = match self.get(divider) {
                Some(width) => serde_json::json!(width.round()),
                None => serde_json::Value::Null,
            };
            crate::background::set_setting(divider.key(), value);
        }
    }
}
