//! Attaching files to the message being written.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;

/// Above this a file goes up in chunks through an upload session, so a
/// dropped connection resumes instead of starting the whole thing again.
/// Below it, one multipart request is fewer round trips.
const CHUNKED_ABOVE: u64 = 8 * 1024 * 1024;
const CHUNK: usize = 4 * 1024 * 1024;

impl Ui {
    /// Asks for files and uploads them straight away.
    ///
    /// Uploading on pick rather than on send is what the other clients do, and
    /// it is the reason sending feels instant: by the time a message goes out
    /// its attachments are already on the server.
    pub(crate) fn pick_attachment(self: &Rc<Self>, cx: &mut App) {
        let Some(channel_id) = self.state.borrow().current_channel.clone() else {
            return;
        };
        let picked = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        let ui = self.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            if paths.is_empty() {
                return;
            }
            cx.update(|cx| {
                ui.chat.set_uploading(paths.len(), cx);
                for path in paths {
                    ui.upload(&channel_id, path, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn upload(
        self: &Rc<Self>,
        channel_id: &str,
        path: std::path::PathBuf,
        _cx: &mut App,
    ) {
        let client = self.state.borrow().client.clone();
        let channel_id = channel_id.to_string();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());

        let ui = self.clone();
        let label = name.clone();
        runtime::spawn(
            async move {
                // Reading in the worker: a large file would otherwise block
                // the frame this was started from.
                let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                if bytes.len() as u64 <= CHUNKED_ABOVE {
                    return client
                        .upload_file(&channel_id, &name, bytes, None)
                        .await
                        .map_err(|e| e.to_string());
                }

                // Chunked: create a session, then send from wherever the
                // server says it got to. There is no Content-Range here — the
                // body simply starts at file_offset.
                let session = client
                    .create_upload_session(&channel_id, &name, bytes.len() as i64)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut offset = session.file_offset as usize;
                loop {
                    let end = (offset + CHUNK).min(bytes.len());
                    let info = client
                        .upload_data(&session.id, bytes[offset..end].to_vec())
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(info) = info {
                        return Ok(mattermost_api::models::FileUploadResponse {
                            file_infos: vec![info],
                            ..Default::default()
                        });
                    }
                    if end >= bytes.len() {
                        return Err("the server never finished the upload".to_string());
                    }
                    // Trust the server's idea of where it got to rather than
                    // our own arithmetic: a partial write is its to report.
                    offset = client
                        .upload_session(&session.id)
                        .await
                        .map_err(|e| e.to_string())?
                        .file_offset as usize;
                }
            },
            move |result, cx| {
                match result {
                    Ok(response) => {
                        let mut st = ui.state.borrow_mut();
                        for info in response.file_infos {
                            st.pending_files.push((info.id, info.name.clone()));
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not attach {label}: {e}"), cx),
                }
                ui.chat.upload_finished(cx);
                ui.refresh_attachments(cx);
            },
        );
    }

    pub(crate) fn refresh_attachments(self: &Rc<Self>, cx: &mut App) {
        // The chips are drawn from the state's own list of waiting files, so
        // all there is to do is draw again.
        crate::ui::refresh(cx);
    }
}
