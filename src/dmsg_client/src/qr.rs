//! Bounded QR rendering and image reading. A QR code transports text, never authority.
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

/// Largest image decoded by a QR reader.
const MAX_PIXELS: u64 = 40_000_000;

/// The text of the first readable QR code in an image (PNG, JPEG, GIF or
/// WebP), such as an `otpauth://` URI.
///
/// # Errors
/// Returns `Error::InvalidInput("image")` for an unreadable or oversized
/// image and `Error::NotFound` when no QR code reads.
pub fn scan(image: &[u8]) -> Result<String> {
    let invalid = || Error::InvalidInput("image".into());
    let reader = image::ImageReader::new(std::io::Cursor::new(image))
        .with_guessed_format()
        .map_err(|_| invalid())?;
    let (width, height) = reader.into_dimensions().map_err(|_| invalid())?;
    ensure(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        invalid(),
    )?;
    let luma = image::load_from_memory(image)
        .map_err(|_| invalid())?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(
        luma.width() as usize,
        luma.height() as usize,
        |x, y| luma.get_pixel(x as u32, y as u32)[0],
    );
    prepared
        .detect_grids()
        .into_iter()
        .find_map(|grid| grid.decode().ok().map(|(_, text)| text))
        .ok_or(Error::NotFound)
}

/// Decode a bounded camera luminance frame without encoding an intermediate
/// image. The frame is text transport only and cannot approve an operation.
///
/// # Errors
/// Invalid dimensions, more than four million pixels, or an unreadable code
/// are rejected before or during bounded image analysis.
pub fn scan_luma(width: u32, height: u32, pixels: &[u8]) -> Result<String> {
    let count = u64::from(width) * u64::from(height);
    ensure(
        width > 0 && height > 0 && count <= 4_000_000 && count == pixels.len() as u64,
        Error::InvalidInput("QR frame".into()),
    )?;
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(width as usize, height as usize, |x, y| {
            pixels[y * width as usize + x]
        });
    prepared
        .detect_grids()
        .into_iter()
        .find_map(|grid| grid.decode().ok().map(|(_, text)| text))
        .ok_or(Error::NotFound)
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

    #[test]
    fn camera_frames_round_trip_and_reject_mismatched_or_oversized_buffers() {
        let text = "dmsg-device-request-fixture";
        let uri = png_data(text).unwrap();
        let bytes = STANDARD.decode(uri.split_once(',').unwrap().1).unwrap();
        let frame = image::load_from_memory(&bytes).unwrap().into_luma8();
        assert_eq!(
            scan_luma(frame.width(), frame.height(), frame.as_raw()).unwrap(),
            text
        );
        assert!(scan_luma(0, 10, &[]).is_err());
        assert!(scan_luma(2, 2, &[0; 3]).is_err());
        assert!(scan_luma(u32::MAX, u32::MAX, &[0]).is_err());
        assert!(scan_luma(2001, 2000, &[0]).is_err());
    }
}
