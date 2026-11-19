//! State persisted through eframe's storage backend.
//!
//! eframe persists window geometry itself (`NativeOptions::persist_window`), and
//! this module additionally stores the last selected asset and the unapplied
//! compose draft as one JSON blob so a restart restores the workspace.

use serde::{Deserialize, Serialize};

pub const STORAGE_KEY: &str = "prompt-studio-native.workspace";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub mode: String,
    pub brief: String,
    pub preset_id: String,
    pub selected_asset_ids: Vec<String>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedState {
    pub geometry: Option<Geometry>,
    pub last_asset_id: Option<String>,
    pub draft: Draft,
}

pub fn encode(state: &PersistedState) -> Result<String, serde_json::Error> {
    serde_json::to_string(state)
}

pub fn decode(raw: &str) -> Result<PersistedState, serde_json::Error> {
    serde_json::from_str(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_geometry_asset_and_draft() {
        let state = PersistedState {
            geometry: Some(Geometry {
                x: 10.0,
                y: 20.0,
                width: 1280.0,
                height: 800.0,
            }),
            last_asset_id: Some("asset-7".into()),
            draft: Draft {
                mode: "compose".into(),
                brief: "an amber lighthouse".into(),
                preset_id: "krea2".into(),
                selected_asset_ids: vec!["asset-7".into(), "asset-8".into()],
                result: Some("A cinematic lighthouse...".into()),
            },
        };
        let raw = encode(&state).expect("encode");
        let restored = decode(&raw).expect("decode");
        assert_eq!(restored, state);
        assert_eq!(restored.geometry.unwrap().width, 1280.0);
        assert_eq!(restored.draft.selected_asset_ids.len(), 2);
    }

    #[test]
    fn decodes_state_without_geometry() {
        let raw = r#"{"lastAssetId":"asset-1","draft":{"mode":"describe","brief":"","presetId":"","selectedAssetIds":[],"result":null}}"#;
        let state = decode(raw).expect("decode");
        assert!(state.geometry.is_none());
        assert_eq!(state.last_asset_id.as_deref(), Some("asset-1"));
        assert_eq!(state.draft.mode, "describe");
    }
}
