use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::CommandError;

/// Wrapper around the `lms` CLI shipped with LM Studio. It is used for partial
/// GPU offload and as a lifecycle fallback when the REST API is unavailable.
#[derive(Debug, Clone)]
pub struct LmsCli {
    path: PathBuf,
}

impl LmsCli {
    pub fn detect() -> Option<Self> {
        if let Ok(explicit) = std::env::var("LMS_PATH") {
            let path = PathBuf::from(explicit);
            if path.is_file() {
                return Some(Self { path });
            }
        }

        let mut directories = Vec::new();
        if let Some(path) = std::env::var_os("PATH") {
            directories.extend(std::env::split_paths(&path));
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            directories.push(
                PathBuf::from(home)
                    .join(".cache")
                    .join("lm-studio")
                    .join("bin"),
            );
        }

        Self::detect_in(&directories)
    }

    pub fn detect_in(directories: &[PathBuf]) -> Option<Self> {
        for directory in directories {
            for name in ["lms.exe", "lms.cmd", "lms.bat", "lms"] {
                let path = directory.join(name);
                if path.is_file() {
                    return Some(Self { path });
                }
            }
        }
        None
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(
        &self,
        model: &str,
        gpu_offload: Option<f32>,
        context_length: Option<u32>,
        ttl_seconds: Option<u64>,
    ) -> Result<(), CommandError> {
        let mut args = vec!["load".to_string(), model.to_string()];
        if let Some(ratio) = gpu_offload {
            args.push("--gpu".into());
            args.push(format!("{ratio}"));
        }
        if let Some(context) = context_length {
            args.push("--context-length".into());
            args.push(context.to_string());
        }
        if let Some(ttl) = ttl_seconds {
            args.push("--ttl".into());
            args.push(ttl.to_string());
        }
        self.run(&args).map(|_| ())
    }

    pub fn unload(&self, model: &str) -> Result<(), CommandError> {
        self.run(&["unload".into(), model.into()]).map(|_| ())
    }

    pub fn unload_all(&self) -> Result<(), CommandError> {
        self.run(&["unload".into(), "--all".into()]).map(|_| ())
    }

    fn run(&self, args: &[String]) -> Result<String, CommandError> {
        let mut command = if matches!(
            self.path
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("cmd") | Some("bat")
        ) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(&self.path);
            command
        } else {
            Command::new(&self.path)
        };

        crate::media::ffmpeg::hide_console(&mut command);
        let output = command
            .args(args)
            .output()
            .map_err(|error| CommandError::from_io(&error))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CommandError::runtime(format!(
                "lms {} failed: {}",
                args.first().map(String::as_str).unwrap_or("command"),
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::LmsCli;
    use crate::media::test_support::temp_dir;

    #[test]
    fn detects_a_binary_in_a_directory() {
        let directory = temp_dir("lms-detect");
        let path = directory.join("lms.exe");
        fs::write(&path, b"stub").expect("write");

        let detected = LmsCli::detect_in(&[directory.clone()]).expect("detected");
        assert_eq!(detected.path(), path);

        let empty = temp_dir("lms-empty");
        assert!(LmsCli::detect_in(&[empty.clone()]).is_none());

        let _ = fs::remove_dir_all(directory);
        let _ = fs::remove_dir_all(empty);
    }

    #[test]
    fn builds_load_arguments() {
        let directory = temp_dir("lms-args");
        let script = directory.join("lms.cmd");
        fs::write(&script, "@echo off\r\necho %* > \"%~dp0args.txt\"\r\n").expect("write script");

        let cli = LmsCli { path: script };
        cli.load("lfm3-vl-2b", Some(0.5), Some(4096), Some(60))
            .expect("load");

        let args = fs::read_to_string(directory.join("args.txt"))
            .expect("read args")
            .trim()
            .to_string();
        assert!(args.starts_with("load lfm3-vl-2b"));
        assert!(args.contains("--gpu 0.5"));
        assert!(args.contains("--context-length 4096"));
        assert!(args.contains("--ttl 60"));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn reports_cli_failures() {
        let directory = temp_dir("lms-fail");
        let script = directory.join("lms.cmd");
        fs::write(&script, "@echo off\r\nexit /b 1\r\n").expect("write script");

        let cli = LmsCli { path: script };
        let error = cli.unload_all().expect_err("failed cli");
        assert_eq!(error.category, crate::commands::ErrorCategory::Runtime);

        let _ = fs::remove_dir_all(directory);
    }
}
