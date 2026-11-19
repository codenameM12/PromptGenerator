use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedMetadata {
    pub source: String,
    pub prompt: Option<String>,
    pub negative: Option<String>,
    pub parameters: Option<Value>,
    pub workflow: Option<Value>,
    pub exif: Option<Value>,
}

/// Extracts A1111 infotext, ComfyUI workflow/prompt chunks, generic PNG text,
/// and EXIF. Returns `None` when nothing recognized is embedded.
pub fn extract(path: &Path) -> Option<EmbeddedMetadata> {
    let mut result = EmbeddedMetadata {
        source: "none".into(),
        prompt: None,
        negative: None,
        parameters: None,
        workflow: None,
        exif: None,
    };
    let mut found = false;
    let mut text_chunks = Map::new();

    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("png"))
        .unwrap_or(false)
        && let Some(chunks) = png_text_chunks(path)
    {
        for (keyword, text) in chunks {
            match keyword.as_str() {
                "parameters" => {
                    let (prompt, negative, parameters) = parse_a1111(&text);
                    result.prompt = prompt;
                    result.negative = negative;
                    result.parameters = parameters;
                    result.source = "a1111".into();
                    found = true;
                }
                "prompt" | "workflow" => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        result.workflow = Some(value);
                        result.source = "comfyui".into();
                        found = true;
                    } else {
                        text_chunks.insert(keyword, Value::String(text));
                        found = true;
                    }
                }
                _ => {
                    text_chunks.insert(keyword, Value::String(text));
                    found = true;
                }
            }
        }
    }

    if let Some(exif) = read_exif(path) {
        result.exif = Some(exif);
        if result.source == "none" {
            result.source = "exif".into();
        }
        found = true;
    }

    if !text_chunks.is_empty() {
        if let Some(Value::Object(existing)) = &mut result.parameters {
            for (key, value) in text_chunks {
                existing.entry(key).or_insert(value);
            }
        } else {
            result.parameters = Some(Value::Object(text_chunks));
        }
    }

    found.then_some(result)
}

fn parse_a1111(text: &str) -> (Option<String>, Option<String>, Option<Value>) {
    let (body, settings) = match text.rsplit_once("\nSteps: ") {
        Some((body, rest)) => (body, Some(format!("Steps: {rest}"))),
        None => (text, None),
    };

    let (prompt, negative) = match body.split_once("\nNegative prompt: ") {
        Some((prompt, negative)) => (
            Some(prompt.trim().to_string()),
            Some(negative.trim().to_string()),
        ),
        None => (Some(body.trim().to_string()), None),
    };

    let parameters = settings.map(|settings| {
        let mut map = Map::new();
        for part in settings.split(", ") {
            if let Some((key, value)) = part.split_once(": ") {
                map.insert(
                    key.trim().to_string(),
                    Value::String(value.trim().to_string()),
                );
            }
        }
        Value::Object(map)
    });

    (
        prompt.filter(|value| !value.is_empty()),
        negative,
        parameters,
    )
}

fn png_text_chunks(path: &Path) -> Option<Vec<(String, String)>> {
    let decoder = png::Decoder::new(BufReader::new(File::open(path).ok()?));
    let reader = decoder.read_info().ok()?;
    let info = reader.info();

    let mut chunks = Vec::new();
    for chunk in &info.uncompressed_latin1_text {
        chunks.push((chunk.keyword.clone(), chunk.text.clone()));
    }
    for chunk in &info.utf8_text {
        if let Ok(text) = chunk.get_text() {
            chunks.push((chunk.keyword.clone(), text));
        }
    }
    for chunk in &info.compressed_latin1_text {
        if let Ok(text) = chunk.get_text() {
            chunks.push((chunk.keyword.clone(), text));
        }
    }

    Some(chunks)
}

fn read_exif(path: &Path) -> Option<Value> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;

    let mut map = Map::new();
    for field in exif.fields() {
        let key = field.tag.to_string();
        let value = field.display_value().with_unit(&exif).to_string();
        map.insert(key, Value::String(value));
    }

    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::extract;
    use crate::media::test_support::{make_image, temp_dir, tools};

    fn write_png(path: &std::path::Path, chunks: &[(&str, &str)]) {
        let file = File::create(path).expect("create png");
        let mut encoder = png::Encoder::new(file, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        for (keyword, text) in chunks {
            encoder
                .add_text_chunk(keyword.to_string(), text.to_string())
                .expect("text chunk");
        }
        let mut writer = encoder.write_header().expect("header");
        writer
            .write_image_data(&[255_u8; 2 * 2 * 4])
            .expect("pixels");
    }

    #[test]
    fn parses_a1111_infotext() {
        let directory = temp_dir("metadata-a1111");
        let path = directory.join("a1111.png");
        write_png(
            &path,
            &[(
                "parameters",
                "a red fox in snow\nNegative prompt: blurry, low quality\nSteps: 20, Sampler: Euler, CFG scale: 7",
            )],
        );

        let metadata = extract(&path).expect("metadata");
        assert_eq!(metadata.source, "a1111");
        assert_eq!(metadata.prompt.as_deref(), Some("a red fox in snow"));
        assert_eq!(metadata.negative.as_deref(), Some("blurry, low quality"));
        let parameters = metadata.parameters.expect("parameters");
        assert_eq!(parameters["Sampler"], "Euler");

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn parses_comfyui_workflow_json() {
        let directory = temp_dir("metadata-comfy");
        let path = directory.join("comfy.png");
        write_png(
            &path,
            &[(
                "prompt",
                r#"{"3":{"class_type":"KSampler","inputs":{"seed":42}}}"#,
            )],
        );

        let metadata = extract(&path).expect("metadata");
        assert_eq!(metadata.source, "comfyui");
        assert_eq!(
            metadata.workflow.expect("workflow")["3"]["class_type"],
            "KSampler"
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn returns_none_for_plain_png() {
        let directory = temp_dir("metadata-none");
        let tools = tools();
        let path = directory.join("plain.png");
        make_image(&tools, &path, "32x32");
        assert!(extract(&path).is_none());
        let _ = std::fs::remove_dir_all(directory);
    }
}
