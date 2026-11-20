//! In-process reader for common still-image formats: dimensions plus a
//! thumbnail without launching ffprobe/ffmpeg. Anything unusual returns `None`
//! so the caller falls back to the ffmpeg path.

use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};

use super::probe::ProbeData;

const FORMATS: &[&str] = &["png", "jpg", "jpeg", "jfif", "jpe", "webp", "bmp", "gif"];
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

#[cfg(test)]
mod jfif_tests {
    use super::*;

    #[test]
    fn jfif_files_are_read_as_jpeg_and_get_a_thumbnail() {
        let dir = std::env::temp_dir().join(format!("ps-jfif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let source = dir.join("photo.jfif");
        let image = image::RgbImage::from_pixel(40, 30, image::Rgb([200, 40, 40]));
        let mut file = std::fs::File::create(&source).expect("file");
        image::codecs::jpeg::JpegEncoder::new(&mut file)
            .encode_image(&image)
            .expect("encode");
        drop(file);
        let thumbnail = dir.join("thumb.jpg");
        let probe = read(&source, &thumbnail).expect("jfif should decode");
        assert_eq!((probe.width, probe.height), (40, 30));
        assert!(thumbnail.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }
}
