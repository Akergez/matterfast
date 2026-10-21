//! Attached files: fetching them, looking at images and saving to disk.

use std::future::Future;
use std::path::{Path, PathBuf};
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
        let bytes = async move { fetch.await.map_err(|e| e.to_string()) };
        self.save_as(&file.name, &file.mime_type, bytes, cx);
    }

    /// Puts `bytes` in a file called `name`, wherever the person says.
    ///
    /// On a desktop that is the system's "Save as". Where that cannot be
    /// shown — no file chooser portal — it goes into the downloads directory
    /// under a name nothing there has yet, and says where; asking and then
    /// doing nothing looked like a button that does not work. A phone has no
    /// "Save as" here at all: the file is handed to whatever opens its kind
    /// (`open_file`).
    pub(crate) fn save_as(
        self: &Rc<Self>,
        name: &str,
        mime: &str,
        bytes: impl Future<Output = Result<Vec<u8>, String>> + Send + 'static,
        cx: &mut App,
    ) {
        let name = file_name(name);
        #[cfg(target_os = "android")]
        {
            let _ = cx;
            let path = crate::open_file::place_for(&name);
            let mime = mime.to_string();
            let ui = self.clone();
            runtime::spawn(
                async move {
                    write(&path, bytes.await?).await?;
                    crate::open_file::open(&path, &mime)
                },
                move |result, cx| {
                    if let Err(e) = result {
                        tracing::warn!(error = %e, "could not open the attachment");
                        ui.toast(&format!("Could not open it: {e}"), cx);
                    }
                },
            );
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = mime;
            let directory = dirs::download_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("/"));
            let chosen = cx.prompt_for_new_path(&directory, Some(&name));
            let ui = self.clone();
            cx.spawn(async move |cx| {
                let (path, unasked) = match chosen.await {
                    Ok(Ok(Some(path))) => (path, false),
                    // Cancelled.
                    Ok(Ok(None)) | Err(_) => return,
                    Ok(Err(error)) => {
                        tracing::warn!(%error, "no save dialog; saving to the downloads directory");
                        (unused_name(&directory, &name), true)
                    }
                };
                cx.update(|_| {
                    runtime::spawn(
                        async move { write(&path, bytes.await?).await.map(|()| path) },
                        move |result, cx| match result {
                            Ok(path) if unasked => {
                                ui.toast(&format!("Saved to {}.", path.display()), cx)
                            }
                            Ok(_) => ui.toast("Saved.", cx),
                            Err(e) => {
                                tracing::warn!(error = %e, "could not save the file");
                                ui.toast(&format!("Could not save it: {e}"), cx);
                            }
                        },
                    );
                });
            })
            .detach();
        }
    }

    /// Whether anything is uploaded and waiting to go out with a message.
    pub fn has_pending_files(&self) -> bool {
        !self.state.borrow().pending_files.is_empty()
    }
}

async fn write(path: &Path, bytes: Vec<u8>) -> Result<(), String> {
    if let Some(directory) = path.parent() {
        tokio::fs::create_dir_all(directory)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(path, bytes).await.map_err(|e| e.to_string())
}

/// An attachment's name as a file name: what somebody else's client called
/// it must not reach outside the directory it is put in.
fn file_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("attachment")
        .to_string()
}

/// `name` in `directory`, or, where that is taken, `name (1)`, `name (2)`…
/// with the extension kept last, the way a browser numbers a second download.
#[cfg_attr(target_os = "android", allow(dead_code))]
fn unused_name(directory: &Path, name: &str) -> PathBuf {
    let free = |candidate: &str| {
        let path = directory.join(candidate);
        (!path.exists()).then_some(path)
    };
    if let Some(path) = free(name) {
        return path;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    (1..)
        .find_map(|n| free(&format!("{stem} ({n}){extension}")))
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::{file_name, unused_name};

    #[test]
    fn a_name_cannot_climb_out_of_the_directory() {
        assert_eq!(file_name("../../.bashrc"), ".bashrc");
        assert_eq!(file_name("dir/report.pdf"), "report.pdf");
        assert_eq!(file_name(""), "attachment");
        assert_eq!(file_name(".."), "attachment");
    }

    #[test]
    fn a_taken_name_is_numbered_before_its_extension() {
        let directory = std::env::temp_dir().join(format!("matterfast-unused-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        assert_eq!(unused_name(&directory, "notes.md"), directory.join("notes.md"));
        std::fs::write(directory.join("notes.md"), "").unwrap();
        std::fs::write(directory.join("notes (1).md"), "").unwrap();
        assert_eq!(unused_name(&directory, "notes.md"), directory.join("notes (2).md"));
        std::fs::write(directory.join(".env"), "").unwrap();
        assert_eq!(unused_name(&directory, ".env"), directory.join(".env (1)"));
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
