use axum::extract::Path;
use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::constants::{LENA, MIKK, SARA};

/// A flat colour per user, so "the right avatar for the right person" is
/// verifiable at a glance.
pub(crate) async fn user_image(Path(user_id): Path<String>) -> Response {
    let rgb = match user_id.as_str() {
        LENA => [0xE8, 0x6A, 0x33],
        MIKK => [0x2E, 0x9E, 0x6B],
        SARA => [0x3B, 0x74, 0xD8],
        _ => [0xC9, 0x9A, 0x1E],
    };
    let mut img = image::RgbImage::new(128, 128);
    for (x, y, px) in img.enumerate_pixels_mut() {
        // A diagonal band makes it obvious this is a real image, not a colour
        // block the client painted itself.
        let band = ((x + y) / 16) % 2 == 0;
        let scale = if band { 1.0 } else { 0.82 };
        *px = image::Rgb([
            (rgb[0] as f32 * scale) as u8,
            (rgb[1] as f32 * scale) as u8,
            (rgb[2] as f32 * scale) as u8,
        ]);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();

    ([(header::CONTENT_TYPE, "image/png")], png.into_inner()).into_response()
}
