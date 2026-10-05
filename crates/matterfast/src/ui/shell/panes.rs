use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    div, AnyElement, App, AppContext, Context, Entity, EntityId, Global, StyleRefinement, Window,
};

use super::shell_view::Shell;
use crate::ui::{frame_log, Ui};

/// Which of the three columns a [`Pane`] draws.
#[derive(Clone, Copy)]
enum Which {
    Sidebar,
    Chat,
    Right,
}

/// One column of the session, as a view of its own.
///
/// The session is not made of entities, so a column is drawn by a function.
/// Drawn straight into the window's one view, though, every column was built
/// again for each frame any of them needed: scrolling the conversation laid
/// out the whole channel list sixty times a second. A view is the unit the
/// toolkit can keep from one frame to the next, so each column gets a thin
/// one, and a hover or a scroll inside it redraws that column alone.
///
/// Whatever changes what a column shows calls `refresh_windows`, which
/// ignores what was kept; nothing has to know which column it touched. What
/// does know, and happens often, uses [`redraw`] instead.
pub(super) struct Pane {
    ui: Rc<Ui>,
    which: Which,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = &self.ui;
        // The call dock sits under the channel list, and under the
        // conversation once the list is a page of its own.
        let collapsed = ui.split.collapsed.get();
        let began = std::time::Instant::now();
        let part = match self.which {
            Which::Sidebar => Part::Sidebar,
            Which::Chat => Part::Chat,
            Which::Right => Part::Right,
        };
        let content = match self.which {
            Which::Sidebar => {
                let dock = (!collapsed)
                    .then(|| crate::ui::call_dock::render(ui, cx))
                    .flatten();
                crate::ui::sidebar::render(ui, dock, cx)
            }
            Which::Chat => {
                ui.learn_unknown_mentions();
                let dock = collapsed
                    .then(|| crate::ui::call_dock::render(ui, cx))
                    .flatten();
                crate::ui::chat::render(ui, collapsed, dock, cx)
            }
            Which::Right => {
                ui.learn_unknown_mentions();
                crate::ui::rhs::render(ui, cx).unwrap_or_else(|| div().into_any_element())
            }
        };
        if !frame_log::enabled() {
            return content;
        }
        frame_log::built(part, began);
        div()
            .size_full()
            .child(frame_log::mark(part, false))
            .child(content)
            .child(frame_log::mark(part, true))
            .into_any_element()
    }
}

/// A part of the window that can be drawn again without the others.
#[derive(Clone, Copy)]
pub enum Part {
    /// What is laid over the columns or around them: the title bar, remote
    /// video, a picture filling the window. Costs next to nothing, as the
    /// columns under it are kept.
    Frame,
    Sidebar,
    Chat,
    Right,
}

/// The views of the session on screen, so that something with only an [`App`]
/// in hand can name one of them.
struct OnScreen {
    frame: EntityId,
    sidebar: EntityId,
    chat: EntityId,
    right: EntityId,
}

impl Global for OnScreen {}

/// Draws these parts of the window again, and no more than these.
///
/// For what happens many times a second and is known to show in one place: a
/// frame of video, a line of transcription, somebody typing. Anything else
/// is better off with `refresh_windows`, which cannot leave a column stale.
pub fn redraw(parts: &[Part], cx: &mut App) {
    let Some(shown) = cx.try_global::<OnScreen>() else {
        cx.refresh_windows();
        return;
    };
    let ids = parts.iter().map(|part| match part {
        Part::Frame => shown.frame,
        Part::Sidebar => shown.sidebar,
        Part::Chat => shown.chat,
        Part::Right => shown.right,
    });
    for id in ids.collect::<Vec<_>>() {
        cx.notify(id);
    }
}

/// The columns of one session, for as long as a window shows it.
pub(super) struct Panes {
    sidebar: Entity<Pane>,
    chat: Entity<Pane>,
    right: Entity<Pane>,
}

impl Panes {
    pub(super) fn new(ui: &Rc<Ui>, cx: &mut Context<Shell>) -> Self {
        let mut pane = |which| {
            let ui = ui.clone();
            cx.new(|_| Pane { ui, which })
        };
        let panes = Panes {
            sidebar: pane(Which::Sidebar),
            chat: pane(Which::Chat),
            right: pane(Which::Right),
        };
        cx.set_global(OnScreen {
            frame: cx.entity_id(),
            sidebar: panes.sidebar.entity_id(),
            chat: panes.chat.entity_id(),
            right: panes.right.entity_id(),
        });
        panes
    }

    pub(super) fn sidebar(&self) -> AnyElement {
        kept(&self.sidebar)
    }

    pub(super) fn chat(&self) -> AnyElement {
        kept(&self.chat)
    }

    pub(super) fn right(&self) -> AnyElement {
        kept(&self.right)
    }
}

/// A column that fills the box it is put in, and is only drawn again when it
/// has changed. A kept view is not measured from its contents, which is why
/// every column is given its size by what holds it.
fn kept(pane: &Entity<Pane>) -> AnyElement {
    pane.clone()
        .cached(StyleRefinement::default().size_full())
        .into_any_element()
}
