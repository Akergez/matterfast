use std::rc::Rc;

use super::player::Player;
use crate::ui::Ui;

impl Ui {
    pub(super) fn media_player(&self, file_id: &str) -> Rc<Player> {
        self.players
            .borrow_mut()
            .entry(file_id.to_string())
            .or_insert_with(|| Rc::new(Player::new()))
            .clone()
    }

    /// The video being shown over the whole window, if one is.
    pub(crate) fn expanded_player(&self) -> Option<(String, Rc<Player>)> {
        self.players
            .borrow()
            .iter()
            .find(|(_, player)| player.expanded.get())
            .map(|(id, player)| (id.clone(), player.clone()))
    }
}
