use std::path::Path;

use crate::commands::CommandError;

use super::ffmpeg::{MediaTools, run_checked};

/// Writes a thumbnail no larger than 512 px on its longest side.
pub fn generate(
    tools: &MediaTools,
    source: &Path,
    destination: &Path,
    is_video: bool,
) -> Result<(), CommandError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CommandError::from_io(&error))?;
    }

    let mut command = tools.ffmpeg_command();
    command.arg("-y");
    if is_video {
        command.args(["-ss", "0.2"]);
    }
    command
        .arg("-i")
        .arg(source)
        .args([
            "-vf",
            "scale=min(512\\,iw):-2",
            "-frames:v",
            "1",
            "-q:v",
            "3",
        ])
        .arg(destination);

    run_checked(command, "thumbnail generation")?;

    if !destination.is_file() {
        return Err(CommandError::media("thumbnail was not created"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::generate;
    use crate::media::probe::probe;
    use crate::media::test_support::{make_image, temp_dir, tools};

    #[test]
    fn downsizes_large_images() {
        let directory = temp_dir("thumb-large");
        let tools = tools();
        let source = directory.join("large.png");
        let thumbnail = directory.join("thumb.jpg");
        make_image(&tools, &source, "1024x768");

        generate(&tools, &source, &thumbnail, false).expect("thumbnail");
        assert!(thumbnail.is_file());

        let probe = probe(&tools, &thumbnail).expect("probe thumbnail");
        assert!(probe.width <= 512);
        assert!(probe.height <= 512);
        assert_eq!(probe.width, 512);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn does_not_upscale_small_images() {
        let directory = temp_dir("thumb-small");
        let tools = tools();
        let source = directory.join("small.png");
        let thumbnail = directory.join("thumb.jpg");
        make_image(&tools, &source, "64x48");

        generate(&tools, &source, &thumbnail, false).expect("thumbnail");
        let probe = probe(&tools, &thumbnail).expect("probe thumbnail");
        assert_eq!(probe.width, 64);
        assert_eq!(probe.height, 48);

        let _ = std::fs::remove_dir_all(directory);
    }
}
