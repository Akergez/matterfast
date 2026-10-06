//! The settings dialog: how the window looks.
//!
//! Every choice here takes effect as it is made — there is no Save, because
//! the only way to judge a theme or a font is to see it, and a dialog that
//! makes you commit first and look afterwards has that backwards.
//!
//! Themes are chosen twice, one for light and one for dark, so that "System"
//! has a pair to switch between. The first of each list is the Material You
//! scheme the window wears unless told otherwise; the rest come from Zed's
//! extension registry, which this dialog can browse. The browsing is the
//! library's (`gpui_zed_themes::Browser`): it asks the registry, installs
//! and removes, and says when what is installed has changed, which is all
//! this dialog needs to hear to make its two lists right again. The registry
//! is not a Mattermost server and needs no session, which is why nothing of
//! it goes through `ui/mod.rs` with the rest.

mod built_in_label;
mod dialog;
mod names;
mod new;
mod render;
mod show;
mod themes_changed;

pub use show::show;
