use axum::extract::Path;
use axum::http::header;
use axum::response::{IntoResponse, Response};

/// `shipit` is a green disc; `party_blob` is a disc that changes colour, as
/// an animated GIF, so it is plain on screen whether the frames are playing.
pub(crate) async fn emoji_image(Path(id): Path<String>) -> Response {
    let disc = |rgb: [u8; 3]| {
        image::RgbaImage::from_fn(64, 64, |x, y| {
            let (dx, dy) = (x as f32 - 31.5, y as f32 - 31.5);
            if dx * dx + dy * dy < 30.0 * 30.0 {
                image::Rgba([rgb[0], rgb[1], rgb[2], 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        })
    };
    if id == "emoji-party_blob" {
        let mut gif = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .unwrap();
            for rgb in [[0xE8, 0x3A, 0x5A], [0xF2, 0xB1, 0x1D], [0x3B, 0x74, 0xD8]] {
                let delay = image::Delay::from_numer_denom_ms(300, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(disc(rgb), 0, 0, delay))
                    .unwrap();
            }
        }
        return ([(header::CONTENT_TYPE, "image/gif")], gif).into_response();
    }
    let mut png = std::io::Cursor::new(Vec::new());
    disc([0x2E, 0x9E, 0x6B])
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    ([(header::CONTENT_TYPE, "image/png")], png.into_inner()).into_response()
}
