use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager};

/// Tracks which source folders the renderer is allowed to read through `media://`.
pub struct MediaAccess {
    cache_dir: PathBuf,
    roots: Mutex<Vec<PathBuf>>,
}

impl MediaAccess {
    pub fn new(cache_dir: PathBuf) -> Self {
        // The data directory may be reached through a junction (portable builds
        // symlink `data`), while the database stores the resolved path. Compare
        // canonical paths so cache thumbnails still pass the allow check.
        let cache_dir = dunce::canonicalize(&cache_dir).unwrap_or(cache_dir);
        Self {
            cache_dir,
            roots: Mutex::new(Vec::new()),
        }
    }

    pub fn allow_path(&self, path: &Path) {
        let Some(parent) = path.parent() else {
            return;
        };
        let Ok(parent) = dunce::canonicalize(parent) else {
            return;
        };
        let mut roots = match self.roots.lock() {
            Ok(roots) => roots,
            Err(_) => return,
        };
        if !roots.iter().any(|root| root == &parent) {
            roots.push(parent);
        }
    }

    pub fn is_allowed(&self, path: &Path) -> bool {
        let Ok(canonical) = dunce::canonicalize(path) else {
            return false;
        };
        if canonical.starts_with(&self.cache_dir) {
            return true;
        }
        match self.roots.lock() {
            Ok(roots) => roots.iter().any(|root| canonical.starts_with(root)),
            Err(_) => false,
        }
    }
}

pub fn handle(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Some(access) = app.try_state::<MediaAccess>() else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "media access not ready");
    };

    let path = match decode_path(request.uri().path()) {
        Some(path) => path,
        None => return error_response(StatusCode::BAD_REQUEST, "invalid media path"),
    };

    if !access.is_allowed(&path) {
        return error_response(StatusCode::FORBIDDEN, "media path is not approved");
    }

    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());

    serve(&path, range)
}

fn decode_path(raw: &str) -> Option<PathBuf> {
    let decoded = percent_decode_str(raw.trim_start_matches('/'))
        .decode_utf8()
        .ok()?
        .into_owned();
    let mut value = decoded;

    // convertFileSrc produces /C:/... on Windows; strip the leading slash.
    if value.starts_with('/') && value.len() > 2 && value.as_bytes()[2] == b':' {
        value.remove(0);
    }

    Some(PathBuf::from(value))
}

pub fn serve(path: &Path, range: Option<&str>) -> Response<Vec<u8>> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return error_response(StatusCode::NOT_FOUND, "media file not found"),
    };
    let total = metadata.len();

    let parsed = match range {
        Some(value) => match parse_range(value, total) {
            Ok(parsed) => Some(parsed),
            Err(()) => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                    .body(Vec::new())
                    .unwrap_or_else(|_| Response::new(Vec::new()));
            }
        },
        None => None,
    };

    let (start, end) = parsed.unwrap_or((0, total.saturating_sub(1)));
    let length = if total == 0 { 0 } else { end - start + 1 };

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "cannot open media"),
    };
    if file.seek(SeekFrom::Start(start)).is_err() {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "cannot seek media");
    }

    let mut bytes = vec![0_u8; length as usize];
    if file.read_exact(&mut bytes).is_err() {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "cannot read media");
    }

    let mut builder = Response::builder()
        .status(if parsed.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, content_type(path))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, length.to_string());

    if parsed.is_some() {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        );
    }

    builder.body(bytes).unwrap_or_else(|_| {
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "cannot build response")
    })
}

fn parse_range(value: &str, total: u64) -> Result<(u64, u64), ()> {
    let spec = value.strip_prefix("bytes=").ok_or(())?;
    if spec.contains(',') {
        return Err(());
    }
    let (start, end) = spec.split_once('-').ok_or(())?;

    if start.is_empty() {
        let suffix: u64 = end.parse().map_err(|_| ())?;
        if suffix == 0 || total == 0 {
            return Err(());
        }
        let length = suffix.min(total);
        return Ok((total - length, total - 1));
    }

    let start: u64 = start.parse().map_err(|_| ())?;
    if start >= total {
        return Err(());
    }
    let end: u64 = if end.is_empty() {
        total - 1
    } else {
        end.parse::<u64>().map_err(|_| ())?.min(total - 1)
    };
    if end < start {
        return Err(());
    }
    Ok((start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        _ => "application/octet-stream",
    }
}

fn error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::{MediaAccess, serve};
    use std::path::PathBuf;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-protocol-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&path).expect("create dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn serves_full_and_partial_ranges() {
        let directory = TempDir::new("serve");
        let file = directory.path.join("clip.mp4");
        std::fs::write(&file, (0_u8..10).collect::<Vec<_>>()).expect("write");

        let full = serve(&file, None);
        assert_eq!(full.status(), 200);
        assert_eq!(full.body(), &(0_u8..10).collect::<Vec<_>>());
        assert_eq!(full.headers()["accept-ranges"], "bytes");

        let partial = serve(&file, Some("bytes=2-5"));
        assert_eq!(partial.status(), 206);
        assert_eq!(partial.body(), &[2_u8, 3, 4, 5]);
        assert_eq!(partial.headers()["content-range"], "bytes 2-5/10");

        let suffix = serve(&file, Some("bytes=-3"));
        assert_eq!(suffix.status(), 206);
        assert_eq!(suffix.body(), &[7_u8, 8, 9]);

        let invalid = serve(&file, Some("bytes=20-30"));
        assert_eq!(invalid.status(), 416);
        assert_eq!(invalid.headers()["content-range"], "bytes */10");
    }

    #[test]
    fn media_access_limits_roots() {
        let cache = TempDir::new("cache");
        let media = TempDir::new("media");
        let file = media.path.join("a.png");
        std::fs::write(&file, b"x").expect("write");

        let access = MediaAccess::new(cache.path.clone());
        assert!(!access.is_allowed(&file));
        access.allow_path(&file);
        assert!(access.is_allowed(&file));

        let cache_file = cache.path.join("thumb.jpg");
        std::fs::write(&cache_file, b"x").expect("write");
        assert!(access.is_allowed(&cache_file));
    }

    #[cfg(windows)]
    #[test]
    fn cache_files_are_allowed_through_a_junction() {
        let temp = TempDir::new("cache-junction");
        let real = temp.path.join("real");
        std::fs::create_dir_all(real.join("thumbnails")).expect("real dirs");
        let alias = temp.path.join("alias");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&alias)
            .arg(&real)
            .status();
        if !matches!(status, Ok(status) if status.success()) {
            // Junctions are unavailable in this environment; skip.
            return;
        }

        let file = real.join("thumbnails").join("t.jpg");
        std::fs::write(&file, b"x").expect("write");

        // Mirrors the portable launcher: data is reached through a junction,
        // but the database stores the resolved path.
        let access = MediaAccess::new(alias.join("thumbnails"));
        assert!(access.is_allowed(&file));
    }
}
