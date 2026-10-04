use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::SharedString;

/// A searchable list of names: themes, or font families.
pub(super) type Names = SelectState<SearchableVec<SharedString>>;
