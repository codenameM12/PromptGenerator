use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::commands::CommandError;
use crate::media::import;

pub const PREVIEW_ROWS: usize = 25;
pub const MAX_IMPORT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetFormat {
    Text,
    Json,
    Jsonl,
    Csv,
    Folder,
}

impl DatasetFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Csv => "csv",
            Self::Folder => "folder",
        }
    }

    fn infer(path: &Path) -> Self {
        if path.is_dir() {
            return Self::Folder;
        }
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("json") => Self::Json,
            Some("jsonl") | Some("ndjson") => Self::Jsonl,
            Some("csv") => Self::Csv,
            Some("tsv") => Self::Csv,
            _ => Self::Text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub format: DatasetFormat,
    pub encoding: String,
    pub delimiter: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DatasetMapping {
    /// Source field name -> one of prompt, negative, params, mediaPath, tags,
    /// identifier, ignore.
    pub fields: BTreeMap<String, String>,
}

impl DatasetMapping {
    pub fn source_for(&self, target: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(_, value)| value.as_str() == target)
            .map(|(source, _)| source.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowIssue {
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappedRow {
    pub row: usize,
    pub fields: BTreeMap<String, String>,
    pub prompt: String,
    pub negative: Option<String>,
    pub params: Value,
    pub media_path: Option<String>,
    pub media_missing: bool,
    pub tags: Vec<String>,
    pub identifier: Option<String>,
}

/// One media reference declared by a dataset row.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegment {
    pub path: String,
    pub weight: f64,
}

/// Splits a row's media field into ordered references. Segments are separated by
/// `|`; a trailing `@<number>` sets the weight. Non-numeric suffixes stay part of
/// the path, empty segments are ignored, and weights clamp to 0.0-2.0.
pub fn parse_media_segments(raw: &str) -> Vec<MediaSegment> {
    raw.split('|')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }
            let (path, weight) = match segment.rsplit_once('@') {
                Some((path, suffix)) => match suffix.trim().parse::<f64>() {
                    Ok(_) if path.trim().is_empty() => (segment, 1.0),
                    Ok(weight) => (path.trim(), weight),
                    Err(_) => (segment, 1.0),
                },
                None => (segment, 1.0),
            };
            let path = path.trim();
            if path.is_empty() {
                return None;
            }
            let weight = if weight.is_finite() {
                weight.clamp(0.0, 2.0)
            } else {
                1.0
            };
            Some(MediaSegment {
                path: path.to_string(),
                weight,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub total: usize,
    pub valid: usize,
    pub issues: Vec<RowIssue>,
    pub media_missing: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupeMode {
    Skip,
    Merge,
    Keep,
}

impl DedupeMode {
    pub fn parse(value: &str) -> Result<Self, CommandError> {
        match value {
            "skip" => Ok(Self::Skip),
            "merge" => Ok(Self::Merge),
            "keep" => Ok(Self::Keep),
            other => Err(CommandError::validation(format!(
                "Unknown dedupe mode: {other}"
            ))),
        }
    }
}

pub fn inspect(path: &Path) -> Result<SourceInfo, CommandError> {
    let format = DatasetFormat::infer(path);
    if format == DatasetFormat::Folder {
        return Ok(SourceInfo {
            format,
            encoding: "utf-8".into(),
            delimiter: b',',
        });
    }
    let metadata = std::fs::metadata(path).map_err(|error| CommandError::from_io(&error))?;
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(CommandError::validation(format!(
            "Dataset is larger than {} MB; split the file and import it in parts",
            MAX_IMPORT_BYTES / (1024 * 1024)
        )));
    }
    let (encoding, sample) = detect_encoding(path)?;
    let delimiter = detect_delimiter(&sample, path);
    Ok(SourceInfo {
        format,
        encoding,
        delimiter,
    })
}

fn detect_encoding(path: &Path) -> Result<(String, String), CommandError> {
    let mut file = File::open(path).map_err(|error| CommandError::from_io(&error))?;
    let mut sample = vec![0u8; 64 * 1024];
    let read = file
        .read(&mut sample)
        .map_err(|error| CommandError::from_io(&error))?;
    sample.truncate(read);
    let boundary = sample.len().saturating_sub(3);
    let utf8 = std::str::from_utf8(&sample[..boundary.max(1)])
        .map(|_| String::new())
        .or_else(|_| std::str::from_utf8(&sample).map(|_| String::new()));
    match utf8 {
        Ok(_) => Ok((
            "utf-8".into(),
            String::from_utf8_lossy(&sample).into_owned(),
        )),
        Err(_) => {
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&sample);
            Ok(("windows-1252".into(), decoded.into_owned()))
        }
    }
}

fn detect_delimiter(sample: &str, path: &Path) -> u8 {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"))
    {
        return b'\t';
    }
    let first = sample.lines().next().unwrap_or_default();
    let tabs = first.matches('\t').count();
    let commas = first.matches(',').count();
    let semicolons = first.matches(';').count();
    if tabs > commas && tabs >= semicolons {
        b'\t'
    } else if semicolons > commas {
        b';'
    } else {
        b','
    }
}

pub fn headers(path: &Path) -> Result<Vec<String>, CommandError> {
    let info = inspect(path)?;
    match info.format {
        DatasetFormat::Folder => Ok(vec!["image".into(), "caption".into()]),
        DatasetFormat::Csv => {
            let mut reader = csv_reader(path, info.delimiter)?;
            let headers = reader.headers().map_err(|error| {
                CommandError::validation(format!("Invalid CSV header: {error}"))
            })?;
            Ok(headers
                .iter()
                .map(|header| header.trim().to_string())
                .collect())
        }
        DatasetFormat::Json | DatasetFormat::Jsonl => {
            let mut keys = Vec::new();
            let mut seen = HashSet::new();
            stream_source(path, info.format, info.delimiter, Some(5), |fields, _| {
                for key in fields.keys() {
                    if seen.insert(key.clone()) {
                        keys.push(key.clone());
                    }
                }
                Ok(())
            })?;
            Ok(keys)
        }
        DatasetFormat::Text => Ok(vec!["text".into()]),
    }
}

pub fn auto_mapping(headers: &[String]) -> DatasetMapping {
    let mut fields = BTreeMap::new();
    for (index, header) in headers.iter().enumerate() {
        let lower = header.to_ascii_lowercase();
        let target = if lower.contains("negative") {
            "negative"
        } else if lower.contains("param") || lower == "settings" {
            "params"
        } else if lower.contains("media")
            || lower.contains("image")
            || lower.contains("video")
            || lower == "path"
            || lower == "file"
        {
            "mediaPath"
        } else if lower.contains("tag") {
            "tags"
        } else if lower == "id"
            || lower.contains("identifier")
            || lower.contains("slug")
            || lower.contains("name")
        {
            "identifier"
        } else if lower.contains("prompt")
            || lower.contains("caption")
            || lower == "text"
            || lower.contains("instruction")
            || lower.contains("description")
            || index == 0
        {
            "prompt"
        } else {
            "ignore"
        };
        fields.insert(header.clone(), target.into());
    }
    DatasetMapping { fields }
}

/// Streams source rows, mapping and validating each one. Rows with a missing
/// prompt or an unsafe media path are reported as issues and not yielded.
pub fn stream<F>(
    path: &Path,
    mapping: &DatasetMapping,
    limit: Option<usize>,
    mut on_row: F,
) -> Result<ScanSummary, CommandError>
where
    F: FnMut(MappedRow) -> Result<(), CommandError>,
{
    let info = inspect(path)?;
    let base = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    let mut summary = ScanSummary {
        total: 0,
        valid: 0,
        issues: Vec::new(),
        media_missing: 0,
        truncated: false,
    };

    if info.format == DatasetFormat::Folder {
        stream_folder(path, mapping, &base, limit, &mut summary, &mut on_row)?;
        return Ok(summary);
    }

    let limit = limit.unwrap_or(usize::MAX);
    stream_source(path, info.format, info.delimiter, None, |fields, row| {
        summary.total += 1;
        match map_row(row, fields, mapping, &base) {
            Ok(mapped) => {
                if mapped.media_missing {
                    summary.media_missing += 1;
                }
                if summary.valid < limit {
                    on_row(mapped)?;
                    summary.valid += 1;
                } else {
                    summary.truncated = true;
                }
            }
            Err(issue) => summary.issues.push(issue),
        }
        Ok(())
    })?;

    Ok(summary)
}

fn stream_folder<F>(
    path: &Path,
    mapping: &DatasetMapping,
    base: &Path,
    limit: Option<usize>,
    summary: &mut ScanSummary,
    on_row: &mut F,
) -> Result<(), CommandError>
where
    F: FnMut(MappedRow) -> Result<(), CommandError>,
{
    let limit = limit.unwrap_or(usize::MAX);
    let mut files: Vec<PathBuf> = Vec::new();
    collect_images(path, &mut files)?;
    files.sort();

    for image in files {
        summary.total += 1;
        let caption_path = image.with_extension("txt");
        let caption = std::fs::read_to_string(&caption_path).unwrap_or_default();
        let mut fields = BTreeMap::new();
        fields.insert("image".into(), image.display().to_string());
        fields.insert("caption".into(), caption.trim().to_string());

        let mapped = map_row(summary.total, fields, mapping, base);
        match mapped {
            Ok(row) => {
                if summary.valid < limit {
                    on_row(row)?;
                    summary.valid += 1;
                } else {
                    summary.truncated = true;
                }
            }
            Err(issue) => summary.issues.push(RowIssue {
                message: if caption.trim().is_empty() {
                    format!("missing caption for {}", image.display())
                } else {
                    issue.message
                },
                ..issue
            }),
        }
    }
    Ok(())
}

fn collect_images(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), CommandError> {
    for entry in std::fs::read_dir(directory).map_err(|error| CommandError::from_io(&error))? {
        let entry = entry.map_err(|error| CommandError::from_io(&error))?;
        let path = entry.path();
        if path.is_dir() {
            collect_images(&path, files)?;
        } else if import::is_supported(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn map_row(
    row: usize,
    fields: BTreeMap<String, String>,
    mapping: &DatasetMapping,
    base: &Path,
) -> Result<MappedRow, RowIssue> {
    let value = |target: &str| {
        mapping
            .source_for(target)
            .and_then(|source| fields.get(source))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    let prompt = value("prompt").unwrap_or_default();
    if prompt.is_empty() {
        return Err(RowIssue {
            row,
            message: "row has no prompt after mapping".into(),
        });
    }

    let negative = value("negative");
    let params = parse_params(value("params"));
    let tags = parse_tags(value("tags"));
    let identifier = value("identifier");

    let (media_path, media_missing) = match value("mediaPath") {
        Some(raw) => match resolve_media(base, &raw) {
            Ok((resolved, missing)) => (Some(resolved), missing),
            Err(message) => {
                return Err(RowIssue { row, message });
            }
        },
        None => (None, false),
    };

    Ok(MappedRow {
        row,
        fields,
        prompt,
        negative,
        params,
        media_path,
        media_missing,
        tags,
        identifier,
    })
}

pub fn resolve_media(base: &Path, raw: &str) -> Result<(String, bool), String> {
    let raw_path = PathBuf::from(raw.replace('\\', "/"));
    let candidate = if raw_path.is_absolute() {
        raw_path.clone()
    } else {
        base.join(&raw_path)
    };

    let normalized = normalize_path(&candidate);
    let base_normalized = normalize_path(base);
    if !normalized.starts_with(&base_normalized) {
        return Err(format!("media path escapes the dataset root: {raw}"));
    }

    if candidate.is_file() {
        Ok((candidate.display().to_string(), false))
    } else {
        Ok((candidate.display().to_string(), true))
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_os_string()),
        }
    }
    parts.into_iter().collect()
}

fn parse_params(value: Option<String>) -> Value {
    let Some(text) = value else {
        return Value::Object(Map::new());
    };
    if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&text) {
        return Value::Object(object);
    }
    let mut object = Map::new();
    for pair in text
        .split(',')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
    {
        let (key, value) = pair.split_once('=').unwrap_or((pair, "true"));
        let value = value.trim();
        let parsed = if let Ok(number) = value.parse::<i64>() {
            Value::Number(number.into())
        } else if let Ok(number) = value.parse::<f64>() {
            serde_json::Number::from_f64(number)
                .map(Value::Number)
                .unwrap_or_else(|| Value::String(value.into()))
        } else {
            Value::String(value.into())
        };
        object.insert(key.trim().to_string(), parsed);
    }
    Value::Object(object)
}

fn parse_tags(value: Option<String>) -> Vec<String> {
    let Some(text) = value else {
        return Vec::new();
    };
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&text) {
        return items
            .into_iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect();
    }
    text.split([',', ';', '|'])
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn norm_prompt(prompt: &str) -> String {
    prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Normalized key used for duplicate detection.
pub fn dedupe_key(row: &MappedRow) -> String {
    if let Some(identifier) = &row.identifier {
        format!("id:{}", identifier.trim().to_ascii_lowercase())
    } else {
        format!("prompt:{}", norm_prompt(&row.prompt))
    }
}

fn stream_source<F>(
    path: &Path,
    format: DatasetFormat,
    delimiter: u8,
    limit: Option<usize>,
    mut on_fields: F,
) -> Result<(), CommandError>
where
    F: FnMut(BTreeMap<String, String>, usize) -> Result<(), CommandError>,
{
    let limit = limit.unwrap_or(usize::MAX);
    let mut row = 0usize;

    match format {
        DatasetFormat::Text => {
            let reader = text_reader(path)?;
            for line in reader.lines() {
                let line = line.map_err(|error| CommandError::from_io(&error))?;
                if line.trim().is_empty() {
                    continue;
                }
                row += 1;
                let mut fields = BTreeMap::new();
                fields.insert("text".into(), line);
                on_fields(fields, row)?;
                if row >= limit {
                    break;
                }
            }
        }
        DatasetFormat::Jsonl => {
            let reader = text_reader(path)?;
            for line in reader.lines() {
                let line = line.map_err(|error| CommandError::from_io(&error))?;
                if line.trim().is_empty() {
                    continue;
                }
                row += 1;
                let value: Value = serde_json::from_str(&line).map_err(|error| {
                    CommandError::validation(format!("Invalid JSONL row {row}: {error}"))
                })?;
                on_fields(json_fields(value)?, row)?;
                if row >= limit {
                    break;
                }
            }
        }
        DatasetFormat::Json => {
            let (text, _) = read_all_text(path)?;
            let value: Value = serde_json::from_str(&text).map_err(|error| {
                CommandError::validation(format!("Invalid JSON dataset: {error}"))
            })?;
            let items = value.as_array().ok_or_else(|| {
                CommandError::validation("A JSON dataset must be an array of rows")
            })?;
            for item in items {
                row += 1;
                on_fields(json_fields(item.clone())?, row)?;
                if row >= limit {
                    break;
                }
            }
        }
        DatasetFormat::Csv => {
            let mut reader = csv_reader(path, delimiter)?;
            let headers = reader
                .headers()
                .map_err(|error| CommandError::validation(format!("Invalid CSV header: {error}")))?
                .iter()
                .map(|header| header.trim().to_string())
                .collect::<Vec<_>>();
            for record in reader.records() {
                let record = record.map_err(|error| {
                    CommandError::validation(format!("Invalid CSV row: {error}"))
                })?;
                row += 1;
                let mut fields = BTreeMap::new();
                for (index, header) in headers.iter().enumerate() {
                    fields.insert(header.clone(), record.get(index).unwrap_or("").to_string());
                }
                on_fields(fields, row)?;
                if row >= limit {
                    break;
                }
            }
        }
        DatasetFormat::Folder => unreachable!("folders do not use stream_source"),
    }
    Ok(())
}

fn json_fields(value: Value) -> Result<BTreeMap<String, String>, CommandError> {
    let object = value
        .as_object()
        .ok_or_else(|| CommandError::validation("Dataset rows must be objects"))?;
    let mut fields = BTreeMap::new();
    for (key, value) in object {
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        fields.insert(key.clone(), text);
    }
    Ok(fields)
}

fn text_reader(path: &Path) -> Result<Box<dyn BufRead>, CommandError> {
    let (encoding, _) = detect_encoding(path)?;
    if encoding == "utf-8" {
        let file = File::open(path).map_err(|error| CommandError::from_io(&error))?;
        Ok(Box::new(BufReader::new(file)))
    } else {
        let (text, _) = read_all_text(path)?;
        Ok(Box::new(BufReader::new(Cursor::new(text.into_bytes()))))
    }
}

fn read_all_text(path: &Path) -> Result<(String, String), CommandError> {
    let bytes = std::fs::read(path).map_err(|error| CommandError::from_io(&error))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok((
            text.trim_start_matches('\u{feff}').to_string(),
            "utf-8".into(),
        )),
        Err(error) => {
            let bytes = error.into_bytes();
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
            Ok((decoded.into_owned(), "windows-1252".into()))
        }
    }
}

fn csv_reader(path: &Path, delimiter: u8) -> Result<csv::Reader<Box<dyn Read>>, CommandError> {
    let (encoding, _) = detect_encoding(path)?;
    let reader: Box<dyn Read> = if encoding == "utf-8" {
        Box::new(BufReader::new(
            File::open(path).map_err(|error| CommandError::from_io(&error))?,
        ))
    } else {
        let (text, _) = read_all_text(path)?;
        Box::new(Cursor::new(text.into_bytes()))
    };
    Ok(csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(reader))
}

fn project(row: &BTreeMap<String, String>, columns: &[String]) -> BTreeMap<String, String> {
    columns
        .iter()
        .map(|column| (column.clone(), row.get(column).cloned().unwrap_or_default()))
        .collect()
}

/// Writes a table with a stable column order. Rows are written incrementally so
/// exporting a large dataset does not build the whole document in memory.
pub fn write_rows(
    path: &Path,
    format: &str,
    columns: &[String],
    rows: impl Iterator<Item = BTreeMap<String, String>>,
) -> Result<(usize, u64), CommandError> {
    if columns.is_empty() {
        return Err(CommandError::validation(
            "Choose at least one export column",
        ));
    }
    let file = File::create(path).map_err(|error| CommandError::from_io(&error))?;
    let mut writer = BufWriter::new(file);
    let mut count = 0usize;

    match format {
        "csv" => {
            let mut csv = csv::WriterBuilder::new().from_writer(&mut writer);
            csv.write_record(columns)
                .map_err(|error| CommandError::io(format!("write CSV header: {error}")))?;
            for row in rows {
                let record = columns
                    .iter()
                    .map(|column| row.get(column).cloned().unwrap_or_default())
                    .collect::<Vec<_>>();
                csv.write_record(record)
                    .map_err(|error| CommandError::io(format!("write CSV row: {error}")))?;
                count += 1;
            }
            csv.flush()
                .map_err(|error| CommandError::io(format!("flush CSV: {error}")))?;
        }
        "json" => {
            writer
                .write_all(b"[")
                .map_err(|error| CommandError::from_io(&error))?;
            for row in rows {
                if count > 0 {
                    writer
                        .write_all(b",")
                        .map_err(|error| CommandError::from_io(&error))?;
                }
                let projected = project(&row, columns);
                let value = serde_json::to_string(&projected)
                    .map_err(|error| CommandError::internal(format!("serialize row: {error}")))?;
                writer
                    .write_all(value.as_bytes())
                    .map_err(|error| CommandError::from_io(&error))?;
                count += 1;
            }
            writer
                .write_all(b"]")
                .map_err(|error| CommandError::from_io(&error))?;
        }
        "jsonl" | "ndjson" => {
            for row in rows {
                let projected = project(&row, columns);
                let value = serde_json::to_string(&projected)
                    .map_err(|error| CommandError::internal(format!("serialize row: {error}")))?;
                writer
                    .write_all(value.as_bytes())
                    .map_err(|error| CommandError::from_io(&error))?;
                writer
                    .write_all(b"\n")
                    .map_err(|error| CommandError::from_io(&error))?;
                count += 1;
            }
        }
        other => {
            return Err(CommandError::validation(format!(
                "Unsupported dataset export format: {other}"
            )));
        }
    }

    writer
        .flush()
        .map_err(|error| CommandError::from_io(&error))?;
    let bytes = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    Ok((count, bytes))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::{
        DatasetFormat, DedupeMode, auto_mapping, dedupe_key, headers, inspect, stream, write_rows,
    };
    use crate::media::test_support::temp_dir;

    fn write_file(name: &str, contents: &[u8]) -> std::path::PathBuf {
        let directory = temp_dir("dataset");
        let path = directory.join(name);
        fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn maps_text_one_prompt_per_line_and_skips_blanks() {
        let path = write_file("prompts.txt", b"first prompt\n\n   \nsecond prompt\n");
        assert_eq!(inspect(&path).expect("inspect").format, DatasetFormat::Text);
        assert_eq!(headers(&path).expect("headers"), vec!["text"]);

        let mapping = auto_mapping(&["text".into()]);
        let mut rows = Vec::new();
        let summary = stream(&path, &mapping, None, |row| {
            rows.push(row.prompt);
            Ok(())
        })
        .expect("stream");
        assert_eq!(summary.total, 2);
        assert_eq!(rows, vec!["first prompt", "second prompt"]);
    }

    #[test]
    fn detects_csv_delimiter_and_auto_maps_caption() {
        let comma = write_file(
            "prompts.csv",
            b"caption,negative,tags\nfirst prompt,blurry,neon;city\nsecond prompt,,snow\n",
        );
        let info = inspect(&comma).expect("inspect");
        assert_eq!(info.format, DatasetFormat::Csv);
        assert_eq!(info.delimiter, b',');
        let mapping = auto_mapping(&headers(&comma).expect("headers"));
        assert_eq!(mapping.source_for("prompt"), Some("caption"));

        let mut rows = Vec::new();
        stream(&comma, &mapping, None, |row| {
            rows.push(row);
            Ok(())
        })
        .expect("stream");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].negative.as_deref(), Some("blurry"));
        assert_eq!(rows[0].tags, vec!["neon", "city"]);

        let tsv = write_file("prompts.tsv", b"prompt\tnegative\none\ttwo\n");
        assert_eq!(inspect(&tsv).expect("inspect").delimiter, b'\t');
    }

    #[test]
    fn parses_ordered_media_segments_and_weights() {
        use super::parse_media_segments;

        let segments = parse_media_segments("subject.png@0.8|style.png");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].path, "subject.png");
        assert!((segments[0].weight - 0.8).abs() < f64::EPSILON);
        assert_eq!(segments[1].path, "style.png");
        assert!((segments[1].weight - 1.0).abs() < f64::EPSILON);

        let clamped = parse_media_segments("a.png@5|b.png@-1|c.png@NaN");
        assert!((clamped[0].weight - 2.0).abs() < f64::EPSILON);
        assert!((clamped[1].weight - 0.0).abs() < f64::EPSILON);
        assert!((clamped[2].weight - 1.0).abs() < f64::EPSILON);

        let at_in_name = parse_media_segments("a@b.png");
        assert_eq!(at_in_name[0].path, "a@b.png");
        assert!((at_in_name[0].weight - 1.0).abs() < f64::EPSILON);

        let at_only = parse_media_segments("@0.8");
        assert_eq!(at_only[0].path, "@0.8");
        assert!((at_only[0].weight - 1.0).abs() < f64::EPSILON);

        assert!(parse_media_segments("  |  ").is_empty());
        assert!(parse_media_segments("").is_empty());
    }

    #[test]
    fn parses_jsonl_relative_media_and_flags_missing() {
        let directory = temp_dir("dataset-jsonl");
        let media = directory.join("frame.png");
        fs::write(&media, b"png").expect("media");
        let path = directory.join("rows.jsonl");
        fs::write(
            &path,
            b"{\"prompt\":\"one\",\"image\":\"frame.png\"}\n{\"prompt\":\"two\",\"image\":\"missing.png\"}\n",
        )
        .expect("jsonl");

        let mapping = auto_mapping(&headers(&path).expect("headers"));
        let mut rows = Vec::new();
        let summary = stream(&path, &mapping, None, |row| {
            rows.push(row);
            Ok(())
        })
        .expect("stream");
        assert_eq!(summary.valid, 2);
        assert_eq!(summary.media_missing, 1);
        assert!(
            rows[0]
                .media_path
                .as_deref()
                .unwrap()
                .ends_with("frame.png")
        );
        assert!(rows[1].media_missing);
    }

    #[test]
    fn rejects_media_paths_outside_the_dataset_root() {
        let directory = temp_dir("dataset-traversal");
        let path = directory.join("rows.jsonl");
        fs::write(
            &path,
            b"{\"prompt\":\"one\",\"image\":\"../../outside.png\"}\n",
        )
        .expect("jsonl");

        let mapping = auto_mapping(&headers(&path).expect("headers"));
        let summary = stream(&path, &mapping, None, |_| Ok(())).expect("stream");
        assert_eq!(summary.valid, 0);
        assert_eq!(summary.issues.len(), 1);
        assert!(summary.issues[0].message.contains("escapes"));
    }

    #[test]
    fn imports_a_folder_with_caption_sidecars() {
        let directory = temp_dir("dataset-folder");
        let image = directory.join("a.png");
        fs::write(&image, b"png").expect("image");
        let caption = directory.join("a.txt");
        fs::write(&caption, b"a caption").expect("caption");
        let incomplete = directory.join("b.png");
        fs::write(&incomplete, b"png").expect("image");

        let mapping = auto_mapping(&headers(&directory).expect("headers"));
        let mut prompts = Vec::new();
        let summary = stream(&directory, &mapping, None, |row| {
            prompts.push(row.prompt);
            Ok(())
        })
        .expect("stream");
        assert_eq!(summary.total, 2);
        assert_eq!(summary.valid, 1);
        assert_eq!(prompts, vec!["a caption"]);
        assert!(summary.issues[0].message.contains("missing caption"));
    }

    #[test]
    fn valid_rows_are_mapped_and_invalid_ones_reported() {
        let path = write_file(
            "rows.csv",
            b"prompt,negative\none,blurry\n,missing prompt\nthree,\n",
        );
        let mapping = auto_mapping(&headers(&path).expect("headers"));
        let summary = stream(&path, &mapping, None, |_| Ok(())).expect("stream");
        assert_eq!(summary.total, 3);
        assert_eq!(summary.valid, 2);
        assert_eq!(summary.issues.len(), 1);
        assert_eq!(summary.issues[0].row, 2);
    }

    #[test]
    fn dedupe_keys_normalize_prompts_and_prefer_identifiers() {
        let path = write_file(
            "dedupe.jsonl",
            b"{\"prompt\":\"A   Red Fox\",\"id\":\"one\"}\n{\"prompt\":\"a red fox\",\"id\":\"two\"}\n",
        );
        let mapping = auto_mapping(&headers(&path).expect("headers"));
        let mut rows = Vec::new();
        stream(&path, &mapping, None, |row| {
            rows.push(row);
            Ok(())
        })
        .expect("stream");
        assert_ne!(dedupe_key(&rows[0]), dedupe_key(&rows[1]));
    }

    #[test]
    fn writes_csv_json_jsonl_with_selected_columns() {
        let directory = temp_dir("dataset-export");
        let rows = || {
            vec![
                std::collections::BTreeMap::from([
                    ("prompt".to_string(), "one".to_string()),
                    ("status".to_string(), "succeeded".to_string()),
                    ("extra".to_string(), "ignored".to_string()),
                ]),
                std::collections::BTreeMap::from([
                    ("prompt".to_string(), "two".to_string()),
                    ("status".to_string(), "failed".to_string()),
                    ("extra".to_string(), "ignored".to_string()),
                ]),
            ]
        };
        let columns = vec!["prompt".to_string(), "status".to_string()];

        let csv_path = directory.join("out.csv");
        let (count, bytes) =
            write_rows(&csv_path, "csv", &columns, rows().into_iter()).expect("csv");
        assert_eq!(count, 2);
        assert!(bytes > 0);
        let text = fs::read_to_string(&csv_path).expect("csv text");
        assert!(text.starts_with("prompt,status\n"));
        assert!(!text.contains("extra"));

        let json_path = directory.join("out.json");
        write_rows(&json_path, "json", &columns, rows().into_iter()).expect("json");
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(json_path).expect("json text")).expect("json");
        assert_eq!(parsed.as_array().expect("array").len(), 2);

        let jsonl_path = directory.join("out.jsonl");
        write_rows(&jsonl_path, "jsonl", &columns, rows().into_iter()).expect("jsonl");
        let lines = fs::read_to_string(jsonl_path).expect("jsonl text");
        assert_eq!(lines.lines().count(), 2);
        let first: serde_json::Value =
            serde_json::from_str(lines.lines().next().expect("line")).expect("row");
        assert_eq!(first["prompt"], json!("one"));
        assert!(first.get("extra").is_none());
    }

    #[test]
    fn formula_like_cells_are_preserved_as_text() {
        let path = write_file("formula.csv", b"prompt\n=SUM(A1:A2)\n");
        let mapping = auto_mapping(&headers(&path).expect("headers"));
        let mut prompts = Vec::new();
        stream(&path, &mapping, None, |row| {
            prompts.push(row.prompt);
            Ok(())
        })
        .expect("stream");
        assert_eq!(prompts, vec!["=SUM(A1:A2)"]);
    }

    #[test]
    fn oversized_files_are_rejected_before_reading() {
        let path = write_file("huge.txt", b"small");
        let info = inspect(&path).expect("small file ok");
        assert_eq!(info.format, DatasetFormat::Text);
        assert_eq!(DedupeMode::parse("skip").expect("skip"), DedupeMode::Skip);
        assert_eq!(
            DedupeMode::parse("merge").expect("merge"),
            DedupeMode::Merge
        );
        assert_eq!(DedupeMode::parse("keep").expect("keep"), DedupeMode::Keep);
        assert!(DedupeMode::parse("other").is_err());
    }
}
