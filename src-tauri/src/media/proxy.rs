use std::path::Path;

use crate::commands::CommandError;

use super::ffmpeg::{MediaTools, run_checked};
use super::probe::ProbeData;

const PLAYABLE_CODECS: &[&str] = &["h264", "vp8", "vp9", "av1"];
const PLAYABLE_PIX_FMTS: &[&str] = &["yuv420p", "yuvj420p", "nv12"];

/// WebView2 can play H.264/VP8/VP9/AV1 in 8-bit 4:2:0. Everything else gets a proxy.
pub fn needs_proxy(probe: &ProbeData) -> bool {
    let codec_ok = probe
        .codec
        .as_deref()
        .map(|codec| PLAYABLE_CODECS.contains(&codec))
        .unwrap_or(false);
    let pix_ok = probe
        .pix_fmt
        .as_deref()
        .map(|pix_fmt| PLAYABLE_PIX_FMTS.contains(&pix_fmt))
        .unwrap_or(false);
    !codec_ok || !pix_ok
}

pub fn generate(
    tools: &MediaTools,
    source: &Path,
    destination: &Path,
    has_audio: bool,
) -> Result<(), CommandError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CommandError::from_io(&error))?;
    }

    let mut command = tools.ffmpeg_command();
    command.arg("-y").arg("-i").arg(source).args([
        "-vf",
        "scale=min(1280\\,iw):-2",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "23",
        "-pix_fmt",
        "yuv420p",
    ]);

    if has_audio {
        command.args(["-c:a", "aac", "-b:a", "128k"]);
    }

    command.args(["-movflags", "+faststart"]).arg(destination);
    run_checked(command, "preview proxy generation")?;

    if !destination.is_file() {
        return Err(CommandError::media("preview proxy was not created"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{generate, needs_proxy};
    use crate::media::hash::hash_file;
    use crate::media::probe::probe;
    use crate::media::test_support::{make_hevc_video, make_video, temp_dir, tools};

    #[test]
    fn h264_is_playable_without_a_proxy() {
        let directory = temp_dir("proxy-h264");
        let tools = tools();
        let source = directory.join("clip.mp4");
        make_video(&tools, &source, "96x54", 1.0, 24, true);

        let probe = probe(&tools, &source).expect("probe");
        assert!(!needs_proxy(&probe));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn generates_a_playable_proxy_for_hevc() {
        let directory = temp_dir("proxy-hevc");
        let tools = tools();
        let source = directory.join("hevc.mp4");
        if !make_hevc_video(&tools, &source, "96x54") {
            let _ = std::fs::remove_dir_all(directory);
            return;
        }

        let source_probe = probe(&tools, &source).expect("probe source");
        assert!(needs_proxy(&source_probe));
        let source_hash = hash_file(&source).expect("hash source");

        let proxy = directory.join("proxy.mp4");
        generate(&tools, &source, &proxy, source_probe.has_audio).expect("proxy");
        assert!(proxy.is_file());

        let proxy_probe = probe(&tools, &proxy).expect("probe proxy");
        assert_eq!(proxy_probe.codec.as_deref(), Some("h264"));
        assert_eq!(proxy_probe.pix_fmt.as_deref(), Some("yuv420p"));
        assert_eq!(hash_file(&source).expect("hash source"), source_hash);

        let _ = std::fs::remove_dir_all(directory);
    }
}
