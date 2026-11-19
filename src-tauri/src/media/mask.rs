use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::commands::CommandError;
use crate::commands::edits::MaskPolarity;

pub struct MaskExportRequest<'a> {
    pub source: &'a Path,
    pub alternate_root: Option<&'a Path>,
    pub mask: &'a [u8],
    pub mask_width: u32,
    pub mask_height: u32,
    pub target_width: u32,
    pub target_height: u32,
    pub feather: u32,
    pub invert: bool,
    pub polarity: MaskPolarity,
}

/// Resizes the painted mask to the target resolution, feathers it, applies
/// inversion and the target's polarity, then writes an 8-bit grayscale PNG.
pub fn export(request: &MaskExportRequest<'_>) -> Result<PathBuf, CommandError> {
    if request.mask_width == 0 || request.mask_height == 0 {
        return Err(CommandError::validation("Mask dimensions must be positive"));
    }
    if request.target_width == 0 || request.target_height == 0 {
        return Err(CommandError::validation(
            "Target mask dimensions must be positive",
        ));
    }
    let expected = request.mask_width as usize * request.mask_height as usize;
    if request.mask.len() != expected {
        return Err(CommandError::validation(format!(
            "Mask buffer has {} bytes, expected {expected}",
            request.mask.len()
        )));
    }

    let resized = resize_nearest(
        request.mask,
        request.mask_width as usize,
        request.mask_height as usize,
        request.target_width as usize,
        request.target_height as usize,
    );
    let feathered = box_blur(
        &resized,
        request.target_width as usize,
        request.target_height as usize,
        request.feather.min(64) as usize,
    );

    let invert = request.invert;
    let flip_polarity = request.polarity == MaskPolarity::BlackKeeps;
    let pixels: Vec<u8> = feathered
        .into_iter()
        .map(|value| {
            let mut value = if invert { 255 - value } else { value };
            if flip_polarity {
                value = 255 - value;
            }
            value
        })
        .collect();

    let directory = output_directory(request.source, request.alternate_root);
    std::fs::create_dir_all(&directory)
        .map_err(|error| CommandError::write_io(&error, &directory, "the mask"))?;

    let output = unique_mask_path(&directory, request.source);
    let temporary = directory.join(format!(".mask-{}.partial.png", Uuid::now_v7()));

    {
        let file = File::create(&temporary).map_err(|error| CommandError::from_io(&error))?;
        let mut encoder = png::Encoder::new(
            BufWriter::new(file),
            request.target_width,
            request.target_height,
        );
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| CommandError::media(format!("write mask header: {error}")))?;
        writer
            .write_image_data(&pixels)
            .map_err(|error| CommandError::media(format!("write mask pixels: {error}")))?;
    }

    std::fs::rename(&temporary, &output).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        CommandError::write_io(&error, &directory, "the mask")
    })?;

    Ok(output)
}

fn output_directory(source: &Path, alternate_root: Option<&Path>) -> PathBuf {
    match alternate_root {
        Some(root) => root.join("edited"),
        None => source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("edited"),
    }
}

fn unique_mask_path(directory: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("media");
    let candidate = directory.join(format!("{stem}-mask.png"));
    if !candidate.exists() {
        return candidate;
    }
    for index in 2..1000 {
        let candidate = directory.join(format!("{stem}-mask ({index}).png"));
        if !candidate.exists() {
            return candidate;
        }
    }
    candidate
}

fn resize_nearest(
    source: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(target_width * target_height);
    for y in 0..target_height {
        let source_y = (y * height / target_height).min(height - 1);
        for x in 0..target_width {
            let source_x = (x * width / target_width).min(width - 1);
            output.push(source[source_y * width + source_x]);
        }
    }
    output
}

fn box_blur(source: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    if radius == 0 {
        return source.to_vec();
    }

    let mut horizontal = vec![0_u8; source.len()];
    for y in 0..height {
        let mut prefix = vec![0_u32; width + 1];
        for x in 0..width {
            prefix[x + 1] = prefix[x] + source[y * width + x] as u32;
        }
        for x in 0..width {
            let start = x.saturating_sub(radius);
            let end = (x + radius).min(width - 1);
            let count = (end - start + 1) as u32;
            horizontal[y * width + x] = ((prefix[end + 1] - prefix[start]) / count) as u8;
        }
    }

    let mut vertical = vec![0_u8; source.len()];
    for x in 0..width {
        let mut prefix = vec![0_u32; height + 1];
        for y in 0..height {
            prefix[y + 1] = prefix[y] + horizontal[y * width + x] as u32;
        }
        for y in 0..height {
            let start = y.saturating_sub(radius);
            let end = (y + radius).min(height - 1);
            let count = (end - start + 1) as u32;
            vertical[y * width + x] = ((prefix[end + 1] - prefix[start]) / count) as u8;
        }
    }

    vertical
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::BufReader;

    use super::{MaskExportRequest, export};
    use crate::commands::edits::MaskPolarity;
    use crate::media::test_support::temp_dir;

    fn read_mask(path: &std::path::Path) -> (Vec<u8>, u32, u32) {
        let decoder = png::Decoder::new(BufReader::new(File::open(path).expect("open")));
        let mut reader = decoder.read_info().expect("info");
        let mut buffer = vec![0_u8; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut buffer).expect("frame");
        buffer.truncate(info.buffer_size());
        (buffer, info.width, info.height)
    }

    #[test]
    fn resizes_and_applies_polarity() {
        let directory = temp_dir("mask-export");
        let source = directory.join("clip.mp4");
        std::fs::write(&source, b"x").expect("source");

        let mask = vec![255, 0, 255, 0];
        let request = MaskExportRequest {
            source: &source,
            alternate_root: None,
            mask: &mask,
            mask_width: 2,
            mask_height: 2,
            target_width: 4,
            target_height: 4,
            feather: 0,
            invert: false,
            polarity: MaskPolarity::WhiteKeeps,
        };

        let path = export(&request).expect("export");
        let (pixels, width, height) = read_mask(&path);
        assert_eq!((width, height), (4, 4));
        assert_eq!(pixels[0], 255);
        assert_eq!(pixels[3], 0);
        assert!(path.to_string_lossy().contains("edited"));

        let inverted = MaskExportRequest {
            polarity: MaskPolarity::BlackKeeps,
            ..request
        };
        let path = export(&inverted).expect("export inverted");
        let (pixels, _, _) = read_mask(&path);
        assert_eq!(pixels[0], 0);
        assert_eq!(pixels[3], 255);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_wrong_buffer_size() {
        let directory = temp_dir("mask-invalid");
        let source = directory.join("clip.mp4");
        std::fs::write(&source, b"x").expect("source");

        let mask = vec![255, 0];
        let error = export(&MaskExportRequest {
            source: &source,
            alternate_root: None,
            mask: &mask,
            mask_width: 2,
            mask_height: 2,
            target_width: 4,
            target_height: 4,
            feather: 0,
            invert: false,
            polarity: MaskPolarity::WhiteKeeps,
        })
        .expect_err("wrong size");
        assert_eq!(error.code, "validation");

        let _ = std::fs::remove_dir_all(directory);
    }
}
