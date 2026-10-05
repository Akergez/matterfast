use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{px, App, ListAlignment, ListState};
use mattermost_api::models::{Channel, SidebarCategory};

/// How tall a channel's row is; a category's heading is measured.
pub(super) const ROW_HEIGHT: f32 = 32.;

/// How far past the edges of the list rows are kept drawn, so a flick of the
/// wheel does not outrun them.
const OVERDRAW: f32 = 320.;

/// One line of the channel list, by the id of what it shows. The rest is read
/// from the state when the line is drawn.
#[derive(Clone, PartialEq)]
pub(super) enum Row {
    Category(String),
    Channel(String),
}

/// The channel list, pane one.
pub struct ChannelSidebar {
    /// Which rows are on screen, and how tall the ones seen so far were: a
    /// server can have hundreds of channels, and only the few in view are
    /// built for a frame.
    pub(super) list: ListState,
    rows: RefCell<Rc<Vec<Row>>>,
}

impl ChannelSidebar {
    pub fn new() -> Self {
        ChannelSidebar {
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            rows: RefCell::new(Rc::new(Vec::new())),
        }
    }

    /// The list is drawn from the state every frame, so all a refresh has to
    /// do is ask for a frame.
    pub fn refresh(&self, cx: &mut App) {
        cx.refresh_windows();
    }

    /// The lines to draw for these groups. Nearly every frame they are the
    /// ones already held, and nothing is copied; when they are not, the list
    /// is told, and stays scrolled to where it was.
    pub(super) fn rows(&self, groups: &[(&SidebarCategory, Vec<&Channel>)]) -> Rc<Vec<Row>> {
        let wanted = || {
            groups.iter().flat_map(|(category, channels)| {
                std::iter::once((true, category.id.as_str()))
                    .chain(channels.iter().map(|channel| (false, channel.id.as_str())))
            })
        };
        let held = self.rows.borrow().clone();
        let same = held
            .iter()
            .map(|row| match row {
                Row::Category(id) => (true, id.as_str()),
                Row::Channel(id) => (false, id.as_str()),
            })
            .eq(wanted());
        if same {
            return held;
        }
        let rows: Rc<Vec<Row>> = Rc::new(
            wanted()
                .map(|(heading, id)| match heading {
                    true => Row::Category(id.to_string()),
                    false => Row::Channel(id.to_string()),
                })
                .collect(),
        );
        let top = self.list.logical_scroll_top();
        self.list
            .reset_with_uniform_height(rows.len(), px(ROW_HEIGHT));
        self.list.scroll_to(top);
        *self.rows.borrow_mut() = rows.clone();
        rows
    }
}
