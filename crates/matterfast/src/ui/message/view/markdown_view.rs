use gpui_adaptive::markdown::MarkdownExtensions;
use gpui_adaptive::Markdown;
use gpui_kit::{ElementId, SharedString};

use super::custom_emoji::CustomEmoji;
use super::mention::Mention;

/// Markdown as an element. `id` has to be stable across redraws: the view
/// keeps its parsed text, and a selection, under it.
///
/// A clicked link is followed; the right button asks what to do with it
/// instead. The text insists on handlers it can share between threads, so
/// they cannot capture the session; they look it up instead.
pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> Markdown {
    // Built once and cloned: a clone keeps its revision, which is what lets
    // a view keep its parsed text from one frame to the next.
    static EXTENSIONS: std::sync::OnceLock<MarkdownExtensions> = std::sync::OnceLock::new();
    let extensions = EXTENSIONS
        .get_or_init(|| MarkdownExtensions::default().plugin(CustomEmoji).plugin(Mention));
    Markdown::new(id, source)
        .selectable(false)
        .inherit_size()
        .extensions(extensions.clone())
        .on_link(|url, _, cx| {
            let Some(ui) = crate::ui::current(cx) else { return };
            let url = url.to_string();
            ui.later(cx, move |ui, cx| ui.follow_link(&url, cx));
        })
        .on_link_menu(|url, position, _, cx| {
            let Some(ui) = crate::ui::current(cx) else { return };
            let url = url.to_string();
            ui.later(cx, move |_, cx| crate::ui::open_link_menu(url, position, cx));
        })
}
