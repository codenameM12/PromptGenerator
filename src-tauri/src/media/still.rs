//! In-process reader for common still-image formats: dimensions plus a
//! thumbnail without launching ffprobe/ffmpeg. Anything unusual returns `None`
//! so the caller falls back to the ffmpeg path.

use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};

use super::probe::ProbeData;

const FORMATS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif"];
const THUMB_WIDTH: u32 = 512;

pub fn read(source: &Path, thumbnail: &Path) -> Option<ProbeData> {
    let extension = source.extension()?.to_str()?.to_ascii_lowercase();
    if !FORMATS.contains(&extension.as_str()) {
        return None;
    }
    let reader = ImageReader::open(source).ok()?.with_guessed_format().ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut decoded = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(orientation) = orientation {
        decoded.apply_orientation(orientation);
    }
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return None;
    }

    if !thumbnail.is_file() {
        let preview = if width > THUMB_WIDTH {
            let target_height = ((height as u64 * THUMB_WIDTH as u64) / width as u64).max(1) as u32;
            decoded.thumbnail_exact(THUMB_WIDTH, target_height)
        } else {
            decoded
        };
        if let Some(parent) = thumbnail.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }
        // JPEG has no alpha channel.
        let rgb = preview.to_rgb8();
        let temporary = thumbnail.with_extension("jpg.partial");
        {
            let file = std::fs::File::create(&temporary).ok()?;
            let mut writer = std::io::BufWriter::new(file);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 88);
            rgb.write_with_encoder(encoder).ok()?;
        }
        std::fs::rename(&temporary, thumbnail).ok()?;
    }

    Some(ProbeData {
        width,
        height,
        rotation: 0,
        display_width: width,
        display_height: height,
        fps: None,
        duration_ms: None,
        codec: Some(extension),
        bitrate: None,
        has_audio: false,
        pix_fmt: None,
        color_space: None,
        container: None,
        frame_count: None,
    })
}
