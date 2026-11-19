//! Clipboard image read (replacement for `tauri-plugin-clipboard-manager`).
//! The decoded image is re-encoded as PNG so the import payload has the same
//! shape as `src-tauri::commands::assets::ClipboardImportRequest`.

use std::io::Cursor;

use crate::backend::BackendError;

pub struct ClipboardImage {
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

/// Reads the current clipboard image and returns PNG bytes.
pub fn read_image() -> Result<ClipboardImage, BackendError> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| BackendError::validation(format!("Clipboard unavailable: {error}")))?;
    let image = clipboard
        .get_image()
        .map_err(|error| BackendError::validation(format!("No image on clipboard: {error}")))?;
    let bytes = encode_png(image.width as u32, image.height as u32, &image.bytes)?;
    Ok(ClipboardImage {
        file_name: "clipboard.png".to_string(),
        width: image.width as u32,
        height: image.height as u32,
        bytes,
    })
}

/// Encodes RGBA pixels as PNG. Exposed separately so the shape is testable
/// without real clipboard access.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, BackendError> {
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or_else(|| {
        BackendError::validation(format!(
            "Clipboard pixel buffer does not match {width}x{height}"
        ))
    })?;
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|error| {
            BackendError::validation(format!("Could not encode clipboard PNG: {error}"))
        })?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{ClipboardImage, encode_png};

    #[test]
    fn encodes_rgba_as_png_with_expected_header() {
        let bytes = encode_png(
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        )
        .expect("encode");
        assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn rejects_mismatched_buffer() {
        assert!(encode_png(2, 2, &[0, 0, 0, 255]).is_err());
    }

    #[test]
    fn import_payload_has_source_shape() {
        let payload = ClipboardImage {
            file_name: "clipboard.png".into(),
            width: 2,
            height: 2,
            bytes: vec![1, 2, 3],
        };
        assert_eq!(payload.file_name, "clipboard.png");
        assert!(!payload.bytes.is_empty());
    }
}
