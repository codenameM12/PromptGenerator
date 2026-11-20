//! All view state lives here, independent of egui widgets. Views render from
//! this state and mutate it in place; navigation only changes `active` and a
//! visited flag, so inputs, filters and editor state survive section switches.

use crate::backend::Asset;
use crate::shortcuts::FocusContext;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewId {
    Compose,
    Media,
    Library,
    Jobs,
    Presets,
    Runtime,
    Characters,
    Datasets,
}

impl ViewId {
    pub const ALL: [ViewId; 8] = [
        ViewId::Compose,
        ViewId::Media,
        ViewId::Library,
        ViewId::Jobs,
        ViewId::Presets,
        ViewId::Runtime,
        ViewId::Characters,
        ViewId::Datasets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewId::Compose => "Compose",
            ViewId::Media => "Media",
            ViewId::Library => "Library",
            ViewId::Jobs => "Jobs",
            ViewId::Presets => "Presets",
            ViewId::Runtime => "Runtime",
            ViewId::Characters => "Characters",
            ViewId::Datasets => "Datasets",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeMode {
    Describe,
    Compose,
    Refine,
    Batch,
}

impl ComposeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ComposeMode::Describe => "describe",
            ComposeMode::Compose => "compose",
            ComposeMode::Refine => "refine",
            ComposeMode::Batch => "batch",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EditState {
    pub crop_x: f32,
    pub crop_y: f32,
    pub crop_width: f32,
    pub crop_height: f32,
    pub fps: f32,
    pub trim_start_ms: u64,
    pub trim_end_ms: u64,
    pub history_cursor: usize,
}

#[derive(Debug, Clone)]
pub struct ComposeState {
    pub mode: ComposeMode,
    pub brief: String,
    pub preset_id: String,
    pub run_guidance: String,
    pub selected_asset_ids: Vec<String>,
    pub descriptions: Vec<(String, String)>,
    pub result: Option<String>,
    pub visited: bool,
}

impl Default for ComposeState {
    fn default() -> Self {
        Self {
            mode: ComposeMode::Compose,
            brief: String::new(),
            preset_id: "mmh3".to_string(),
            run_guidance: String::new(),
            selected_asset_ids: Vec::new(),
            descriptions: Vec::new(),
            result: None,
            visited: true, // Compose is the default, mounted view.
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MediaState {
    pub selected_asset_id: Option<String>,
    pub edit: EditState,
    pub visited: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub query: String,
    pub favorites_only: bool,
    pub preset_filter: String,
    pub status_filter: String,
    pub tag_filter: String,
    pub sort: String,
    pub trash_only: bool,
    /// Only generations that used this character (id); empty = no filter.
    pub character_filter: String,
    /// Generation ids that used `character_filter` (refreshed with the list).
    pub character_ids: Vec<String>,
    /// Rows ticked for a bulk action, and the anchor for shift-range ticking.
    pub checked: std::collections::HashSet<String>,
    pub check_anchor: Option<String>,
    pub selected_generation_id: Option<String>,
    pub visited: bool,
    /// Set once the first draw has issued the mount refresh through the backend.
    pub loaded: bool,
    /// `library_query` page items and total, stored as returned JSON.
    pub records: Vec<Value>,
    pub total: i64,
    /// `library_get` detail for the selected generation.
    pub detail: Option<Value>,
    /// Last user-visible notice or error text.
    pub notice: Option<String>,
    pub error: Option<String>,
    /// presets_list / library_tags / library_collections combined by key.
    pub meta: Value,
}

#[derive(Debug, Clone, Default)]
pub struct JobsState {
    pub selected_job_id: Option<String>,
    pub paused: bool,
    pub visited: bool,
    pub loaded: bool,
    pub jobs: Vec<Value>,
    pub detail: Option<Value>,
    pub queue_status: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PresetsState {
    pub selected_preset_id: Option<String>,
    pub json_text: String,
    pub visited: bool,
    pub loaded: bool,
    pub presets: Vec<Value>,
    pub document: Option<Value>,
    pub compiled: Option<Value>,
    pub builtin: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    pub describer: String,
    pub composer: String,
    pub endpoint: String,
    pub keep_warm: String,
    pub describer_prompt: String,
    pub composer_prompt: String,
    pub visited: bool,
    pub loaded: bool,
    pub models: Vec<Value>,
    pub status: Option<Value>,
    pub settings: Option<Value>,
    pub error: Option<String>,
    /// Confirmation after a successful save (instructions, roles...).
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CharactersState {
    pub visited: bool,
    pub loaded: bool,
    pub list: Vec<Value>,
    pub selected: Option<String>,
    pub detail: Option<Value>,
    pub new_name: String,
    pub name_draft: String,
    pub description_draft: String,
    /// Unsaved per-image edits: (name, role, description, default).
    pub drafts: std::collections::HashMap<String, (String, String, String, bool)>,
    /// Screen areas (and character ids) that accept dropped files this frame.
    pub drop_zones: Vec<(egui::Rect, String)>,
    pub notice: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DatasetsState {
    pub source_path: String,
    pub mapping_profile: String,
    pub selected_dataset_id: Option<String>,
    pub preset_id: String,
    pub dedupe: String,
    pub visited: bool,
    pub loaded: bool,
    pub datasets: Vec<Value>,
    pub preview: Option<Value>,
    pub rows: Option<Value>,
    pub summary: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ViewModel {
    pub active: ViewId,
    pub compose: ComposeState,
    pub media: MediaState,
    pub library: LibraryState,
    pub jobs: JobsState,
    pub presets: PresetsState,
    pub runtime: RuntimeState,
    pub datasets: DatasetsState,
    pub characters: CharactersState,
    pub assets: Vec<Asset>,
    pub status: String,
    pub modal_open: bool,
    pub text_input_focused: bool,
    pub busy: bool,
    /// True while files hover the window, so the shell can show a drop hint.
    pub drop_hint: bool,
    /// Asset count reported by the Compose mount, so the status bar is not
    /// limited to views that happen to copy the grid into `assets`.
    pub asset_count: usize,
    /// Bumped whenever an edit is applied so Compose reloads the edited images.
    pub edits_version: u64,
    /// Bumped when media was added elsewhere (a set loaded in Compose) so the
    /// Media tab reloads its grid.
    pub media_reload: u64,
    /// Bumped when the Characters list changes so Compose refreshes its picker.
    pub characters_version: u64,
}

impl Default for ViewModel {
    fn default() -> Self {
        Self {
            active: ViewId::Compose,
            compose: ComposeState::default(),
            media: MediaState::default(),
            library: LibraryState::default(),
            jobs: JobsState::default(),
            presets: PresetsState::default(),
            runtime: RuntimeState::default(),
            datasets: DatasetsState::default(),
            characters: CharactersState::default(),
            assets: Vec::new(),
            status: String::new(),
            modal_open: false,
            text_input_focused: false,
            busy: false,
            drop_hint: false,
            asset_count: 0,
            media_reload: 0,
            characters_version: 0,
            edits_version: 0,
        }
    }
}

impl ViewModel {
    /// Switches the visible section. Every state struct stays in `self`, so no
    /// input, filter, selection or editor value is reset.
    pub fn navigate(&mut self, id: ViewId) {
        self.active = id;
        match id {
            ViewId::Compose => self.compose.visited = true,
            ViewId::Media => self.media.visited = true,
            ViewId::Library => self.library.visited = true,
            ViewId::Jobs => self.jobs.visited = true,
            ViewId::Presets => self.presets.visited = true,
            ViewId::Runtime => self.runtime.visited = true,
            ViewId::Characters => self.characters.visited = true,
            ViewId::Datasets => self.datasets.visited = true,
        }
    }

    pub fn is_visited(&self, id: ViewId) -> bool {
        match id {
            ViewId::Compose => self.compose.visited,
            ViewId::Media => self.media.visited,
            ViewId::Library => self.library.visited,
            ViewId::Jobs => self.jobs.visited,
            ViewId::Presets => self.presets.visited,
            ViewId::Runtime => self.runtime.visited,
            ViewId::Characters => self.characters.visited,
            ViewId::Datasets => self.datasets.visited,
        }
    }

    pub fn focus_context(&self) -> FocusContext {
        FocusContext {
            modal_open: self.modal_open,
            text_input_focused: self.text_input_focused,
        }
    }

    /// Serializes the unapplied draft for window persistence.
    pub fn draft(&self) -> crate::persistence::Draft {
        crate::persistence::Draft {
            mode: self.compose.mode.as_str().to_string(),
            brief: self.compose.brief.clone(),
            preset_id: self.compose.preset_id.clone(),
            selected_asset_ids: self.compose.selected_asset_ids.clone(),
            result: self.compose.result.clone(),
        }
    }

    pub fn restore_draft(&mut self, draft: &crate::persistence::Draft) {
        self.compose.mode = match draft.mode.as_str() {
            "describe" => ComposeMode::Describe,
            "refine" => ComposeMode::Refine,
            "batch" => ComposeMode::Batch,
            _ => ComposeMode::Compose,
        };
        self.compose.brief = draft.brief.clone();
        self.compose.preset_id = draft.preset_id.clone();
        self.compose.selected_asset_ids = draft.selected_asset_ids.clone();
        self.compose.result = draft.result.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_preserves_inputs_and_editor_state() {
        let mut vm = ViewModel::default();
        vm.compose.brief = "an amber lighthouse".into();
        vm.compose.preset_id = "krea2".into();
        vm.compose.selected_asset_ids.push("asset-1".into());
        vm.library.query = "cats".into();
        vm.media.selected_asset_id = Some("asset-1".into());
        vm.media.edit.crop_x = 12.0;
        vm.media.edit.crop_width = 320.0;
        vm.media.edit.history_cursor = 4;

        vm.navigate(ViewId::Media);
        vm.navigate(ViewId::Library);
        vm.navigate(ViewId::Datasets);
        vm.navigate(ViewId::Compose);

        assert_eq!(vm.active, ViewId::Compose);
        assert_eq!(vm.compose.brief, "an amber lighthouse");
        assert_eq!(vm.compose.preset_id, "krea2");
        assert_eq!(vm.compose.selected_asset_ids, vec!["asset-1"]);
        assert_eq!(vm.library.query, "cats");
        assert_eq!(vm.media.selected_asset_id.as_deref(), Some("asset-1"));
        assert_eq!(vm.media.edit.crop_x, 12.0);
        assert_eq!(vm.media.edit.crop_width, 320.0);
        assert_eq!(vm.media.edit.history_cursor, 4);
    }

    #[test]
    fn all_eight_views_start_known_and_visit_tracked() {
        let mut vm = ViewModel::default();
        assert_eq!(ViewId::ALL.len(), 8);
        assert!(vm.compose.visited);
        for view in ViewId::ALL {
            vm.navigate(view);
            assert!(vm.is_visited(view), "{view:?} should be visited");
        }
    }

    #[test]
    fn draft_round_trips_through_view_model() {
        let mut vm = ViewModel::default();
        vm.compose.brief = "draft text".into();
        vm.compose.selected_asset_ids.push("a".into());
        let draft = vm.draft();
        vm.compose.brief.clear();
        vm.compose.selected_asset_ids.clear();
        vm.restore_draft(&draft);
        assert_eq!(vm.compose.brief, "draft text");
        assert_eq!(vm.compose.selected_asset_ids, vec!["a"]);
    }
}
