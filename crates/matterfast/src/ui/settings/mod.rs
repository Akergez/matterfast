//! The settings dialog: how the window looks.
//!
//! Every choice here takes effect as it is made — there is no Save, because
//! the only way to judge a theme or a font is to see it, and a dialog that
//! makes you commit first and look afterwards has that backwards.
//!
//! Themes are chosen twice, one for light and one for dark, so that "System"
//! has a pair to switch between. More of them come from Zed's extension
//! registry, which this dialog can browse: the list is asked for when the
//! section is first opened and searched locally after that, so typing costs
//! no requests. The registry is not a Mattermost server and needs no session,
//! which is why its two calls are made from here ([`crate::zed_extensions`])
//! rather than from `ui/mod.rs` with the rest.

mod browse;
mod built_in_label;
mod constants;
mod dialog;
mod downloads;
mod found;
mod names;
mod new;
mod registry;
mod registry_rows;
mod render;
mod show;
mod theme_changes;

pub use show::show;
