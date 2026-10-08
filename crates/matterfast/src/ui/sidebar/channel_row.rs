use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, rems, AnyElement, App, ElementId, FontWeight, Rems};
use mattermost_api::models::{Channel, ChannelType};

use super::call_badge::call_badge;
use super::channel_sidebar::ROW_HEIGHT;
use super::preview::{preview, Preview};
use super::row_menu::row_menu;
use crate::state::AppState;
use crate::timefmt::format_chat_time;
use crate::ui::kit::{self, Lucide};
use crate::ui::message::{custom_status_tooltip, emoji_element, status_is_live};
use crate::ui::rhs::PanelMode;
use crate::ui::{Action, Ui};

/// How big a conversation's row is drawn: the face at its start, and the
/// row itself. Both in rems, so that a row is as many lines of its own text
/// tall at any size of text, and the face stays as tall as those lines.
#[derive(Clone, Copy)]
pub(crate) struct Fit {
    face: Rems,
    height: Rems,
    /// Whether threads have rows of their own in the same list. Where they
    /// do, an open thread is the row that is lit, and the conversation it
    /// is in is not lit as well.
    among_threads: bool,
    /// How tall the two lines are, and the room left under them where the
    /// rows around have a third. Each line has its height whether or not
    /// there is anything on it: a row with nothing said in it would
    /// otherwise have its name in the middle, and the names of a list would
    /// not be evenly apart. Together they are a little under the face.
    heading: Rems,
    detail: Rems,
    spare: Option<Rems>,
}

impl Fit {
    /// In the list of conversations: a face, and two lines beside it.
    pub(crate) const LIST: Fit = Fit {
        face: rems(2.5),
        height: ROW_HEIGHT,
        among_threads: false,
        heading: rems(1.25),
        detail: rems(1.125),
        spare: None,
    };
    /// In the inbox, between threads, which are three lines tall with a
    /// face to match: a conversation there stands as tall as its
    /// neighbours, so that the text of one row starts under the text of
    /// the last.
    pub(crate) const INBOX: Fit = Fit {
        face: INBOX_FACE,
        height: INBOX_ROW,
        among_threads: true,
        heading: INBOX_HEADING,
        detail: INBOX_SAID,
        spare: Some(INBOX_ANSWERS),
    };
}

/// How tall each of the three lines of a row of the inbox is: who and where,
/// what was said, and how many answered. Set tight, so that together they
/// are a little shorter than the face beside them — the text does not stand
/// out past the face at the top or the bottom.
pub(crate) const INBOX_HEADING: Rems = rems(1.125);
pub(crate) const INBOX_SAID: Rems = rems(1.);
pub(crate) const INBOX_ANSWERS: Rems = rems(0.875);

/// The face of a row of the inbox: as tall as the three lines beside it.
pub(crate) const INBOX_FACE: Rems = rems(3.125);
/// A row of the inbox: its face, and room around it.
pub(crate) const INBOX_ROW: Rems = rems(4.5);

pub(crate) fn channel_row(
    ui: &Rc<Ui>,
    channel: &Channel,
    st: &AppState,
    fit: Fit,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let title = st.channel_title(channel);
    let unread = st.unread(&channel.id);
    let in_thread = matches!(ui.right.mode(cx), PanelMode::Thread(_));
    let selected = st.current_channel.as_deref() == Some(channel.id.as_str())
        && !(fit.among_threads && in_thread);
    let teammate = channel.dm_teammate_id(&st.me.id);

    // A conversation with one person is that person: their face, with the
    // presence dot, exactly like a message row. A channel has no face, so it
    // gets a disc with what kind of channel it is — "#" for a public one,
    // the way Mattermost writes them.
    let face: AnyElement = match (teammate, &channel.r#type) {
        (Some(user_id), _) => {
            // The picture is asked for in pixels: the rems at the size the
            // text is set in.
            let size = f32::from(fit.face.to_pixels(theme.font_size));
            kit::avatar_with_presence(ui, user_id, &title, size, st.presence(user_id), cx)
        }
        (None, kind) => div()
            .size(fit.face)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme.muted)
            .text_color(theme.muted_foreground)
            .map(|disc| match kind {
                ChannelType::Open => disc.text_lg().child("#"),
                ChannelType::Private => disc.child(Lucide::Lock),
                ChannelType::Group => disc.child(Lucide::Users),
                _ => disc.child(Lucide::User),
            })
            .into_any_element(),
    };

    // The first line: who, and when they were last written to.
    let mut heading = h_flex()
        .h(fit.heading)
        .line_height(fit.heading)
        .gap_1p5()
        .items_center()
        .child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .when(fit.among_threads || unread.is_unread(), |label| {
                label.font_weight(FontWeight::SEMIBOLD)
            })
            .child(title),
    );
    // A DM is a person, and their status says whether writing to them is
    // worth doing now.
    if let Some(status) = teammate
        .and_then(|id| st.users.get(id))
        .and_then(|user| user.custom_status())
        .filter(|status| !status.emoji.is_empty() && status_is_live(status))
    {
        heading = heading.child(kit::with_tooltip(
            "status",
            emoji_element(ui, &status.emoji, 16.),
            custom_status_tooltip(&status),
        ));
    }
    heading = heading.child(
        div()
            .flex_none()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(format_chat_time(channel.last_post_at)),
    );

    // The second: what was said last, and what is waiting.
    let quiet = theme.muted_foreground;
    let said = |author: Option<String>, text: String| match author {
        Some(author) => format!("{author}: {text}"),
        None => text,
    };
    let line = div().flex_1().min_w_0().truncate().text_sm();
    let line = match preview(channel, st) {
        Preview::Draft(text) => line
            .text_color(quiet)
            .child(h_flex().gap_1().child(div().text_color(theme.danger).child("Draft:")).child(text)),
        Preview::Last { author, text } => line.text_color(quiet).child(said(author, text)),
        // Greyer than a line that is the news, and marked: the dot says
        // there is something newer that is not here to be read yet.
        Preview::Stale { author, text } => line
            .text_color(quiet.opacity(0.6))
            .child(format!("\u{2022} {}", said(author, text))),
        Preview::Nothing => line,
    };
    let mut detail = h_flex()
        .h(fit.detail)
        .line_height(fit.detail)
        .gap_1p5()
        .items_center()
        .child(line);

    // A call in this channel matters more than an unread badge, so it goes
    // first and is always shown.
    if let Some(people) = st.active_calls.get(&channel.id) {
        detail = detail.child(call_badge(ui, people, st, cx));
    }
    if unread.mentions > 0 {
        detail = detail.child(kit::mention_badge(unread.mentions, unread.urgent, cx));
    } else if unread.is_unread() && !unread.muted {
        detail = detail.child(kit::unread_dot(cx));
    }

    let row = h_flex()
        .id(ElementId::Name(format!("channel-{}", channel.id).into()))
        .w_full()
        .h(fit.height)
        .px_2()
        .gap_2p5()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.sidebar_accent))
        .when(!selected, |row| row.hover(|style| style.bg(theme.list_hover)))
        // Muted channels still show mentions, but not bold-for-messages: that
        // is exactly the rule the official clients use.
        .when(unread.muted, |row| row.opacity(0.6))
        .child(face)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(heading)
                .child(detail)
                .when_some(fit.spare, |lines, spare| lines.child(div().h(spare))),
        );

    let entries = row_menu(&channel.id, st);
    let menu_ui = ui.clone();
    let channel_id = channel.id.clone();
    let select = channel.id.clone();
    // Choosing a conversation is choosing all of it: a thread that was open
    // beside the last thing looked at is put away, and this row is the one
    // that is lit.
    row.on_click(ui.click(move |ui, cx| {
        if matches!(ui.right.mode(cx), PanelMode::Thread(_)) {
            ui.dispatch(Action::CloseRightPanel, cx);
        }
        ui.dispatch(Action::SelectChannel(select.clone()), cx)
    }))
    // Mute it, mark it, or file it under a different category: all things
    // people expect to reach from the row itself rather than from a settings
    // screen.
    .context_menu(move |mut menu, _, _| {
        for (label, action) in &entries {
            let action = action.clone();
            let channel_id = channel_id.clone();
            menu = menu.item(PopupMenuItem::new(label.clone()).on_click(menu_ui.click(
                move |ui, cx| ui.dispatch(Action::Row(channel_id.clone(), action.clone()), cx),
            )));
        }
        menu
    })
    .into_any_element()
}
