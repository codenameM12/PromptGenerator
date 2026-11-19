use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::CommandError;
use crate::commands::models::KeepWarm;
use crate::commands::settings::{Settings, SettingsPatch};

const SETTINGS_FILE: &str = "settings.json";
const TOKEN_FILE: &str = "token.bin";
const CORRUPT_SETTINGS_FILE: &str = "settings.corrupt.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    endpoint: String,
    describer_model: String,
    composer_model: String,
    #[serde(default)]
    describer_system_prompt: Option<String>,
    #[serde(default)]
    composer_system_prompt: Option<String>,
    describer_concurrency: u8,
    ttl_seconds: u64,
    keep_warm: KeepWarm,
    theme: String,
    naming_template: String,
    output_root: Option<String>,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:1234".into(),
            describer_model: "lfm3-vl-2b".into(),
            composer_model: "gemma4-12b-qat-uncensored-hauhaucs-balanced".into(),
            describer_system_prompt: None,
            composer_system_prompt: None,
            describer_concurrency: 2,
            ttl_seconds: 600,
            keep_warm: KeepWarm::None,
            theme: "system".into(),
            naming_template: "{preset}_{asset}_{date}".into(),
            output_root: None,
        }
    }
}

/// Persists settings as JSON and the LM Studio token as a DPAPI blob.
pub struct SettingsStore {
    dir: PathBuf,
    stored: StoredSettings,
    token: Option<String>,
}

impl SettingsStore {
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, CommandError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|error| CommandError::from_io(&error))?;

        let settings_path = dir.join(SETTINGS_FILE);
        let stored = match fs::read_to_string(&settings_path) {
            Ok(text) => match serde_json::from_str::<StoredSettings>(&text) {
                Ok(stored) => match validate(&stored) {
                    Ok(()) => stored,
                    Err(error) => {
                        backup_corrupt(&settings_path, &dir);
                        eprintln!("[settings] ignoring invalid settings: {error}");
                        StoredSettings::default()
                    }
                },
                Err(error) => {
                    backup_corrupt(&settings_path, &dir);
                    eprintln!("[settings] ignoring unreadable settings: {error}");
                    StoredSettings::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoredSettings::default(),
            Err(error) => return Err(CommandError::from_io(&error)),
        };

        let token = match fs::read(dir.join(TOKEN_FILE)) {
            Ok(blob) => {
                match windows_dpapi::decrypt_data(&blob, windows_dpapi::Scope::User, None) {
                    Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(error) => {
                        eprintln!("[settings] could not decrypt stored token: {error}");
                        None
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(CommandError::from_io(&error)),
        };

        Ok(Self { dir, stored, token })
    }

    pub fn get(&self) -> Settings {
        Settings {
            endpoint: self.stored.endpoint.clone(),
            has_token: self.token.is_some(),
            describer_model: self.stored.describer_model.clone(),
            composer_model: self.stored.composer_model.clone(),
            describer_system_prompt: self.stored.describer_system_prompt.clone(),
            composer_system_prompt: self.stored.composer_system_prompt.clone(),
            describer_concurrency: self.stored.describer_concurrency,
            ttl_seconds: self.stored.ttl_seconds,
            keep_warm: self.stored.keep_warm,
            theme: self.stored.theme.clone(),
            naming_template: self.stored.naming_template.clone(),
            output_root: self.stored.output_root.clone(),
        }
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn update(&mut self, patch: SettingsPatch) -> Result<Settings, CommandError> {
        let mut candidate = self.stored.clone();

        if let Some(endpoint) = patch.endpoint {
            candidate.endpoint = endpoint.trim().to_string();
        }
        if let Some(model) = patch.describer_model {
            candidate.describer_model = model.trim().to_string();
        }
        if let Some(model) = patch.composer_model {
            candidate.composer_model = model.trim().to_string();
        }
        if let Some(prompt) = patch.describer_system_prompt {
            let prompt = prompt.trim().to_string();
            candidate.describer_system_prompt = if prompt.is_empty() {
                None
            } else {
                Some(prompt)
            };
        }
        if let Some(prompt) = patch.composer_system_prompt {
            let prompt = prompt.trim().to_string();
            candidate.composer_system_prompt = if prompt.is_empty() {
                None
            } else {
                Some(prompt)
            };
        }
        if let Some(concurrency) = patch.describer_concurrency {
            candidate.describer_concurrency = concurrency;
        }
        if let Some(ttl) = patch.ttl_seconds {
            candidate.ttl_seconds = ttl;
        }
        if let Some(keep_warm) = patch.keep_warm {
            candidate.keep_warm = keep_warm;
        }
        if let Some(theme) = patch.theme {
            candidate.theme = theme;
        }
        if let Some(template) = patch.naming_template {
            candidate.naming_template = template;
        }
        if let Some(root) = patch.output_root {
            candidate.output_root = if root.trim().is_empty() {
                None
            } else {
                Some(root)
            };
        }

        validate(&candidate)?;

        let next_token = match patch.token {
            Some(token) if token.trim().is_empty() => None,
            Some(token) => Some(token),
            None => self.token.clone(),
        };

        write_settings(&self.dir, &candidate)?;
        write_token(&self.dir, next_token.as_deref())?;

        self.stored = candidate;
        self.token = next_token;
        Ok(self.get())
    }
}

fn validate(stored: &StoredSettings) -> Result<(), CommandError> {
    let url = Url::parse(&stored.endpoint)
        .map_err(|_| CommandError::validation("Endpoint must be a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(CommandError::validation("Endpoint must use http or https"));
    }
    if url.host_str().is_none() {
        return Err(CommandError::validation("Endpoint must include a host"));
    }
    if !(1..=4).contains(&stored.describer_concurrency) {
        return Err(CommandError::validation(
            "Description concurrency must be between 1 and 4",
        ));
    }
    if stored.ttl_seconds > 86_400 {
        return Err(CommandError::validation(
            "Model TTL must be 24 hours or less",
        ));
    }
    if !matches!(stored.theme.as_str(), "system" | "light" | "dark") {
        return Err(CommandError::validation(
            "Theme must be system, light, or dark",
        ));
    }
    if stored.describer_model.trim().is_empty() {
        return Err(CommandError::validation("Describer model is required"));
    }
    if stored.composer_model.trim().is_empty() {
        return Err(CommandError::validation("Composer model is required"));
    }
    if stored.naming_template.trim().is_empty() {
        return Err(CommandError::validation("Naming template is required"));
    }
    for (label, prompt) in [
        ("Describer", &stored.describer_system_prompt),
        ("Composer", &stored.composer_system_prompt),
    ] {
        if let Some(prompt) = prompt
            && prompt.chars().count() > 8000
        {
            return Err(CommandError::validation(format!(
                "{label} system prompt must be 8000 characters or fewer"
            )));
        }
    }
    if let Some(root) = &stored.output_root
        && !Path::new(root).is_absolute()
    {
        return Err(CommandError::validation(
            "Alternate output root must be an absolute path",
        ));
    }
    Ok(())
}

fn write_settings(dir: &Path, stored: &StoredSettings) -> Result<(), CommandError> {
    let json = serde_json::to_string_pretty(stored)
        .map_err(|error| CommandError::internal(format!("serialize settings: {error}")))?;
    let target = dir.join(SETTINGS_FILE);
    let temporary = dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&temporary, json).map_err(|error| CommandError::from_io(&error))?;
    if target.exists() {
        fs::remove_file(&target).map_err(|error| CommandError::from_io(&error))?;
    }
    fs::rename(&temporary, &target).map_err(|error| CommandError::from_io(&error))
}

fn write_token(dir: &Path, token: Option<&str>) -> Result<(), CommandError> {
    let target = dir.join(TOKEN_FILE);
    match token {
        Some(token) => {
            let encrypted =
                windows_dpapi::encrypt_data(token.as_bytes(), windows_dpapi::Scope::User, None)
                    .map_err(|error| {
                        CommandError::internal(format!("encrypt token with DPAPI: {error}"))
                    })?;
            fs::write(&target, encrypted).map_err(|error| CommandError::from_io(&error))
        }
        None => match fs::remove_file(&target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(CommandError::from_io(&error)),
        },
    }
}

fn backup_corrupt(path: &Path, dir: &Path) {
    let backup = dir.join(CORRUPT_SETTINGS_FILE);
    let _ = fs::rename(path, backup);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::SettingsStore;
    use crate::commands::models::KeepWarm;
    use crate::commands::settings::SettingsPatch;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-settings-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn defaults_match_the_documented_roles() {
        let dir = TempDir::new("defaults");
        let store = SettingsStore::load(&dir.path).expect("load defaults");
        let settings = store.get();

        assert_eq!(settings.endpoint, "http://127.0.0.1:1234");
        assert_eq!(settings.describer_model, "lfm3-vl-2b");
        assert_eq!(
            settings.composer_model,
            "gemma4-12b-qat-uncensored-hauhaucs-balanced"
        );
        assert_eq!(settings.describer_concurrency, 2);
        assert_eq!(settings.ttl_seconds, 600);
        assert_eq!(settings.keep_warm, KeepWarm::None);
        assert!(!settings.has_token);
    }

    #[test]
    fn rejects_invalid_endpoint_and_keeps_previous_value() {
        let dir = TempDir::new("endpoint");
        let mut store = SettingsStore::load(&dir.path).expect("load");
        let original = store.get().endpoint;

        let error = store
            .update(SettingsPatch {
                endpoint: Some("not a url".into()),
                ..Default::default()
            })
            .expect_err("invalid endpoint must fail");
        assert_eq!(error.code, "validation");
        assert_eq!(store.get().endpoint, original);
    }

    #[test]
    fn rejects_out_of_range_concurrency() {
        let dir = TempDir::new("concurrency");
        let mut store = SettingsStore::load(&dir.path).expect("load");

        for value in [0, 5] {
            let error = store
                .update(SettingsPatch {
                    describer_concurrency: Some(value),
                    ..Default::default()
                })
                .expect_err("out of range concurrency must fail");
            assert_eq!(error.code, "validation");
        }
    }

    #[test]
    fn persists_settings_across_reloads() {
        let dir = TempDir::new("reload");
        {
            let mut store = SettingsStore::load(&dir.path).expect("load");
            store
                .update(SettingsPatch {
                    theme: Some("light".into()),
                    describer_concurrency: Some(4),
                    ..Default::default()
                })
                .expect("update");
        }

        let store = SettingsStore::load(&dir.path).expect("reload");
        let settings = store.get();
        assert_eq!(settings.theme, "light");
        assert_eq!(settings.describer_concurrency, 4);
    }

    #[test]
    fn system_prompt_overrides_round_trip_and_clear() {
        let dir = TempDir::new("system-prompts");
        let mut store = SettingsStore::load(&dir.path).expect("load");
        assert!(store.get().describer_system_prompt.is_none());
        assert!(store.get().composer_system_prompt.is_none());

        store
            .update(SettingsPatch {
                describer_system_prompt: Some("Describe carefully.".into()),
                composer_system_prompt: Some("Write terse prompts.".into()),
                ..Default::default()
            })
            .expect("set prompts");
        assert_eq!(
            store.get().describer_system_prompt.as_deref(),
            Some("Describe carefully.")
        );

        let reloaded = SettingsStore::load(&dir.path).expect("reload");
        assert_eq!(
            reloaded.get().composer_system_prompt.as_deref(),
            Some("Write terse prompts.")
        );

        store
            .update(SettingsPatch {
                composer_system_prompt: Some("   ".into()),
                ..Default::default()
            })
            .expect("clear prompt");
        assert!(store.get().composer_system_prompt.is_none());
    }

    #[test]
    fn token_is_dpapi_encrypted_and_round_trips() {
        let dir = TempDir::new("token");
        let secret = "lm-studio-secret-token";
        {
            let mut store = SettingsStore::load(&dir.path).expect("load");
            store
                .update(SettingsPatch {
                    token: Some(secret.into()),
                    ..Default::default()
                })
                .expect("set token");
            assert!(store.get().has_token);
            assert_eq!(store.token(), Some(secret));
        }

        let raw_settings =
            fs::read_to_string(dir.path.join("settings.json")).expect("read settings");
        assert!(!raw_settings.contains(secret));

        let raw_token = fs::read(dir.path.join("token.bin")).expect("read token blob");
        assert_ne!(raw_token, secret.as_bytes());

        let store = SettingsStore::load(&dir.path).expect("reload");
        assert_eq!(store.token(), Some(secret));
    }

    #[test]
    fn a_foreign_token_blob_is_treated_as_absent() {
        let dir = TempDir::new("foreign-token");
        fs::write(dir.path.join("token.bin"), b"not-a-dpapi-blob").expect("write token");
        let store = SettingsStore::load(&dir.path).expect("load");
        assert_eq!(store.token(), None);
        assert!(!store.get().has_token);
    }

    #[test]
    fn clearing_the_token_removes_the_blob() {
        let dir = TempDir::new("clear-token");
        let mut store = SettingsStore::load(&dir.path).expect("load");
        store
            .update(SettingsPatch {
                token: Some("secret".into()),
                ..Default::default()
            })
            .expect("set token");
        store
            .update(SettingsPatch {
                token: Some(String::new()),
                ..Default::default()
            })
            .expect("clear token");

        assert!(!store.get().has_token);
        assert!(!dir.path.join("token.bin").exists());
    }

    #[test]
    fn corrupt_settings_file_is_backed_up_and_defaults_are_used() {
        let dir = TempDir::new("corrupt");
        fs::write(dir.path.join("settings.json"), "{not json").expect("write corrupt file");

        let store = SettingsStore::load(&dir.path).expect("load with corrupt file");
        assert_eq!(store.get().endpoint, "http://127.0.0.1:1234");
        assert!(dir.path.join("settings.corrupt.json").exists());
    }
}
