use gpui_kit::component::text::{MarkdownExtensions, TextView};
use gpui_kit::{App, ClickEvent, ElementId, SharedString, Window};

use super::custom_emoji::CustomEmoji;
use super::mention::Mention;

/// A clicked link in a message body.
///
/// The text view insists on a handler it can share between threads, so this
/// cannot capture the session; it looks it up instead.
fn link_clicked(url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App) {
    let Some(ui) = crate::ui::current(cx) else {
        return;
    };
    let url = url.to_string();
    ui.later(cx, move |ui, cx| ui.follow_link(&url, cx));
}

/// Markdown as an element. `id` has to be stable across redraws: the view
/// keeps its parsed text, and a selection, under it.
pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> TextView {
    // Built once and cloned: a clone keeps its revision, which is what lets
    // a view keep its parsed text from one frame to the next.
    static EXTENSIONS: std::sync::OnceLock<MarkdownExtensions> = std::sync::OnceLock::new();
    let extensions = EXTENSIONS
        .get_or_init(|| MarkdownExtensions::default().plugin(CustomEmoji).plugin(Mention));
    TextView::markdown(id, source)
        .on_link_click(link_clicked)
        .markdown_extensions(extensions.clone())
}
