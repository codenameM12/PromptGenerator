//! Resolves the bundled ffmpeg/ffprobe sidecars and drives them with
//! `std::process::Command` (native replacement for the Tauri shell plugin).
//! Kept dependency-free from `src-tauri`; the real `media/ffmpeg.rs` resolver is
//! the reference this mirrors.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::backend::BackendError;

#[derive(Debug, Clone)]
pub struct MediaTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl MediaTools {
    pub fn resolve() -> Result<Self, BackendError> {
        let mut directories = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                directories.push(parent.to_path_buf());
                if let Some(grandparent) = parent.parent() {
                    directories.push(grandparent.to_path_buf());
                }
            }
        }
        // The sidecars currently live in the sibling src-tauri crate.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        directories.push(manifest.join("binaries"));
        if let Some(repo) = manifest.parent() {
            directories.push(repo.join("src-tauri").join("binaries"));
        }

        Ok(Self {
            ffmpeg: find_binary(&directories, "ffmpeg")?,
            ffprobe: find_binary(&directories, "ffprobe")?,
        })
    }

    pub fn ffmpeg_command(&self) -> Command {
        Command::new(&self.ffmpeg)
    }

    pub fn ffprobe_json(&self, path: &Path) -> Result<serde_json::Value, BackendError> {
        let output = Command::new(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_streams",
                "-show_format",
            ])
            .arg(path)
            .output()
            .map_err(io_error)?;
        if !output.status.success() {
            return Err(BackendError::validation(format!(
                "ffprobe failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| {
            BackendError::validation(format!(
                "Could not parse ffprobe output for {}: {error}",
                path.display()
            ))
        })
    }

    /// Produces a single scaled thumbnail for `source` at `output`.
    pub fn make_thumbnail(
        &self,
        source: &Path,
        output: &Path,
        size: u32,
    ) -> Result<(), BackendError> {
        let output = Command::new(&self.ffmpeg)
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(source)
            .args(["-vf", &format!("scale={size}:-1"), "-frames:v", "1"])
            .arg(output)
            .output()
            .map_err(io_error)?;
        if !output.status.success() {
            return Err(BackendError::validation(format!(
                "Thumbnail failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }
}

fn io_error(error: std::io::Error) -> BackendError {
    BackendError::not_found(format!("Could not run media tool: {error}"))
}

fn find_binary(directories: &[PathBuf], name: &str) -> Result<PathBuf, BackendError> {
    let candidates = [
        format!("{name}.exe"),
        format!("{name}-x86_64-pc-windows-msvc.exe"),
        name.to_string(),
    ];
    for directory in directories {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(BackendError::not_found(format!(
        "Could not find the bundled {name} binary. Reinstall Prompt Studio or check src-tauri/binaries."
    )))
}

#[cfg(test)]
mod tests {
    use super::MediaTools;

    #[test]
    fn resolves_bundled_sidecars() {
        let tools = MediaTools::resolve().expect("resolve bundled tools");
        assert!(tools.ffmpeg.is_file(), "{}", tools.ffmpeg.display());
        assert!(tools.ffprobe.is_file(), "{}", tools.ffprobe.display());
        let version = tools
            .ffmpeg_command()
            .arg("-version")
            .output()
            .expect("run ffmpeg");
        assert!(version.status.success());
    }

    #[test]
    fn probes_and_thumbnails_a_generated_fixture() {
        let tools = match MediaTools::resolve() {
            Ok(tools) => tools,
            Err(_) => return, // sidecars not present in this environment
        };
        let dir = tempfile::tempdir().expect("temp dir");
        let source = dir.path().join("fixture.png");
        let generated = tools
            .ffmpeg_command()
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=32x32:d=1",
                "-frames:v",
                "1",
            ])
            .arg(&source)
            .output()
            .expect("generate fixture");
        if !generated.status.success() {
            return; // lavfi unavailable; integration skipped
        }
        assert!(source.is_file());

        let probe = tools.ffprobe_json(&source).expect("probe");
        assert!(probe.get("streams").is_some(), "probe has streams");

        let thumb = dir.path().join("thumb.png");
        tools
            .make_thumbnail(&source, &thumb, 8)
            .expect("make thumbnail");
        assert!(thumb.is_file());
        assert!(std::fs::metadata(&thumb).expect("metadata").len() > 0);
    }
}
