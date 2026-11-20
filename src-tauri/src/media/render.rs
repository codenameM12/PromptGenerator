use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;

use crate::commands::CommandError;
use crate::commands::edits::{EditOps, FitMode, TargetSpec};

use super::edit::{EditPlan, SourceInfo, cache_key, plan};
use super::ffmpeg::MediaTools;

pub struct RenderRequest<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub source_info: SourceInfo,
    pub ops: &'a EditOps,
    pub target: &'a TargetSpec,
    pub output_root: Option<&'a Path>,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub output_path: PathBuf,
    pub cache_key: String,
    pub plan: EditPlan,
}

pub fn render(
    tools: &MediaTools,
    request: &RenderRequest<'_>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f32),
) -> Result<RenderResult, CommandError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(CommandError::cancelled());
    }

    let plan = plan(request.ops, request.target, &request.source_info)?;
    let key = cache_key(request.source_hash, request.ops, request.target)?;

    let directory = output_directory(request.source, request.output_root);
    std::fs::create_dir_all(&directory)
        .map_err(|error| CommandError::write_io(&error, &directory, "the edited output"))?;

    let output = unique_output_path(&directory, request.source, &plan.format);
    let temporary = directory.join(format!(
        ".{}.{}.partial.{}",
        request
            .source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("media"),
        Uuid::now_v7(),
        plan.format
    ));

    let mut command = tools.ffmpeg_command();
    command.arg("-y");

    if request.source_info.is_video {
        if let Some(trim) = &plan.trim {
            command
                .arg("-ss")
                .arg(format!("{:.3}", trim.start_ms as f64 / 1000.0));
        }
        if plan.loop_video {
            command.args(["-stream_loop", "-1"]);
        }
        command.arg("-i").arg(request.source);

        let duration_ms = if plan.loop_video {
            request.source_info.duration_ms.unwrap_or_else(|| {
                plan.trim
                    .as_ref()
                    .map(|trim| trim.end_ms - trim.start_ms)
                    .unwrap_or(0)
            })
        } else if let Some(trim) = &plan.trim {
            trim.end_ms - trim.start_ms
        } else {
            0
        };
        if duration_ms > 0 {
            command
                .arg("-t")
                .arg(format!("{:.3}", duration_ms as f64 / 1000.0));
        }
    } else {
        command.arg("-i").arg(request.source);
        if let Some(mask) = &request.ops.mask_path {
            command.arg("-i").arg(mask);
        }
    }

    let filter = build_filter(&plan, request.ops, &request.source_info)?;
    let masked = !request.source_info.is_video && request.ops.mask_path.is_some();
    if masked {
        // The mask (white = keep) is applied to the source first, so crop,
        // rotate and flip then act on the masked image. Everything outside
        // the painted area becomes opaque black, which every model handles
        // the same way (unlike transparency).
        let tail = if filter.is_empty() {
            String::new()
        } else {
            format!(",{filter}")
        };
        let graph = format!(
            "[1:v]format=gray[mg];[mg][0:v]scale2ref=flags=bilinear[m][s];             [s]format=rgba[sa];[sa][m]alphamerge,split=2[t][u];             [u]drawbox=x=0:y=0:w=iw:h=ih:color=black:t=fill[bg];             [bg][t]overlay=format=auto,format=rgb24{tail}[out]"
        );
        command.args(["-filter_complex", &graph, "-map", "[out]"]);
    } else if !filter.is_empty() {
        command.args(["-vf", &filter]);
    }

    if request.source_info.is_video {
        if let Some(fps) = plan.fps {
            command.arg("-r").arg(format!("{fps}"));
        }
        if plan.remove_audio {
            command.arg("-an");
        }

        if plan.format == "webm" {
            command.args(["-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0"]);
            if !plan.remove_audio {
                command.args(["-c:a", "libopus", "-b:a", "128k"]);
            }
        } else {
            command.args([
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "20",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ]);
            if !plan.remove_audio {
                command.args(["-c:a", "aac", "-b:a", "192k"]);
            }
        }
    } else {
        command.arg("-frames:v").arg("1");
        match plan.format.as_str() {
            "jpg" | "jpeg" => {
                command.args(["-c:v", "mjpeg", "-q:v", "2"]);
            }
            "webp" => {
                command.args(["-c:v", "libwebp", "-quality", "90"]);
            }
            _ => {
                command.args(["-c:v", "png"]);
            }
        }
    }

    if request.source_info.is_video {
        command.arg("-progress").arg("pipe:1").arg("-nostats");
    }

    command.arg(&temporary);

    if request.source_info.is_video {
        run_with_progress(
            &mut command,
            &temporary,
            duration_for_progress(&plan, &request.source_info),
            cancel,
            &mut progress,
        )?;
    } else {
        let output = command
            .output()
            .map_err(|error| CommandError::from_io(&error))?;
        if !output.status.success() {
            let _ = std::fs::remove_file(&temporary);
            return Err(CommandError::media(format!(
                "edit failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }

    if !temporary.is_file() {
        return Err(CommandError::media("edit produced no output"));
    }

    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&temporary);
        return Err(CommandError::cancelled());
    }

    std::fs::rename(&temporary, &output).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        CommandError::write_io(&error, &directory, "the edited output")
    })?;

    progress(1.0);

    Ok(RenderResult {
        output_path: output,
        cache_key: key,
        plan,
    })
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

fn unique_output_path(directory: &Path, source: &Path, format: &str) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("media");
    let candidate = directory.join(format!("{stem}.{format}"));
    if !candidate.exists() {
        return candidate;
    }

    for index in 2..1000 {
        let candidate = directory.join(format!("{stem} ({index}).{format}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    candidate
}

fn build_filter(
    plan: &EditPlan,
    ops: &EditOps,
    source: &SourceInfo,
) -> Result<String, CommandError> {
    let mut filters = Vec::new();

    if let Some(crop) = &plan.crop {
        filters.push(format!(
            "crop={}:{}:{}:{}",
            crop.width, crop.height, crop.x, crop.y
        ));
    }

    match ops.rotate.rem_euclid(360) {
        0 => {}
        90 => filters.push("transpose=1".into()),
        180 => {
            filters.push("hflip".into());
            filters.push("vflip".into());
        }
        270 => filters.push("transpose=2".into()),
        _ => {
            return Err(CommandError::validation(
                "Rotation must be 0, 90, 180, or 270",
            ));
        }
    }

    // Flips apply after the rotation, including at 180 degrees (where the
    // rotation itself is hflip+vflip); skipping them there dropped the user's flip.
    if ops.flip_h {
        filters.push("hflip".into());
    }
    if ops.flip_v {
        filters.push("vflip".into());
    }

    let width = plan.resolution.width;
    let height = plan.resolution.height;

    match ops.fit {
        FitMode::Fit => filters.push(format!("scale={width}:{height}:flags=lanczos")),
        FitMode::Fill => filters.push(format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase:flags=lanczos,crop={width}:{height}"
        )),
        FitMode::Pad => {
            let color = pad_color(ops.pad_color.as_deref());
            filters.push(format!(
                "scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color={color}"
            ));
        }
    }

    filters.push("setsar=1".into());

    let _ = source;
    Ok(filters.join(","))
}

fn pad_color(value: Option<&str>) -> String {
    let value = value.unwrap_or("black");
    let safe: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '@' | '.' | '#')
        })
        .collect();
    if let Some(hex) = safe.strip_prefix('#') {
        format!("0x{hex}")
    } else if safe.is_empty() {
        "black".into()
    } else {
        safe
    }
}

fn duration_for_progress(plan: &EditPlan, source: &SourceInfo) -> f64 {
    if plan.loop_video {
        return source.duration_ms.unwrap_or(0) as f64 / 1000.0;
    }
    if let Some(trim) = &plan.trim {
        return (trim.end_ms - trim.start_ms) as f64 / 1000.0;
    }
    source.duration_ms.unwrap_or(0) as f64 / 1000.0
}

fn run_with_progress(
    command: &mut Command,
    temporary: &Path,
    duration_seconds: f64,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(f32),
) -> Result<(), CommandError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| CommandError::from_io(&error))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CommandError::internal("ffmpeg produced no stdout"))?;
    let reader = BufReader::new(stdout);
    let mut last_error = String::new();

    for line in reader.lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(temporary);
            return Err(CommandError::cancelled());
        }

        if let Some(value) = line.strip_prefix("out_time_ms=")
            && let Ok(microseconds) = value.parse::<f64>()
            && duration_seconds > 0.0
        {
            let ratio = (microseconds / 1_000_000.0 / duration_seconds).clamp(0.0, 1.0);
            progress(ratio as f32);
        }
        if let Some(value) = line.strip_prefix("out_time_us=")
            && let Ok(microseconds) = value.parse::<f64>()
            && duration_seconds > 0.0
        {
            let ratio = (microseconds / 1_000_000.0 / duration_seconds).clamp(0.0, 1.0);
            progress(ratio as f32);
        }
    }

    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            last_error.push_str(&line);
            last_error.push('\n');
        }
    }

    let status = child
        .wait()
        .map_err(|error| CommandError::from_io(&error))?;
    if !status.success() {
        let _ = std::fs::remove_file(temporary);
        return Err(CommandError::media(format!(
            "edit failed: {}",
            last_error.trim()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::{RenderRequest, render};
    use crate::commands::edits::{EditOps, FitMode, MaskPolarity, TargetSpec, Trim};
    use crate::media::edit::SourceInfo;
    use crate::media::probe::probe;
    use crate::media::test_support::{make_image, make_video, temp_dir, tools};

    fn ops() -> EditOps {
        EditOps {
            crop: None,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            fit: FitMode::Fit,
            trim: None,
            fps: None,
            remove_audio: false,
            mask_path: None,
            format: None,
            loop_video: false,
            pad_color: None,
        }
    }

    fn target() -> TargetSpec {
        TargetSpec {
            preset_id: "test".into(),
            width: 0,
            height: 0,
            dimension_multiple: 16,
            mask_polarity: MaskPolarity::WhiteKeeps,
        }
    }

    #[test]
    fn mask_blanks_everything_outside_the_painted_area() {
        let directory = temp_dir("render-mask");
        let tools = tools();
        let source = directory.join("source.png");
        make_image(&tools, &source, "64x48");

        // White square (keep) in the middle of a black (remove) mask.
        let mask = directory.join("mask.png");
        let mut command = tools.ffmpeg_command();
        command
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x48:d=1",
                "-vf",
                "drawbox=x=16:y=12:w=32:h=24:color=white:t=fill,format=gray",
                "-frames:v",
                "1",
            ])
            .arg(&mask);
        crate::media::ffmpeg::run_checked(command, "make mask").expect("mask");

        let mut value = ops();
        value.mask_path = Some(mask.display().to_string());
        value.format = Some("png".into());

        let result = render(
            &tools,
            &RenderRequest {
                source: &source,
                source_hash: "hash",
                source_info: SourceInfo {
                    width: 64,
                    height: 48,
                    duration_ms: None,
                    fps: None,
                    is_video: false,
                },
                ops: &value,
                target: &target(),
                output_root: None,
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("render");

        let output = image::open(&result.output_path).expect("decode").to_rgb8();
        assert_eq!(output.get_pixel(2, 2).0, [0, 0, 0], "outside is black");
        let inside = output.get_pixel(32, 24).0;
        assert!(inside[0] > 200 && inside[1] < 60, "inside keeps the source: {inside:?}");

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn renders_a_cropped_image() {
        let directory = temp_dir("render-image");
        let tools = tools();
        let source = directory.join("source.png");
        make_image(&tools, &source, "800x600");

        let mut value = ops();
        value.crop = Some(crate::commands::edits::Crop {
            x: 100,
            y: 50,
            width: 400,
            height: 300,
        });
        value.format = Some("png".into());

        let result = render(
            &tools,
            &RenderRequest {
                source: &source,
                source_hash: "hash",
                source_info: SourceInfo {
                    width: 800,
                    height: 600,
                    duration_ms: None,
                    fps: None,
                    is_video: false,
                },
                ops: &value,
                target: &target(),
                output_root: None,
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("render");

        assert!(result.output_path.is_file());
        let probe = probe(&tools, &result.output_path).expect("probe");
        assert_eq!(probe.width, 400);
        assert_eq!(probe.height, 304);
        assert!(result.output_path.to_string_lossy().contains("edited"));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn renders_a_trimmed_silent_video() {
        let directory = temp_dir("render-video");
        let tools = tools();
        let source = directory.join("source.mp4");
        make_video(&tools, &source, "96x54", 2.0, 24, true);

        let mut value = ops();
        value.trim = Some(Trim {
            start_ms: 500,
            end_ms: 1500,
        });
        value.fps = Some(30.0);
        value.remove_audio = true;
        value.format = Some("mp4".into());

        let result = render(
            &tools,
            &RenderRequest {
                source: &source,
                source_hash: "hash",
                source_info: SourceInfo {
                    width: 96,
                    height: 54,
                    duration_ms: Some(2000),
                    fps: Some(24.0),
                    is_video: true,
                },
                ops: &value,
                target: &target(),
                output_root: None,
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("render");

        let probe = probe(&tools, &result.output_path).expect("probe");
        assert!(!probe.has_audio);
        assert!((probe.fps.unwrap_or_default() - 30.0).abs() < 1.0);
        assert!(probe.duration_ms.unwrap_or_default() >= 900);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn collisions_get_a_numeric_suffix() {
        let directory = temp_dir("render-collision");
        let tools = tools();
        let source = directory.join("source.png");
        make_image(&tools, &source, "64x48");

        let value = ops();
        let request = RenderRequest {
            source: &source,
            source_hash: "hash",
            source_info: SourceInfo {
                width: 64,
                height: 48,
                duration_ms: None,
                fps: None,
                is_video: false,
            },
            ops: &value,
            target: &target(),
            output_root: None,
        };

        let first = render(&tools, &request, &AtomicBool::new(false), |_| {}).expect("first");
        let second = render(&tools, &request, &AtomicBool::new(false), |_| {}).expect("second");
        assert_ne!(first.output_path, second.output_path);
        assert!(second.output_path.to_string_lossy().contains("(2)"));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn writes_to_an_alternate_root() {
        let directory = temp_dir("render-alt");
        let root = temp_dir("render-alt-root");
        let tools = tools();
        let source = directory.join("source.png");
        make_image(&tools, &source, "64x48");

        let value = ops();
        let result = render(
            &tools,
            &RenderRequest {
                source: &source,
                source_hash: "hash",
                source_info: SourceInfo {
                    width: 64,
                    height: 48,
                    duration_ms: None,
                    fps: None,
                    is_video: false,
                },
                ops: &value,
                target: &target(),
                output_root: Some(&root),
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("render");

        assert!(result.output_path.starts_with(root.join("edited")));
        assert!(result.output_path.is_file());

        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_leaves_no_output() {
        let directory = temp_dir("render-cancel");
        let tools = tools();
        let source = directory.join("source.png");
        make_image(&tools, &source, "64x48");

        let cancel = AtomicBool::new(true);
        let value = ops();
        let error = render(
            &tools,
            &RenderRequest {
                source: &source,
                source_hash: "hash",
                source_info: SourceInfo {
                    width: 64,
                    height: 48,
                    duration_ms: None,
                    fps: None,
                    is_video: false,
                },
                ops: &value,
                target: &target(),
                output_root: None,
            },
            &cancel,
            |_| {},
        )
        .expect_err("cancel");
        assert_eq!(error.code, "cancelled");

        assert!(!directory.join("edited").exists());
        let _ = std::fs::remove_dir_all(directory);
    }
}
