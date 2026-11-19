use std::path::{Path, PathBuf};
use std::process::Command;

use super::ffmpeg::MediaTools;

pub fn tools() -> MediaTools {
    MediaTools::resolve().expect("resolve bundled media tools")
}

pub fn temp_dir(label: &str) -> PathBuf {
    let unique = format!(
        "prompt-studio-media-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

pub fn make_image(tools: &MediaTools, path: &Path, size: &str) {
    let mut command = tools.ffmpeg_command();
    command
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=red:s={size}:d=1"),
            "-frames:v",
            "1",
        ])
        .arg(path);
    assert_success(command, "generate image");
}

pub fn make_video(
    tools: &MediaTools,
    path: &Path,
    size: &str,
    seconds: f64,
    fps: u32,
    with_audio: bool,
) {
    let mut command = tools.ffmpeg_command();
    command.args([
        "-y",
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=size={size}:rate={fps}:duration={seconds}"),
    ]);
    if with_audio {
        command.args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:duration={seconds}"),
        ]);
    }
    command.args([
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
    ]);
    if with_audio {
        command.args(["-c:a", "aac", "-shortest"]);
    }
    command.arg(path);
    assert_success(command, "generate video");
}

pub fn make_two_scene_video(tools: &MediaTools, path: &Path) -> bool {
    let mut command = tools.ffmpeg_command();
    command
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=96x54:d=1",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=96x54:d=1",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(path);

    match command.output() {
        Ok(output) if output.status.success() => true,
        _ => {
            let _ = std::fs::remove_file(path);
            false
        }
    }
}

/// Returns false when the local FFmpeg build has no libx265 encoder.
pub fn make_hevc_video(tools: &MediaTools, path: &Path, size: &str) -> bool {
    let mut command = tools.ffmpeg_command();
    command
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={size}:rate=24:duration=1"),
            "-c:v",
            "libx265",
            "-pix_fmt",
            "yuv420p10le",
            "-tag:v",
            "hvc1",
            "-x265-params",
            "log-level=error",
        ])
        .arg(path);

    match command.output() {
        Ok(output) if output.status.success() => true,
        _ => {
            let _ = std::fs::remove_file(path);
            false
        }
    }
}

fn assert_success(mut command: Command, label: &str) {
    let output = command.output().expect("run ffmpeg");
    assert!(
        output.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
