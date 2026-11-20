use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::commands::CommandError;

/// Keeps child processes from opening a console window (they flashed on every
/// import/probe when launched from the GUI app). No effect off Windows.
pub fn hide_console(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Resolved FFmpeg sidecar binaries.
#[derive(Debug, Clone)]
pub struct MediaTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl MediaTools {
    pub fn resolve() -> Result<Self, CommandError> {
        let executable = std::env::current_exe().map_err(|error| CommandError::from_io(&error))?;

        let mut directories = Vec::new();
        if let Some(parent) = executable.parent() {
            directories.push(parent.to_path_buf());
            if let Some(grandparent) = parent.parent() {
                directories.push(grandparent.to_path_buf());
            }
        }
        directories.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries"));

        let ffmpeg = find_binary(&directories, "ffmpeg")?;
        let ffprobe = find_binary(&directories, "ffprobe")?;
        Ok(Self { ffmpeg, ffprobe })
    }

    pub fn ffmpeg_command(&self) -> Command {
        let mut command = Command::new(&self.ffmpeg);
        hide_console(&mut command);
        command
    }

    pub fn ffprobe_json(&self, path: &Path) -> Result<Value, CommandError> {
        let mut probe = Command::new(&self.ffprobe);
        hide_console(&mut probe);
        let output = probe
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
            .map_err(|error| CommandError::from_io(&error))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CommandError::media(format!(
                "ffprobe failed for {}: {}",
                path.display(),
                stderr.trim()
            )));
        }

        serde_json::from_slice(&output.stdout).map_err(|error| {
            CommandError::media(format!(
                "could not parse ffprobe output for {}: {error}",
                path.display()
            ))
        })
    }
}

fn find_binary(directories: &[PathBuf], name: &str) -> Result<PathBuf, CommandError> {
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

    Err(CommandError::not_found(format!(
        "Could not find the bundled {name} binary. Reinstall Prompt Studio or check src-tauri/binaries."
    )))
}

/// Runs a command and turns a non-zero exit into a media error with stderr.
pub fn run_checked(mut command: Command, context: &str) -> Result<(), CommandError> {
    let output = command
        .output()
        .map_err(|error| CommandError::from_io(&error))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(CommandError::media(format!("{context}: {}", stderr.trim())))
}
