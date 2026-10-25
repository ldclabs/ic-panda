//! Bounded QR rendering for explicit pairing transport, never authorization.
use base64::{Engine, engine::general_purpose::STANDARD};
use dmsg_types::{Error, Result, ensure};

/// A PNG data URL with a four-module quiet zone. Oversized tokens retain
/// their text/share transport instead of using a different protocol.
///
/// # Errors
/// Returns `QuotaExceeded` if the data does not fit a QR code.
pub fn png_data(text: &str) -> Result<String> {
    ensure(text.len() <= 4096, Error::QuotaExceeded)?;
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|_| Error::QuotaExceeded)?;
    let width = code.width();
    let colors = code.to_colors();
    let scale = 6;
    let image = image::GrayImage::from_fn(
        ((width + 8) * scale) as u32,
        ((width + 8) * scale) as u32,
        |x, y| {
            let (x, y) = (x as usize / scale, y as usize / scale);
            image::Luma([
                if x >= 4
                    && y >= 4
                    && x < width + 4
                    && y < width + 4
                    && colors[(y - 4) * width + x - 4] == qrcode::Color::Dark
                {
                    0
                } else {
                    255
                },
            ])
        },
    );
    let mut output = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(output.into_inner())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_text_round_trips_without_a_wrapper() {
        let text = "dmsg-device-request-fixture";
        let uri = png_data(text).unwrap();
        let bytes = STANDARD
            .decode(uri.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().into_luma8();
        let mut image = rqrr::PreparedImage::prepare_from_greyscale(
            image.width() as usize,
            image.height() as usize,
            |x, y| image.get_pixel(x as u32, y as u32)[0],
        );
        let grids = image.detect_grids();
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].decode().unwrap().1, text);
        assert_eq!(png_data(&"x".repeat(4097)), Err(Error::QuotaExceeded));
    }
}
