use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use super::theme::ZED_THEME;

/// An extension the way the registry packages one: `extension.toml` and a
/// `themes` directory, gzipped.
pub(crate) async fn zed_extension_download(Path(id): Path<String>) -> Response {
    if id != "testserver-dusk" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    for (path, text) in [
        ("./extension.toml", "id = \"testserver-dusk\"\n"),
        ("./themes/testserver-dusk.json", ZED_THEME),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, path, text.as_bytes()).unwrap();
    }
    let bytes = archive.into_inner().unwrap().finish().unwrap();
    ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
}
