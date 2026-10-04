/// Asks the desktop portal for a screen or window to share.
///
/// Wayland has no other way in: an app cannot read the screen itself, it
/// can only be handed a PipeWire node by the compositor after the user
/// picks one.
pub async fn pick_screen() -> Result<(u32, std::os::fd::OwnedFd), String> {
    use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};

    let proxy = Screencast::new().await.map_err(|e| e.to_string())?;
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Embedded)
                .set_sources(SourceType::Monitor | SourceType::Window)
                .set_multiple(false),
        )
        .await
        .map_err(|e| e.to_string())?;
    let response = proxy
        .start(&session, None, Default::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    let stream = response
        .streams()
        .first()
        .ok_or("the portal returned no stream")?;
    let node_id = stream.pipe_wire_node_id();
    let fd = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await
        .map_err(|e| e.to_string())?;
    Ok((node_id, fd))
}
