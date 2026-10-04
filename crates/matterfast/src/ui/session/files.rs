//! Attached files: fetching them, looking at images and saving to disk.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::App;
use mattermost_api::models::FileInfo;

use super::ui::Ui;
use crate::runtime;
use crate::ui::constants::FILE_CACHE_TTL;
use crate::ui::lightbox;

impl Ui {
    /// Fetches an attached file, from the disk cache when it is there. The
    /// file is behind the session token, so it is fetched here rather than
    /// handed to anything else as a URL.
    pub(crate) fn fetch_file(
        &self,
        file_id: String,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, mattermost_api::Error>> + Send + 'static
    {
        let client = self.state.borrow().client.clone();
        let resources = self.avatars.resources();
        async move {
            let key = format!("original:{file_id}");
            resources
                .get_or_fetch(key, FILE_CACHE_TTL, || async move {
                    client.download_file(&file_id).await
                })
                .await
        }
    }

    /// Opens the full-size image over the window. A lightbox rather than a
    /// second window: a picture is something you glance at and dismiss, not
    /// something to manage in the window list.
    pub fn open_image(self: &Rc<Self>, file: &FileInfo, cx: &mut App) {
        let _ = cx;
        let fetch = self.fetch_file(file.id.clone());
        let file = file.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let bytes = fetch.await.map_err(|e| e.to_string())?;
                let decoded = bytes.clone();
                // Decoding a photograph is tens of milliseconds, which is a
                // few frames nobody should have to watch stall.
                let picture = tokio::task::spawn_blocking(move || {
                    image::load_from_memory(&decoded)
                        .map(|picture| crate::avatars::render_image(picture.into_rgba8()))
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;
                Ok::<_, String>((picture, bytes))
            },
            move |result, cx| match result {
                Ok((picture, bytes)) => lightbox::show(&ui, &file, Arc::new(picture), bytes, cx),
                Err(e) => ui.toast(&format!("Could not open that image: {e}"), cx),
            },
        );
    }

    /// Downloads an attachment to wherever the person says.
    pub fn save_attachment(self: &Rc<Self>, file: &FileInfo, cx: &mut App) {
        let fetch = self.fetch_file(file.id.clone());
        let directory = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let chosen = cx.prompt_for_new_path(&directory, Some(&file.name));
        let ui = self.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            cx.update(|_| {
                runtime::spawn(
                    async move {
                        let bytes = fetch.await.map_err(|e| e.to_string())?;
                        tokio::fs::write(&path, bytes)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result, cx| match result {
                        Ok(()) => ui.toast("Saved.", cx),
                        Err(e) => {
                            tracing::warn!(error = %e, "could not save the attachment");
                            ui.toast(&format!("Could not save it: {e}"), cx);
                        }
                    },
                );
            });
        })
        .detach();
    }

    /// Whether anything is uploaded and waiting to go out with a message.
    pub fn has_pending_files(&self) -> bool {
        !self.state.borrow().pending_files.is_empty()
    }
}
