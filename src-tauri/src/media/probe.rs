use std::path::Path;

use serde_json::Value;

use crate::commands::CommandError;

use super::ffmpeg::MediaTools;

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeData {
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub display_width: u32,
    pub display_height: u32,
    pub fps: Option<f64>,
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
    pub bitrate: Option<u64>,
    pub has_audio: bool,
    pub pix_fmt: Option<String>,
    pub color_space: Option<String>,
    pub container: Option<String>,
    pub frame_count: Option<u64>,
}

pub fn probe(tools: &MediaTools, path: &Path) -> Result<ProbeData, CommandError> {
    parse_probe(&tools.ffprobe_json(path)?)
}

pub fn parse_probe(value: &Value) -> Result<ProbeData, CommandError> {
    let streams = value
        .get("streams")
        .and_then(Value::as_array)
        .ok_or_else(|| CommandError::media("ffprobe output has no streams"))?;

    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        .ok_or_else(|| CommandError::media("file has no video stream"))?;

    let has_audio = streams
        .iter()
        .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));

    let width = video
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let height = video
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default() as u32;
    let rotation = rotation_of(video);
    let (display_width, display_height) = if rotation.abs() % 180 == 90 {
        (height, width)
    } else {
        (width, height)
    };

    let format = value.get("format").cloned().unwrap_or(Value::Null);

    Ok(ProbeData {
        width,
        height,
        rotation,
        display_width,
        display_height,
        fps: parse_rate(
            video
                .get("avg_frame_rate")
                .and_then(Value::as_str)
                .or_else(|| video.get("r_frame_rate").and_then(Value::as_str)),
        ),
        duration_ms: seconds_to_ms(
            format
                .get("duration")
                .and_then(Value::as_str)
                .or_else(|| video.get("duration").and_then(Value::as_str)),
        ),
        codec: string_field(video, "codec_name"),
        bitrate: format
            .get("bit_rate")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok()),
        has_audio,
        pix_fmt: string_field(video, "pix_fmt"),
        color_space: string_field(video, "color_space"),
        container: string_field(&format, "format_name"),
        frame_count: video
            .get("nb_frames")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok()),
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn rotation_of(video: &Value) -> i32 {
    if let Some(list) = video.get("side_data_list").and_then(Value::as_array) {
        for item in list {
            if let Some(rotation) = item.get("rotation").and_then(Value::as_i64) {
                return rotation as i32;
            }
        }
    }

    video
        .get("tags")
        .and_then(|tags| tags.get("rotate"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or_default()
}

fn parse_rate(value: Option<&str>) -> Option<f64> {
    let value = value?;
    let (numerator, denominator) = value.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    if denominator == 0.0 {
        None
    } else {
        Some(numerator / denominator)
    }
}

fn seconds_to_ms(value: Option<&str>) -> Option<u64> {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .map(|seconds| (seconds * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::{parse_probe, probe};
    use crate::media::test_support::{make_image, make_video, temp_dir, tools};

    #[test]
    fn parses_rotation_from_side_data() {
        let value = serde_json::json!({
            "streams": [{
                "codec_type": "video",
                "width": 1920,
                "height": 1080,
                "codec_name": "h264",
                "side_data_list": [{ "rotation": -90 }]
            }],
            "format": { "duration": "2.5", "format_name": "mov,mp4" }
        });

        let parsed = parse_probe(&value).expect("parse");
        assert_eq!(parsed.rotation, -90);
        assert_eq!(parsed.display_width, 1080);
        assert_eq!(parsed.display_height, 1920);
        assert_eq!(parsed.duration_ms, Some(2500));
    }

    #[test]
    fn probes_generated_media() {
        let directory = temp_dir("probe");
        let tools = tools();

        let image = directory.join("still.png");
        make_image(&tools, &image, "64x48");
        let image_probe = probe(&tools, &image).expect("probe image");
        assert_eq!(image_probe.width, 64);
        assert_eq!(image_probe.height, 48);
        assert!(!image_probe.has_audio);

        let video = directory.join("clip.mp4");
        make_video(&tools, &video, "96x54", 1.0, 24, true);
        let video_probe = probe(&tools, &video).expect("probe video");
        assert_eq!(video_probe.width, 96);
        assert_eq!(video_probe.height, 54);
        assert!(video_probe.has_audio);
        assert!((video_probe.fps.unwrap_or_default() - 24.0).abs() < 0.5);
        assert!(video_probe.duration_ms.unwrap_or_default() >= 900);

        let _ = std::fs::remove_dir_all(directory);
    }
}
