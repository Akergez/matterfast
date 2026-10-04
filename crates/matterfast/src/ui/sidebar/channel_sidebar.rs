use gpui_kit::App;

/// The channel list, pane one.
pub struct ChannelSidebar;

impl ChannelSidebar {
    /// The list is drawn from the state every frame, so all a refresh has to
    /// do is ask for a frame.
    pub fn refresh(&self, cx: &mut App) {
        cx.refresh_windows();
    }
}
