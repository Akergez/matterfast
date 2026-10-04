use gpui_kit::{ElementId, SharedString};

/// A stable id for something inside a row. Ids nest under the row's own, so
/// these only have to be unique within one message.
pub(super) fn eid(name: impl Into<SharedString>) -> ElementId {
    ElementId::Name(name.into())
}
