use gpui_adaptive::TextFieldState;
use gpui_kit::Entity;

use super::row::Row;
use crate::ui::kit::Lucide;

/// Why a section has nothing in it.
pub(crate) type Empty = (Lucide, &'static str, &'static str);

pub(crate) struct Section {
    pub(super) title: String,
    pub(super) rows: Vec<Row>,
    pub(super) empty: Option<Empty>,
    /// A search box over this section's rows.
    pub(crate) search: Option<Entity<TextFieldState>>,
}
