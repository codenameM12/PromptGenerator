use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::CommandError;

use super::ffmpeg::{MediaTools, run_checked};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SamplingStrategy {
    FirstMiddleLast,
    Even,
    SceneChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameInfo {
    pub index: u32,
    pub timestamp_ms: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSet {
    pub frames: Vec<FrameInfo>,
    pub contact_sheet: Option<PathBuf>,
    pub scene_count: Option<u32>,
    pub capped: bool,
}

pub struct SampleRequest<'a> {
    pub source: &'a Path,
    pub duration_ms: u64,
    pub is_video: bool,
    pub strategy: SamplingStrategy,
    pub count: u8,
    pub max_images: u8,
    pub want_contact_sheet: bool,
    pub cache_dir: &'a Path,
    pub asset_hash: &'a str,
}

pub fn sample(tools: &MediaTools, request: &SampleRequest<'_>) -> Result<FrameSet, CommandError> {
    let count = request.count.clamp(1, 16) as usize;
    let max_images = request.max_images.clamp(1, 32) as usize;

    if !request.is_video {
        return Ok(FrameSet {
            frames: vec![FrameInfo {
                index: 0,
                timestamp_ms: 0,
                path: request.source.to_path_buf(),
            }],
            contact_sheet: None,
            scene_count: None,
            capped: false,
        });
    }

    if request.duration_ms == 0 {
        return Err(CommandError::media(
            "Cannot sample frames from a video with unknown duration",
        ));
    }

    let scene_timestamps = if request.strategy == SamplingStrategy::SceneChange {
        detect_scenes(tools, request.source)?
    } else {
        Vec::new()
    };

    let timestamps = timestamps_for(
        request.strategy,
        request.duration_ms,
        count,
        &scene_timestamps,
    );

    let directory = request
        .cache_dir
        .join("frames")
        .join(&request.asset_hash[..2])
        .join(request.asset_hash)
        .join(format!("{}-{count}", strategy_key(request.strategy)));

    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|error| CommandError::from_io(&error))?;
    }
    std::fs::create_dir_all(&directory).map_err(|error| CommandError::from_io(&error))?;

    let mut frames = Vec::new();
    for (index, timestamp) in timestamps.iter().enumerate() {
        let path = directory.join(format!("{index:03}.jpg"));
        extract_frame(tools, request.source, *timestamp, &path)?;
        frames.push(FrameInfo {
            index: index as u32,
            timestamp_ms: *timestamp,
            path,
        });
    }

    let scene_count = if request.strategy == SamplingStrategy::SceneChange {
        Some(scene_timestamps.len() as u32)
    } else {
        None
    };

    let should_sheet = request.want_contact_sheet || frames.len() > max_images;
    let contact_sheet = if should_sheet {
        Some(build_contact_sheet(
            tools,
            &directory,
            &frames,
            request.asset_hash,
        )?)
    } else {
        None
    };

    let capped = frames.len() > max_images;
    if capped {
        frames.truncate(max_images);
    }

    Ok(FrameSet {
        frames,
        contact_sheet,
        scene_count,
        capped,
    })
}

fn strategy_key(strategy: SamplingStrategy) -> &'static str {
    match strategy {
        SamplingStrategy::FirstMiddleLast => "fml",
        SamplingStrategy::Even => "even",
        SamplingStrategy::SceneChange => "scene",
    }
}

fn timestamps_for(
    strategy: SamplingStrategy,
    duration_ms: u64,
    count: usize,
    scene_timestamps: &[u64],
) -> Vec<u64> {
    let last = duration_ms.saturating_sub(50);

    let mut timestamps = match strategy {
        SamplingStrategy::Even => {
            if count <= 1 {
                vec![0]
            } else {
                (0..count)
                    .map(|index| {
                        let value = index as u64 * duration_ms / (count as u64 - 1);
                        value.min(last)
                    })
                    .collect()
            }
        }
        SamplingStrategy::FirstMiddleLast => {
            if count < 3 {
                return timestamps_for(SamplingStrategy::Even, duration_ms, count, &[]);
            }
            let mut values = vec![0, duration_ms / 2, last];
            if count > 3 {
                values.extend(timestamps_for(
                    SamplingStrategy::Even,
                    duration_ms,
                    count - 3,
                    &[],
                ));
            }
            values.sort_unstable();
            values.dedup();
            values
        }
        SamplingStrategy::SceneChange => {
            let mut values: Vec<u64> = scene_timestamps
                .iter()
                .take(count)
                .map(|value| (*value).min(last))
                .collect();
            if values.len() < count {
                for value in timestamps_for(SamplingStrategy::Even, duration_ms, count, &[]) {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                    if values.len() >= count {
                        break;
                    }
                }
                values.sort_unstable();
            }
            values
        }
    };

    timestamps.truncate(count);
    timestamps
}

fn extract_frame(
    tools: &MediaTools,
    source: &Path,
    timestamp_ms: u64,
    destination: &Path,
) -> Result<(), CommandError> {
    let seconds = timestamp_ms as f64 / 1000.0;
    let mut command = tools.ffmpeg_command();
    command
        .args(["-y", "-ss", &format!("{seconds:.3}"), "-i"])
        .arg(source)
        .args(["-frames:v", "1", "-q:v", "2"])
        .arg(destination);
    run_checked(command, "frame extraction")?;

    if !destination.is_file() {
        return Err(CommandError::media(format!(
            "frame extraction produced no file at {timestamp_ms} ms"
        )));
    }
    Ok(())
}

fn detect_scenes(tools: &MediaTools, source: &Path) -> Result<Vec<u64>, CommandError> {
    let output = tools
        .ffmpeg_command()
        .arg("-i")
        .arg(source)
        .args([
            "-filter:v",
            "select='gt(scene,0.3)',showinfo",
            "-an",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|error| CommandError::from_io(&error))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CommandError::media(format!(
            "scene detection failed: {}",
            stderr.trim()
        )));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut timestamps = Vec::new();
    for line in stderr.lines() {
        if let Some(index) = line.find("pts_time:") {
            let value = line[index + "pts_time:".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if let Ok(seconds) = value.parse::<f64>() {
                timestamps.push((seconds * 1000.0).round() as u64);
            }
        }
    }
    timestamps.sort_unstable();
    timestamps.dedup();
    Ok(timestamps)
}

fn build_contact_sheet(
    tools: &MediaTools,
    directory: &Path,
    frames: &[FrameInfo],
    asset_hash: &str,
) -> Result<PathBuf, CommandError> {
    let staging = directory.join("sheet");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|error| CommandError::from_io(&error))?;
    }
    std::fs::create_dir_all(&staging).map_err(|error| CommandError::from_io(&error))?;

    for (index, frame) in frames.iter().enumerate() {
        std::fs::copy(
            &frame.path,
            staging.join(format!("frame_{:03}.jpg", index + 1)),
        )
        .map_err(|error| CommandError::from_io(&error))?;
    }

    let columns = frames.len().clamp(1, 3);
    let rows = frames.len().div_ceil(columns).max(1);
    let filter = format!("scale=320:-2,tile={columns}x{rows}:padding=4:margin=4");
    let output = directory.join("contact-sheet.jpg");

    let mut command = tools.ffmpeg_command();
    command
        .args(["-y", "-framerate", "1", "-i"])
        .arg(staging.join("frame_%03d.jpg"))
        .args(["-vf", &filter, "-frames:v", "1", "-q:v", "3"])
        .arg(&output);
    run_checked(command, "contact sheet assembly")?;

    let _ = std::fs::remove_dir_all(&staging);
    let _ = asset_hash;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{FrameSet, SampleRequest, SamplingStrategy, sample};
    use crate::media::test_support::{
        make_image, make_two_scene_video, make_video, temp_dir, tools,
    };

    fn request<'a>(source: &'a std::path::Path, cache: &'a std::path::Path) -> SampleRequest<'a> {
        SampleRequest {
            source,
            duration_ms: 2000,
            is_video: true,
            strategy: SamplingStrategy::Even,
            count: 5,
            max_images: 9,
            want_contact_sheet: false,
            cache_dir: cache,
            asset_hash: "abcdef012345",
        }
    }

    #[test]
    fn even_sampling_is_deterministic() {
        let directory = temp_dir("frames-even");
        let cache = temp_dir("frames-even-cache");
        let tools = tools();
        let source = directory.join("clip.mp4");
        make_video(&tools, &source, "96x54", 2.0, 24, false);

        let first = sample(&tools, &request(&source, &cache)).expect("first");
        let second = sample(&tools, &request(&source, &cache)).expect("second");

        let first_times: Vec<u64> = first
            .frames
            .iter()
            .map(|frame| frame.timestamp_ms)
            .collect();
        let second_times: Vec<u64> = second
            .frames
            .iter()
            .map(|frame| frame.timestamp_ms)
            .collect();
        assert_eq!(first_times, second_times);
        assert_eq!(first_times.len(), 5);
        assert_eq!(first_times.first(), Some(&0));
        assert!(first.frames.iter().all(|frame| frame.path.is_file()));

        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn image_sampling_returns_the_source() {
        let directory = temp_dir("frames-image");
        let cache = temp_dir("frames-image-cache");
        let tools = tools();
        let source = directory.join("still.png");
        make_image(&tools, &source, "64x48");

        let mut value = request(&source, &cache);
        value.is_video = false;
        let result = sample(&tools, &value).expect("sample image");
        assert_eq!(result.frames.len(), 1);
        assert_eq!(result.frames[0].path, source);

        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn contact_sheet_respects_the_image_cap() {
        let directory = temp_dir("frames-cap");
        let cache = temp_dir("frames-cap-cache");
        let tools = tools();
        let source = directory.join("clip.mp4");
        make_video(&tools, &source, "96x54", 2.0, 24, false);

        let mut value = request(&source, &cache);
        value.count = 9;
        value.max_images = 4;

        let FrameSet {
            frames,
            contact_sheet,
            capped,
            ..
        } = sample(&tools, &value).expect("sample");

        assert!(capped);
        assert_eq!(frames.len(), 4);
        let sheet = contact_sheet.expect("contact sheet");
        assert!(sheet.is_file());

        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn detects_scene_changes() {
        let directory = temp_dir("frames-scene");
        let cache = temp_dir("frames-scene-cache");
        let tools = tools();
        let source = directory.join("scenes.mp4");
        if !make_two_scene_video(&tools, &source) {
            let _ = std::fs::remove_dir_all(directory);
            let _ = std::fs::remove_dir_all(cache);
            return;
        }

        let mut value = request(&source, &cache);
        value.strategy = SamplingStrategy::SceneChange;
        value.count = 4;

        let result = sample(&tools, &value).expect("sample scenes");
        assert!(result.scene_count.unwrap_or_default() >= 1);
        assert!(result.frames.len() >= 2);

        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_dir_all(cache);
    }
}
