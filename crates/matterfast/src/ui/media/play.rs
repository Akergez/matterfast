use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::FileInfo;

use super::formats::extension;
use super::sanitised::sanitised;
use super::stage::Stage;
use crate::playback::Playback;
use crate::runtime;
use crate::ui::Ui;

impl Ui {
    /// Play was pressed: resume, start over, or fetch the file and begin.
    pub(super) fn play_media(self: &Rc<Self>, file: &FileInfo, cx: &mut App) {
        let player = self.media_player(&file.id);
        match player.stage.get() {
            Stage::Loading => {}
            Stage::Playing => {
                if let Some(playback) = player.playback.borrow().as_ref() {
                    playback.set_paused(true);
                }
                player.stage.set(Stage::Paused);
            }
            Stage::Paused => {
                if let Some(playback) = player.playback.borrow().as_ref() {
                    playback.set_paused(false);
                }
                player.stage.set(Stage::Playing);
                self.tick_media(file.id.clone());
            }
            Stage::Ended => {
                if let Some(playback) = player.playback.borrow().as_ref() {
                    playback.restart();
                }
                player.stage.set(Stage::Playing);
                self.tick_media(file.id.clone());
            }
            Stage::Idle => {
                player.stage.set(Stage::Loading);
                player.error.borrow_mut().take();
                let fetch = self.fetch_file(file.id.clone());
                let path = std::env::temp_dir().join(format!(
                    "matterfast-{}.{}",
                    sanitised(&file.id),
                    sanitised(&extension(file))
                ));
                let ui = self.clone();
                let file_id = file.id.clone();
                runtime::spawn(
                    async move {
                        let bytes = fetch.await.map_err(|e| e.to_string())?;
                        tokio::fs::write(&path, bytes)
                            .await
                            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                        Ok::<_, String>(path)
                    },
                    move |result, cx| {
                        let player = ui.media_player(&file_id);
                        match result.and_then(Playback::start) {
                            Ok((playback, events)) => {
                                *player.playback.borrow_mut() = Some(playback);
                                player.stage.set(Stage::Playing);
                                ui.watch_media(file_id.clone(), events);
                                ui.tick_media(file_id);
                            }
                            Err(error) => player.failed(&error),
                        }
                        cx.refresh_windows();
                    },
                );
            }
        }
        cx.refresh_windows();
    }
}
