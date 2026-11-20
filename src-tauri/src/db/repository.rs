//! Typed SQLite repositories. Each module owns one aggregate and keeps SQL out
//! of the command layer.

pub mod assets {
    use std::path::Path;

    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct AssetRow {
        pub id: String,
        pub hash: String,
        pub kind: String,
        pub source_path: String,
        pub display_name: String,
        pub width: Option<u32>,
        pub height: Option<u32>,
        pub fps: Option<f64>,
        pub duration_ms: Option<i64>,
        pub codec: Option<String>,
        pub has_audio: bool,
        pub color: Option<String>,
        pub rotation: i32,
        pub thumbnail_path: Option<String>,
        pub proxy_path: Option<String>,
        pub embedded_metadata: Option<String>,
        pub favorite: bool,
        pub missing: bool,
        pub created_at: String,
    }

    #[derive(Debug, Clone)]
    pub struct NewAsset {
        pub id: String,
        pub hash: String,
        pub kind: String,
        pub source_path: String,
        pub display_name: String,
        pub width: Option<u32>,
        pub height: Option<u32>,
        pub fps: Option<f64>,
        pub duration_ms: Option<i64>,
        pub codec: Option<String>,
        pub has_audio: bool,
        pub color: Option<String>,
        pub rotation: i32,
        pub thumbnail_path: Option<String>,
        pub proxy_path: Option<String>,
        pub embedded_metadata: Option<String>,
    }

    const COLUMNS: &str = "id, hash, kind, source_path, display_name, width, height, fps, \
                           duration_ms, codec, has_audio, color, rotation, thumbnail_path, \
                           proxy_path, embedded_metadata, favorite, missing, created_at";

    fn map(row: &Row<'_>) -> rusqlite::Result<AssetRow> {
        Ok(AssetRow {
            id: row.get(0)?,
            hash: row.get(1)?,
            kind: row.get(2)?,
            source_path: row.get(3)?,
            display_name: row.get(4)?,
            width: row.get(5)?,
            height: row.get(6)?,
            fps: row.get(7)?,
            duration_ms: row.get(8)?,
            codec: row.get(9)?,
            has_audio: row.get(10)?,
            color: row.get(11)?,
            rotation: row.get(12)?,
            thumbnail_path: row.get(13)?,
            proxy_path: row.get(14)?,
            embedded_metadata: row.get(15)?,
            favorite: row.get(16)?,
            missing: row.get(17)?,
            created_at: row.get(18)?,
        })
    }

    pub fn insert(connection: &Connection, asset: &NewAsset) -> Result<AssetRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO assets (id, hash, kind, source_path, display_name, width, height, fps,
                 duration_ms, codec, has_audio, color, rotation, thumbnail_path, proxy_path,
                 embedded_metadata, favorite, missing, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0, 0, ?17)",
            params![
                asset.id,
                asset.hash,
                asset.kind,
                asset.source_path,
                asset.display_name,
                asset.width,
                asset.height,
                asset.fps,
                asset.duration_ms,
                asset.codec,
                asset.has_audio,
                asset.color,
                asset.rotation,
                asset.thumbnail_path,
                asset.proxy_path,
                asset.embedded_metadata,
                created_at,
            ],
        )?;
        get(connection, &asset.id)?.ok_or_else(|| CommandError::internal("asset insert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<AssetRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM assets WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    pub fn find_by_hash(
        connection: &Connection,
        hash: &str,
    ) -> Result<Option<AssetRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM assets WHERE hash = ?1");
        Ok(connection.query_row(&sql, [hash], map).optional()?)
    }

    pub fn list(
        connection: &Connection,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AssetRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM assets WHERE deleted_at IS NULL \
             ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params![limit, offset], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_favorite(
        connection: &Connection,
        id: &str,
        favorite: bool,
    ) -> Result<bool, CommandError> {
        let changed = connection.execute(
            "UPDATE assets SET favorite = ?1 WHERE id = ?2",
            params![favorite, id],
        )?;
        Ok(changed > 0)
    }

    pub fn set_missing(
        connection: &Connection,
        id: &str,
        missing: bool,
    ) -> Result<bool, CommandError> {
        let changed = connection.execute(
            "UPDATE assets SET missing = ?1 WHERE id = ?2",
            params![missing, id],
        )?;
        Ok(changed > 0)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM assets WHERE id = ?1", [id])? > 0)
    }

    pub fn soft_delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE assets SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )? > 0)
    }

    pub fn restore(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("UPDATE assets SET deleted_at = NULL WHERE id = ?1", [id])? > 0)
    }

    /// Whether an asset currently sits in the trash.
    pub fn is_trashed(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection
            .query_row(
                "SELECT deleted_at IS NOT NULL FROM assets WHERE id = ?1",
                [id],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false))
    }

    pub fn hard_delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM assets WHERE id = ?1", [id])? > 0)
    }

    /// Rows in the asset trash, newest first.
    pub fn list_deleted(connection: &Connection) -> Result<Vec<AssetRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM assets WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Favorite assets, independent of generation favorites.
    pub fn list_favorites(connection: &Connection) -> Result<Vec<AssetRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM assets WHERE favorite = 1 AND deleted_at IS NULL
             ORDER BY created_at DESC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Re-checks source paths and flips the `missing` flag. Returns the number
    /// of rows whose state changed.
    pub fn refresh_missing(connection: &Connection) -> Result<usize, CommandError> {
        let rows = list(connection, 1_000_000, 0)?;
        let mut changed = 0;

        for row in rows {
            let missing = !Path::new(&row.source_path).is_file();
            if missing != row.missing {
                set_missing(connection, &row.id, missing)?;
                changed += 1;
            }
        }

        Ok(changed)
    }
}

pub mod frames {
    use rusqlite::{Connection, params};

    use crate::commands::CommandError;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FrameRow {
        pub frame_index: i64,
        pub timestamp_ms: i64,
        pub path: String,
    }

    pub fn replace_for_asset(
        connection: &Connection,
        asset_id: &str,
        frames: &[FrameRow],
    ) -> Result<(), CommandError> {
        connection.execute("DELETE FROM asset_frames WHERE asset_id = ?1", [asset_id])?;
        for frame in frames {
            connection.execute(
                "INSERT INTO asset_frames (asset_id, frame_index, timestamp_ms, path)
                 VALUES (?1, ?2, ?3, ?4)",
                params![asset_id, frame.frame_index, frame.timestamp_ms, frame.path],
            )?;
        }
        Ok(())
    }

    pub fn list_for_asset(
        connection: &Connection,
        asset_id: &str,
    ) -> Result<Vec<FrameRow>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT frame_index, timestamp_ms, path FROM asset_frames
             WHERE asset_id = ?1 ORDER BY frame_index",
        )?;
        let rows = statement.query_map([asset_id], |row| {
            Ok(FrameRow {
                frame_index: row.get(0)?,
                timestamp_ms: row.get(1)?,
                path: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub mod edits {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct EditRow {
        pub id: String,
        pub asset_id: String,
        pub ops_json: String,
        pub target_json: String,
        pub output_path: String,
        pub cache_key: String,
        pub width: Option<u32>,
        pub height: Option<u32>,
        pub fps: Option<f64>,
        pub duration_ms: Option<i64>,
        pub created_at: String,
    }

    #[derive(Debug, Clone)]
    pub struct NewEdit {
        pub id: String,
        pub asset_id: String,
        pub ops_json: String,
        pub target_json: String,
        pub output_path: String,
        pub cache_key: String,
        pub width: Option<u32>,
        pub height: Option<u32>,
        pub fps: Option<f64>,
        pub duration_ms: Option<i64>,
    }

    const COLUMNS: &str = "id, asset_id, ops_json, target_json, output_path, cache_key, width, height, fps, duration_ms, created_at";

    fn map(row: &Row<'_>) -> rusqlite::Result<EditRow> {
        Ok(EditRow {
            id: row.get(0)?,
            asset_id: row.get(1)?,
            ops_json: row.get(2)?,
            target_json: row.get(3)?,
            output_path: row.get(4)?,
            cache_key: row.get(5)?,
            width: row.get(6)?,
            height: row.get(7)?,
            fps: row.get(8)?,
            duration_ms: row.get(9)?,
            created_at: row.get(10)?,
        })
    }

    pub fn insert(connection: &Connection, edit: &NewEdit) -> Result<EditRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO edits (id, asset_id, ops_json, target_json, output_path, cache_key,
                 width, height, fps, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                edit.id,
                edit.asset_id,
                edit.ops_json,
                edit.target_json,
                edit.output_path,
                edit.cache_key,
                edit.width,
                edit.height,
                edit.fps,
                edit.duration_ms,
                created_at,
            ],
        )?;
        get(connection, &edit.id)?.ok_or_else(|| CommandError::internal("edit insert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<EditRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM edits WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    pub fn find_by_cache_key(
        connection: &Connection,
        cache_key: &str,
    ) -> Result<Option<EditRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM edits WHERE cache_key = ?1");
        Ok(connection.query_row(&sql, [cache_key], map).optional()?)
    }

    pub fn list_for_asset(
        connection: &Connection,
        asset_id: &str,
    ) -> Result<Vec<EditRow>, CommandError> {
        let sql =
            format!("SELECT {COLUMNS} FROM edits WHERE asset_id = ?1 ORDER BY created_at DESC");
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([asset_id], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Newest applied edit for an asset, if any.
    pub fn latest_for_asset(
        connection: &Connection,
        asset_id: &str,
    ) -> Result<Option<EditRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM edits WHERE asset_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1"
        );
        Ok(connection.query_row(&sql, [asset_id], map).optional()?)
    }

    /// Newest applied edit per requested asset; assets without edits are absent.
    pub fn latest_by_assets(
        connection: &Connection,
        asset_ids: &[String],
    ) -> Result<Vec<EditRow>, CommandError> {
        let mut latest = Vec::new();
        for asset_id in asset_ids {
            if let Some(row) = latest_for_asset(connection, asset_id)? {
                latest.push(row);
            }
        }
        Ok(latest)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM edits WHERE id = ?1", [id])? > 0)
    }
}

pub mod descriptions {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct DescriptionRow {
        pub id: String,
        pub asset_id: Option<String>,
        pub edit_id: Option<String>,
        pub model: String,
        pub prompt_version: String,
        pub sampling_json: String,
        pub cache_key: String,
        pub json: String,
        pub confidence: Option<f64>,
        pub cached: bool,
        pub created_at: String,
    }

    #[derive(Debug, Clone)]
    pub struct NewDescription {
        pub id: String,
        pub asset_id: Option<String>,
        pub edit_id: Option<String>,
        pub model: String,
        pub prompt_version: String,
        pub sampling_json: String,
        pub cache_key: String,
        pub json: String,
        pub confidence: Option<f64>,
        pub cached: bool,
    }

    const COLUMNS: &str = "id, asset_id, edit_id, model, prompt_version, sampling_json, cache_key, json, confidence, cached, created_at";

    fn map(row: &Row<'_>) -> rusqlite::Result<DescriptionRow> {
        Ok(DescriptionRow {
            id: row.get(0)?,
            asset_id: row.get(1)?,
            edit_id: row.get(2)?,
            model: row.get(3)?,
            prompt_version: row.get(4)?,
            sampling_json: row.get(5)?,
            cache_key: row.get(6)?,
            json: row.get(7)?,
            confidence: row.get(8)?,
            cached: row.get(9)?,
            created_at: row.get(10)?,
        })
    }

    pub fn insert(
        connection: &Connection,
        description: &NewDescription,
    ) -> Result<DescriptionRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO descriptions (id, asset_id, edit_id, model, prompt_version, sampling_json,
                 cache_key, json, confidence, cached, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                description.id,
                description.asset_id,
                description.edit_id,
                description.model,
                description.prompt_version,
                description.sampling_json,
                description.cache_key,
                description.json,
                description.confidence,
                description.cached,
                created_at,
            ],
        )?;
        get(connection, &description.id)?
            .ok_or_else(|| CommandError::internal("description insert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<DescriptionRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM descriptions WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    pub fn find_by_cache_key(
        connection: &Connection,
        cache_key: &str,
    ) -> Result<Option<DescriptionRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM descriptions WHERE cache_key = ?1");
        Ok(connection.query_row(&sql, [cache_key], map).optional()?)
    }

    pub fn list_for_asset(
        connection: &Connection,
        asset_id: &str,
    ) -> Result<Vec<DescriptionRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM descriptions WHERE asset_id = ?1 ORDER BY created_at DESC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([asset_id], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM descriptions WHERE id = ?1", [id])? > 0)
    }
}

pub mod presets {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct PresetRow {
        pub id: String,
        pub owner: String,
        pub name: String,
        pub kind: String,
        pub version: String,
        pub description: String,
        pub document_json: String,
        pub content_hash: String,
        pub created_at: String,
        pub updated_at: String,
        pub deleted_at: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct NewPreset {
        pub id: String,
        pub owner: String,
        pub name: String,
        pub kind: String,
        pub version: String,
        pub description: String,
        pub document_json: String,
        pub content_hash: String,
    }

    const COLUMNS: &str = "id, owner, name, kind, version, description, document_json, content_hash, created_at, updated_at, deleted_at";

    fn map(row: &Row<'_>) -> rusqlite::Result<PresetRow> {
        Ok(PresetRow {
            id: row.get(0)?,
            owner: row.get(1)?,
            name: row.get(2)?,
            kind: row.get(3)?,
            version: row.get(4)?,
            description: row.get(5)?,
            document_json: row.get(6)?,
            content_hash: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            deleted_at: row.get(10)?,
        })
    }

    pub fn upsert(connection: &Connection, preset: &NewPreset) -> Result<PresetRow, CommandError> {
        let now = now_iso(connection)?;
        connection.execute(
            "INSERT INTO presets (id, owner, name, kind, version, description, document_json,
                 content_hash, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT (id) DO UPDATE SET
                 owner = excluded.owner,
                 name = excluded.name,
                 kind = excluded.kind,
                 version = excluded.version,
                 description = excluded.description,
                 document_json = excluded.document_json,
                 content_hash = excluded.content_hash,
                 updated_at = excluded.updated_at",
            params![
                preset.id,
                preset.owner,
                preset.name,
                preset.kind,
                preset.version,
                preset.description,
                preset.document_json,
                preset.content_hash,
                now,
            ],
        )?;
        get(connection, &preset.id)?.ok_or_else(|| CommandError::internal("preset upsert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<PresetRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM presets WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    pub fn list(
        connection: &Connection,
        include_deleted: bool,
    ) -> Result<Vec<PresetRow>, CommandError> {
        let sql = if include_deleted {
            format!("SELECT {COLUMNS} FROM presets ORDER BY owner, name")
        } else {
            format!("SELECT {COLUMNS} FROM presets WHERE deleted_at IS NULL ORDER BY owner, name")
        };
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        let changed = connection.execute(
            "UPDATE presets SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        Ok(changed > 0)
    }

    pub fn is_referenced(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM generations WHERE preset_id = ?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

pub mod generations {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct GenerationRow {
        pub id: String,
        pub parent_id: Option<String>,
        pub asset_id: Option<String>,
        pub edit_id: Option<String>,
        pub description_id: Option<String>,
        pub project_id: Option<String>,
        pub shot_index: Option<i64>,
        pub preset_id: String,
        pub preset_version: String,
        pub preset_hash: String,
        pub prompt: String,
        pub negative: Option<String>,
        pub params_json: String,
        pub guidance_json: String,
        pub description_json: Option<String>,
        pub seed: Option<i64>,
        pub models_json: String,
        pub status: String,
        pub error: Option<String>,
        pub raw_response: Option<String>,
        pub tokens_in: Option<i64>,
        pub tokens_out: Option<i64>,
        pub latency_ms: Option<i64>,
        pub cache_key: Option<String>,
        pub favorite: bool,
        pub rating: Option<i64>,
        pub notes: Option<String>,
        pub created_at: String,
        pub deleted_at: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct NewGeneration {
        pub id: String,
        pub parent_id: Option<String>,
        pub asset_id: Option<String>,
        pub edit_id: Option<String>,
        pub description_id: Option<String>,
        pub project_id: Option<String>,
        pub shot_index: Option<i64>,
        pub preset_id: String,
        pub preset_version: String,
        pub preset_hash: String,
        pub prompt: String,
        pub negative: Option<String>,
        pub params_json: String,
        pub guidance_json: String,
        pub description_json: Option<String>,
        pub seed: Option<i64>,
        pub models_json: String,
        pub status: String,
        pub error: Option<String>,
        pub raw_response: Option<String>,
        pub tokens_in: Option<i64>,
        pub tokens_out: Option<i64>,
        pub latency_ms: Option<i64>,
        pub cache_key: Option<String>,
    }

    const COLUMNS: &str = "id, parent_id, asset_id, edit_id, description_id, project_id, shot_index, \
        preset_id, preset_version, preset_hash, prompt, negative, params_json, guidance_json, \
        description_json, seed, models_json, status, error, raw_response, tokens_in, tokens_out, \
        latency_ms, cache_key, favorite, rating, notes, created_at, deleted_at";

    fn map(row: &Row<'_>) -> rusqlite::Result<GenerationRow> {
        Ok(GenerationRow {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            asset_id: row.get(2)?,
            edit_id: row.get(3)?,
            description_id: row.get(4)?,
            project_id: row.get(5)?,
            shot_index: row.get(6)?,
            preset_id: row.get(7)?,
            preset_version: row.get(8)?,
            preset_hash: row.get(9)?,
            prompt: row.get(10)?,
            negative: row.get(11)?,
            params_json: row.get(12)?,
            guidance_json: row.get(13)?,
            description_json: row.get(14)?,
            seed: row.get(15)?,
            models_json: row.get(16)?,
            status: row.get(17)?,
            error: row.get(18)?,
            raw_response: row.get(19)?,
            tokens_in: row.get(20)?,
            tokens_out: row.get(21)?,
            latency_ms: row.get(22)?,
            cache_key: row.get(23)?,
            favorite: row.get(24)?,
            rating: row.get(25)?,
            notes: row.get(26)?,
            created_at: row.get(27)?,
            deleted_at: row.get(28)?,
        })
    }

    pub fn insert(
        connection: &Connection,
        generation: &NewGeneration,
    ) -> Result<GenerationRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO generations (id, parent_id, asset_id, edit_id, description_id, project_id,
                 shot_index, preset_id, preset_version, preset_hash, prompt, negative, params_json,
                 guidance_json, description_json, seed, models_json, status, error, raw_response,
                 tokens_in, tokens_out, latency_ms, cache_key, favorite, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                 ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, 0, ?25)",
            params![
                generation.id,
                generation.parent_id,
                generation.asset_id,
                generation.edit_id,
                generation.description_id,
                generation.project_id,
                generation.shot_index,
                generation.preset_id,
                generation.preset_version,
                generation.preset_hash,
                generation.prompt,
                generation.negative,
                generation.params_json,
                generation.guidance_json,
                generation.description_json,
                generation.seed,
                generation.models_json,
                generation.status,
                generation.error,
                generation.raw_response,
                generation.tokens_in,
                generation.tokens_out,
                generation.latency_ms,
                generation.cache_key,
                created_at,
            ],
        )?;
        get(connection, &generation.id)?
            .ok_or_else(|| CommandError::internal("generation insert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<GenerationRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM generations WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    pub fn find_by_cache_key(
        connection: &Connection,
        cache_key: &str,
    ) -> Result<Option<GenerationRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM generations WHERE cache_key = ?1");
        Ok(connection.query_row(&sql, [cache_key], map).optional()?)
    }

    pub fn search(
        connection: &Connection,
        query: &str,
        limit: i64,
    ) -> Result<Vec<GenerationRow>, CommandError> {
        let match_query = normalize_query(query);
        if match_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut statement = connection.prepare(
            "SELECT g.* FROM generations_fts
             JOIN generations g ON g.rowid = generations_fts.rowid
             WHERE generations_fts MATCH ?1 AND g.deleted_at IS NULL
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![match_query, limit], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn normalize_query(query: &str) -> String {
        query
            .split_whitespace()
            .map(|token| format!("\"{}\"", token.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn list(
        connection: &Connection,
        limit: i64,
        offset: i64,
        include_deleted: bool,
    ) -> Result<Vec<GenerationRow>, CommandError> {
        let filter = if include_deleted {
            ""
        } else {
            "WHERE deleted_at IS NULL"
        };
        let sql = format!(
            "SELECT {COLUMNS} FROM generations {filter} ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params![limit, offset], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_favorite(
        connection: &Connection,
        id: &str,
        favorite: bool,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE generations SET favorite = ?1 WHERE id = ?2",
            params![favorite, id],
        )? > 0)
    }

    pub fn soft_delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE generations SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )? > 0)
    }

    pub fn restore(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE generations SET deleted_at = NULL WHERE id = ?1",
            [id],
        )? > 0)
    }

    pub fn hard_delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM generations WHERE id = ?1", [id])? > 0)
    }

    #[derive(Debug, Clone, Default)]
    pub struct GenerationFilter {
        pub text: Option<String>,
        pub preset_id: Option<String>,
        pub kind: Option<String>,
        pub model: Option<String>,
        pub favorite: Option<bool>,
        pub status: Option<String>,
        pub tags: Vec<String>,
        pub collection_id: Option<String>,
        pub from: Option<String>,
        pub to: Option<String>,
        pub include_deleted: bool,
        pub only_deleted: bool,
        pub ids: Option<Vec<String>>,
        pub sort: String,
        pub limit: i64,
        pub offset: i64,
    }

    /// Paginated, filtered history query. Search uses the FTS index and all
    /// other filters compose with it; the same predicate set drives COUNT so the
    /// page total always matches.
    pub fn query(
        connection: &Connection,
        filter: &GenerationFilter,
    ) -> Result<(Vec<GenerationRow>, i64), CommandError> {
        use rusqlite::params_from_iter;
        use rusqlite::types::Value;

        let mut joins = String::new();
        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<Value> = Vec::new();

        if let Some(text) = filter
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            joins.push_str(" JOIN generations_fts ON generations_fts.rowid = g.rowid");
            conditions.push("generations_fts MATCH ?".into());
            values.push(Value::Text(normalize_query(text)));
        }
        if let Some(preset_id) = &filter.preset_id {
            conditions.push("g.preset_id = ?".into());
            values.push(Value::Text(preset_id.clone()));
        }
        if let Some(kind) = &filter.kind {
            joins.push_str(" LEFT JOIN presets p ON p.id = g.preset_id");
            conditions.push("p.kind = ?".into());
            values.push(Value::Text(kind.clone()));
        }
        if let Some(model) = &filter.model {
            conditions.push("g.models_json LIKE ?".into());
            values.push(Value::Text(format!("%\"{model}\"%")));
        }
        if let Some(favorite) = filter.favorite {
            conditions.push("g.favorite = ?".into());
            values.push(Value::Integer(i64::from(favorite)));
        }
        if let Some(status) = &filter.status {
            conditions.push("g.status = ?".into());
            values.push(Value::Text(status.clone()));
        }
        if let Some(from) = &filter.from {
            conditions.push("g.created_at >= ?".into());
            values.push(Value::Text(from.clone()));
        }
        if let Some(to) = &filter.to {
            conditions.push("g.created_at <= ?".into());
            values.push(Value::Text(to.clone()));
        }
        if let Some(collection_id) = &filter.collection_id {
            conditions.push(
                "g.id IN (SELECT generation_id FROM collection_items WHERE collection_id = ?)"
                    .into(),
            );
            values.push(Value::Text(collection_id.clone()));
        }
        if !filter.tags.is_empty() {
            let placeholders = vec!["?"; filter.tags.len()].join(", ");
            conditions.push(format!(
                "g.id IN (
                    SELECT generation_id FROM generation_tags
                    JOIN tags ON tags.id = generation_tags.tag_id
                    WHERE tags.name IN ({placeholders})
                    GROUP BY generation_id HAVING COUNT(DISTINCT tags.name) = ?
                )"
            ));
            for tag in &filter.tags {
                values.push(Value::Text(tag.clone()));
            }
            values.push(Value::Integer(filter.tags.len() as i64));
        }
        if let Some(ids) = &filter.ids {
            if ids.is_empty() {
                return Ok((Vec::new(), 0));
            }
            let placeholders = vec!["?"; ids.len()].join(", ");
            conditions.push(format!("g.id IN ({placeholders})"));
            for id in ids {
                values.push(Value::Text(id.clone()));
            }
        }

        if filter.only_deleted {
            conditions.push("g.deleted_at IS NOT NULL".into());
        } else if !filter.include_deleted {
            conditions.push("g.deleted_at IS NULL".into());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let order = match filter.sort.as_str() {
            "oldest" => "g.created_at ASC",
            "rating" => "COALESCE(g.rating, -1) DESC, g.created_at DESC",
            "prompt" => "g.prompt COLLATE NOCASE ASC",
            _ => "g.created_at DESC",
        };

        let count_sql = format!("SELECT COUNT(*) FROM generations g{joins}{where_clause}");
        let total: i64 =
            connection.query_row(&count_sql, params_from_iter(values.iter()), |row| {
                row.get(0)
            })?;

        let sql = format!(
            "SELECT g.id, g.parent_id, g.asset_id, g.edit_id, g.description_id, g.project_id,
                    g.shot_index, g.preset_id, g.preset_version, g.preset_hash, g.prompt,
                    g.negative, g.params_json, g.guidance_json, g.description_json, g.seed,
                    g.models_json, g.status, g.error, g.raw_response, g.tokens_in, g.tokens_out,
                    g.latency_ms, g.cache_key, g.favorite, g.rating, g.notes, g.created_at,
                    g.deleted_at
             FROM generations g{joins}{where_clause}
             ORDER BY {order} LIMIT ? OFFSET ?"
        );
        let mut page_values = values;
        page_values.push(Value::Integer(filter.limit.clamp(1, 500)));
        page_values.push(Value::Integer(filter.offset.max(0)));

        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(page_values.iter()), map)?;
        Ok((rows.collect::<Result<Vec<_>, _>>()?, total))
    }

    pub fn set_rating(
        connection: &Connection,
        id: &str,
        rating: Option<i64>,
    ) -> Result<bool, CommandError> {
        if let Some(value) = rating
            && !(1..=5).contains(&value)
        {
            return Err(CommandError::validation("Ratings must be between 1 and 5"));
        }
        Ok(connection.execute(
            "UPDATE generations SET rating = ?1 WHERE id = ?2",
            params![rating, id],
        )? > 0)
    }

    pub fn set_notes(
        connection: &Connection,
        id: &str,
        notes: Option<&str>,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE generations SET notes = ?1 WHERE id = ?2",
            params![notes, id],
        )? > 0)
    }

    pub fn set_tags(
        connection: &Connection,
        id: &str,
        names: &[String],
    ) -> Result<(), CommandError> {
        connection.execute("DELETE FROM generation_tags WHERE generation_id = ?1", [id])?;
        for name in names
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
        {
            let tag_id = super::tags::ensure(connection, &uuid::Uuid::now_v7().to_string(), name)?;
            super::tags::attach(connection, id, &tag_id)?;
        }
        Ok(())
    }

    pub fn collections_for_generation(
        connection: &Connection,
        id: &str,
    ) -> Result<Vec<String>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT collections.name FROM collections
             JOIN collection_items ON collection_items.collection_id = collections.id
             WHERE collection_items.generation_id = ?1
             ORDER BY collections.name",
        )?;
        let rows = statement.query_map([id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn soft_delete_many(
        connection: &Connection,
        ids: &[String],
    ) -> Result<usize, CommandError> {
        let now = now_iso(connection)?;
        let mut changed = 0;
        for id in ids {
            changed += connection.execute(
                "UPDATE generations SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now, id],
            )?;
        }
        Ok(changed)
    }

    pub fn restore_many(connection: &Connection, ids: &[String]) -> Result<usize, CommandError> {
        let mut changed = 0;
        for id in ids {
            changed += connection.execute(
                "UPDATE generations SET deleted_at = NULL WHERE id = ?1",
                [id],
            )?;
        }
        Ok(changed)
    }

    pub fn hard_delete_many(
        connection: &Connection,
        ids: &[String],
    ) -> Result<usize, CommandError> {
        let mut changed = 0;
        for id in ids {
            changed += connection.execute("DELETE FROM generations WHERE id = ?1", [id])?;
        }
        Ok(changed)
    }

    /// Version tree rows for a generation: its ancestors plus every descendant.
    pub fn version_tree(
        connection: &Connection,
        id: &str,
    ) -> Result<Vec<GenerationRow>, CommandError> {
        let mut root = id.to_string();
        loop {
            let parent: Option<Option<String>> = connection
                .query_row(
                    "SELECT parent_id FROM generations WHERE id = ?1",
                    [&root],
                    |row| row.get(0),
                )
                .optional()?;
            match parent {
                Some(Some(parent)) => root = parent,
                _ => break,
            }
        }

        let sql = format!(
            "WITH RECURSIVE tree(id) AS (
                 SELECT id FROM generations WHERE id = ?1
                 UNION ALL
                 SELECT g.id FROM generations g JOIN tree t ON g.parent_id = t.id
             )
             SELECT {COLUMNS} FROM generations WHERE id IN (SELECT id FROM tree)
             ORDER BY created_at ASC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([root], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub mod jobs {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    pub const ACTIVE_STATES: [&str; 4] = ["queued", "waiting", "running", "interrupted"];

    #[derive(Debug, Clone, PartialEq)]
    pub struct JobRow {
        pub id: String,
        pub kind: String,
        pub state: String,
        pub progress: f64,
        pub payload_json: String,
        pub error: Option<String>,
        pub attempts: i64,
        pub priority: i64,
        pub dedupe_key: Option<String>,
        pub note: Option<String>,
        pub created_at: String,
        pub started_at: Option<String>,
        pub finished_at: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct NewJob {
        pub id: String,
        pub kind: String,
        pub state: String,
        pub payload_json: String,
        pub priority: i64,
        pub dedupe_key: Option<String>,
        pub note: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct JobItemRow {
        pub id: String,
        pub job_id: String,
        pub item_index: i64,
        pub state: String,
        pub payload_json: String,
        pub result_json: Option<String>,
        pub error: Option<String>,
        pub attempts: i64,
        pub idempotency_key: Option<String>,
        pub started_at: Option<String>,
        pub finished_at: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct NewJobItem {
        pub id: String,
        pub job_id: String,
        pub item_index: i64,
        pub state: String,
        pub payload_json: String,
        pub idempotency_key: Option<String>,
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct ItemCounts {
        pub total: i64,
        pub queued: i64,
        pub running: i64,
        pub waiting: i64,
        pub succeeded: i64,
        pub failed: i64,
        pub cancelled: i64,
        pub interrupted: i64,
    }

    impl ItemCounts {
        pub fn finished(&self) -> i64 {
            self.succeeded + self.failed + self.cancelled
        }
    }

    const COLUMNS: &str = "id, kind, state, progress, payload_json, error, attempts, priority, \
                           dedupe_key, note, created_at, started_at, finished_at";

    fn map(row: &Row<'_>) -> rusqlite::Result<JobRow> {
        Ok(JobRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            state: row.get(2)?,
            progress: row.get(3)?,
            payload_json: row.get(4)?,
            error: row.get(5)?,
            attempts: row.get(6)?,
            priority: row.get(7)?,
            dedupe_key: row.get(8)?,
            note: row.get(9)?,
            created_at: row.get(10)?,
            started_at: row.get(11)?,
            finished_at: row.get(12)?,
        })
    }

    fn map_item(row: &Row<'_>) -> rusqlite::Result<JobItemRow> {
        Ok(JobItemRow {
            id: row.get(0)?,
            job_id: row.get(1)?,
            item_index: row.get(2)?,
            state: row.get(3)?,
            payload_json: row.get(4)?,
            result_json: row.get(5)?,
            error: row.get(6)?,
            attempts: row.get(7)?,
            idempotency_key: row.get(8)?,
            started_at: row.get(9)?,
            finished_at: row.get(10)?,
        })
    }

    pub fn insert(connection: &Connection, job: &NewJob) -> Result<JobRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO jobs (id, kind, state, progress, payload_json, attempts, priority,
                               dedupe_key, note, created_at)
             VALUES (?1, ?2, ?3, 0, ?4, 0, ?5, ?6, ?7, ?8)",
            params![
                job.id,
                job.kind,
                job.state,
                job.payload_json,
                job.priority,
                job.dedupe_key,
                job.note,
                created_at,
            ],
        )?;
        get(connection, &job.id)?.ok_or_else(|| CommandError::internal("job insert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<JobRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM jobs WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    /// Most recently created jobs first, for the history view.
    pub fn list(connection: &Connection, limit: i64) -> Result<Vec<JobRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM jobs ORDER BY created_at DESC LIMIT ?1");
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([limit], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Active jobs in execution order: priority first, then oldest first.
    pub fn list_active(connection: &Connection) -> Result<Vec<JobRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM jobs
             WHERE state IN ('queued', 'running', 'waiting', 'interrupted')
             ORDER BY priority DESC, created_at ASC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn count_queued(connection: &Connection) -> Result<i64, CommandError> {
        Ok(connection.query_row(
            "SELECT COUNT(*) FROM jobs WHERE state = 'queued'",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn find_active_by_dedupe(
        connection: &Connection,
        key: &str,
    ) -> Result<Option<JobRow>, CommandError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM jobs
             WHERE dedupe_key = ?1 AND state IN ('queued', 'waiting', 'running', 'interrupted')
             ORDER BY created_at ASC LIMIT 1"
        );
        Ok(connection.query_row(&sql, [key], map).optional()?)
    }

    /// Claims the highest-priority queued job and moves it to running.
    /// The check and transition happen in one transaction so two workers cannot
    /// claim the same job.
    pub fn claim_next(connection: &Connection) -> Result<Option<JobRow>, CommandError> {
        let transaction = connection.unchecked_transaction()?;
        let now = now_iso(&transaction)?;
        let candidate: Option<String> = transaction
            .query_row(
                "SELECT id FROM jobs WHERE state = 'queued'
                 ORDER BY priority DESC, created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let Some(id) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };

        let changed = transaction.execute(
            "UPDATE jobs SET state = 'running', attempts = attempts + 1,
                 started_at = COALESCE(started_at, ?1), finished_at = NULL
             WHERE id = ?2 AND state = 'queued'",
            params![now, id],
        )?;
        transaction.commit()?;

        if changed == 0 {
            return Ok(None);
        }
        get(connection, &id)
    }

    pub fn update_state(
        connection: &Connection,
        id: &str,
        state: &str,
        progress: f64,
        error: Option<&str>,
    ) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        let changed = connection.execute(
            "UPDATE jobs SET state = ?1, progress = ?2, error = ?3,
                 started_at = COALESCE(started_at, ?4),
                 finished_at = CASE WHEN ?1 IN ('succeeded', 'failed', 'cancelled') THEN ?4 ELSE finished_at END
             WHERE id = ?5",
            params![state, progress, error, now, id],
        )?;
        Ok(changed > 0)
    }

    pub fn set_priority(
        connection: &Connection,
        id: &str,
        priority: i64,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE jobs SET priority = ?1 WHERE id = ?2 AND state IN ('queued', 'waiting', 'interrupted')",
            params![priority, id],
        )? > 0)
    }

    /// Marks jobs and items left running by an unclean exit as interrupted.
    /// Returns the number of jobs affected.
    pub fn mark_interrupted(connection: &Connection) -> Result<usize, CommandError> {
        let now = now_iso(connection)?;
        let transaction = connection.unchecked_transaction()?;
        let jobs = transaction.execute(
            "UPDATE jobs SET state = 'interrupted', finished_at = NULL
             WHERE state = 'running'",
            [],
        )?;
        transaction.execute(
            "UPDATE job_items SET state = 'interrupted', finished_at = ?1
             WHERE state = 'running'",
            params![now],
        )?;
        transaction.commit()?;
        Ok(jobs)
    }

    /// Moves an interrupted or failed job back to queued. Succeeded items are
    /// preserved so resume only reprocesses incomplete work.
    pub fn requeue(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let changed = connection.execute(
            "UPDATE jobs SET state = 'queued', error = NULL, finished_at = NULL,
                 dedupe_key = NULL
             WHERE id = ?1 AND state IN ('interrupted', 'failed', 'cancelled')",
            [id],
        )?;
        connection.execute(
            "UPDATE job_items SET state = 'queued', error = NULL, started_at = NULL, finished_at = NULL
             WHERE job_id = ?1 AND state IN ('failed', 'interrupted', 'cancelled')",
            [id],
        )?;
        Ok(changed > 0)
    }

    pub fn cancel_job(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE job_items SET state = 'cancelled', finished_at = ?1
             WHERE job_id = ?2 AND state IN ('queued', 'waiting', 'interrupted')",
            params![now, id],
        )?;
        let changed = transaction.execute(
            "UPDATE jobs SET state = 'cancelled', finished_at = ?1
             WHERE id = ?2 AND state IN ('queued', 'waiting', 'interrupted')",
            params![now, id],
        )?;
        transaction.commit()?;
        Ok(changed > 0)
    }

    pub fn cancel_waiting(connection: &Connection) -> Result<usize, CommandError> {
        let now = now_iso(connection)?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE job_items SET state = 'cancelled', finished_at = ?1
             WHERE state IN ('queued', 'waiting')
               AND job_id IN (SELECT id FROM jobs WHERE state IN ('queued', 'waiting'))",
            params![now],
        )?;
        let changed = transaction.execute(
            "UPDATE jobs SET state = 'cancelled', finished_at = ?1
             WHERE state IN ('queued', 'waiting')",
            params![now],
        )?;
        transaction.commit()?;
        Ok(changed)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM jobs WHERE id = ?1", [id])? > 0)
    }

    pub fn insert_item(
        connection: &Connection,
        item: &NewJobItem,
    ) -> Result<JobItemRow, CommandError> {
        connection.execute(
            "INSERT INTO job_items (id, job_id, item_index, state, payload_json, attempts, idempotency_key)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                item.id,
                item.job_id,
                item.item_index,
                item.state,
                item.payload_json,
                item.idempotency_key,
            ],
        )?;
        get_item(connection, &item.id)?
            .ok_or_else(|| CommandError::internal("job item insert vanished"))
    }

    pub fn get_item(connection: &Connection, id: &str) -> Result<Option<JobItemRow>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT id, job_id, item_index, state, payload_json, result_json, error, attempts,
                 idempotency_key, started_at, finished_at
             FROM job_items WHERE id = ?1",
        )?;
        Ok(statement.query_row([id], map_item).optional()?)
    }

    pub fn list_items(
        connection: &Connection,
        job_id: &str,
    ) -> Result<Vec<JobItemRow>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT id, job_id, item_index, state, payload_json, result_json, error, attempts,
                 idempotency_key, started_at, finished_at
             FROM job_items WHERE job_id = ?1 ORDER BY item_index",
        )?;
        let rows = statement.query_map([job_id], map_item)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Next unfinished item in index order. Succeeded items are skipped by the
    /// query itself, which makes resume idempotent.
    pub fn next_item(
        connection: &Connection,
        job_id: &str,
    ) -> Result<Option<JobItemRow>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT id, job_id, item_index, state, payload_json, result_json, error, attempts,
                 idempotency_key, started_at, finished_at
             FROM job_items
             WHERE job_id = ?1 AND state NOT IN ('succeeded', 'cancelled', 'failed')
             ORDER BY item_index LIMIT 1",
        )?;
        Ok(statement.query_row([job_id], map_item).optional()?)
    }

    pub fn claim_item(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE job_items SET state = 'running', attempts = attempts + 1,
                 started_at = COALESCE(started_at, ?1), finished_at = NULL, error = NULL
             WHERE id = ?2 AND state IN ('queued', 'waiting', 'failed', 'interrupted')",
            params![now, id],
        )? > 0)
    }

    /// Increments the attempt counter for a retry while keeping the item in the
    /// running state.
    pub fn retry_item(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE job_items SET attempts = attempts + 1, error = NULL,
                 started_at = COALESCE(started_at, ?1)
             WHERE id = ?2 AND state = 'running'",
            params![now, id],
        )? > 0)
    }

    pub fn complete_item(
        connection: &Connection,
        id: &str,
        result_json: &str,
    ) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE job_items SET state = 'succeeded', result_json = ?1, error = NULL,
                 finished_at = ?2
             WHERE id = ?3",
            params![result_json, now, id],
        )? > 0)
    }

    pub fn fail_item(connection: &Connection, id: &str, error: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE job_items SET state = 'failed', error = ?1, finished_at = ?2 WHERE id = ?3",
            params![error, now, id],
        )? > 0)
    }

    pub fn cancel_item(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        let now = now_iso(connection)?;
        Ok(connection.execute(
            "UPDATE job_items SET state = 'cancelled', finished_at = ?1
             WHERE id = ?2 AND state NOT IN ('succeeded', 'failed')",
            params![now, id],
        )? > 0)
    }

    pub fn item_counts(connection: &Connection, job_id: &str) -> Result<ItemCounts, CommandError> {
        let mut counts = ItemCounts::default();
        let mut statement = connection
            .prepare("SELECT state, COUNT(*) FROM job_items WHERE job_id = ?1 GROUP BY state")?;
        let rows = statement.query_map([job_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (state, count) = row?;
            counts.total += count;
            match state.as_str() {
                "queued" => counts.queued = count,
                "running" => counts.running = count,
                "waiting" => counts.waiting = count,
                "succeeded" => counts.succeeded = count,
                "failed" => counts.failed = count,
                "cancelled" => counts.cancelled = count,
                "interrupted" => counts.interrupted = count,
                _ => {}
            }
        }
        Ok(counts)
    }

    pub fn update_progress(
        connection: &Connection,
        id: &str,
        progress: f64,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE jobs SET progress = ?1 WHERE id = ?2",
            params![progress, id],
        )? > 0)
    }
}

pub mod datasets {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct DatasetRow {
        pub id: String,
        pub name: String,
        pub source_path: String,
        pub mapping_json: Option<String>,
        pub export_columns_json: Option<String>,
        pub row_count: i64,
        pub created_at: String,
    }

    #[derive(Debug, Clone)]
    pub struct NewDataset {
        pub id: String,
        pub name: String,
        pub source_path: String,
        pub mapping_json: Option<String>,
        pub export_columns_json: Option<String>,
        pub row_count: i64,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct DatasetItemRow {
        pub id: String,
        pub dataset_id: String,
        pub row_index: i64,
        pub data_json: String,
        pub status: String,
        pub generation_id: Option<String>,
        pub error: Option<String>,
        pub media_asset_ids: Vec<String>,
    }

    #[derive(Debug, Clone)]
    pub struct NewDatasetItem {
        pub id: String,
        pub dataset_id: String,
        pub row_index: i64,
        pub data_json: String,
        pub status: String,
    }

    const COLUMNS: &str =
        "id, name, source_path, mapping_json, export_columns_json, row_count, created_at";

    fn parse_ids(raw: String) -> Vec<String> {
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn map(row: &Row<'_>) -> rusqlite::Result<DatasetRow> {
        Ok(DatasetRow {
            id: row.get(0)?,
            name: row.get(1)?,
            source_path: row.get(2)?,
            mapping_json: row.get(3)?,
            export_columns_json: row.get(4)?,
            row_count: row.get(5)?,
            created_at: row.get(6)?,
        })
    }

    pub fn insert(
        connection: &Connection,
        dataset: &NewDataset,
    ) -> Result<DatasetRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO datasets (id, name, source_path, mapping_json, export_columns_json, row_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                dataset.id,
                dataset.name,
                dataset.source_path,
                dataset.mapping_json,
                dataset.export_columns_json,
                dataset.row_count,
                created_at,
            ],
        )?;
        get(connection, &dataset.id)?
            .ok_or_else(|| CommandError::internal("dataset insert vanished"))
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<DatasetRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM datasets WHERE id = ?1");
        Ok(connection.query_row(&sql, [id], map).optional()?)
    }

    pub fn list(connection: &Connection) -> Result<Vec<DatasetRow>, CommandError> {
        let sql = format!("SELECT {COLUMNS} FROM datasets ORDER BY created_at DESC");
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM datasets WHERE id = ?1", [id])? > 0)
    }

    pub fn insert_item(
        connection: &Connection,
        item: &NewDatasetItem,
    ) -> Result<DatasetItemRow, CommandError> {
        connection.execute(
            "INSERT INTO dataset_rows (id, dataset_id, row_index, data_json, status)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                item.id,
                item.dataset_id,
                item.row_index,
                item.data_json,
                item.status,
            ],
        )?;
        get_item(connection, &item.id)?
            .ok_or_else(|| CommandError::internal("dataset row insert vanished"))
    }

    pub fn get_item(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<DatasetItemRow>, CommandError> {
        Ok(connection
            .query_row(
                "SELECT id, dataset_id, row_index, data_json, status, generation_id, error,
                        COALESCE(media_asset_ids_json, '[]')
                 FROM dataset_rows WHERE id = ?1",
                [id],
                |row| {
                    Ok(DatasetItemRow {
                        id: row.get(0)?,
                        dataset_id: row.get(1)?,
                        row_index: row.get(2)?,
                        data_json: row.get(3)?,
                        status: row.get(4)?,
                        generation_id: row.get(5)?,
                        error: row.get(6)?,
                        media_asset_ids: parse_ids(row.get::<_, String>(7)?),
                    })
                },
            )
            .optional()?)
    }

    pub fn list_items(
        connection: &Connection,
        dataset_id: &str,
    ) -> Result<Vec<DatasetItemRow>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT id, dataset_id, row_index, data_json, status, generation_id, error,
                    COALESCE(media_asset_ids_json, '[]')
             FROM dataset_rows WHERE dataset_id = ?1 ORDER BY row_index",
        )?;
        let rows = statement.query_map([dataset_id], |row| {
            Ok(DatasetItemRow {
                id: row.get(0)?,
                dataset_id: row.get(1)?,
                row_index: row.get(2)?,
                data_json: row.get(3)?,
                status: row.get(4)?,
                generation_id: row.get(5)?,
                error: row.get(6)?,
                media_asset_ids: parse_ids(row.get::<_, String>(7)?),
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_item_media_assets(
        connection: &Connection,
        id: &str,
        asset_ids: &[String],
    ) -> Result<bool, CommandError> {
        let json = serde_json::to_string(asset_ids)
            .map_err(|error| CommandError::internal(format!("serialize media ids: {error}")))?;
        Ok(connection.execute(
            "UPDATE dataset_rows SET media_asset_ids_json = ?1 WHERE id = ?2",
            params![json, id],
        )? > 0)
    }

    pub fn update_item_status(
        connection: &Connection,
        id: &str,
        status: &str,
        generation_id: Option<&str>,
        error: Option<&str>,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE dataset_rows SET status = ?1, generation_id = COALESCE(?2, generation_id),
                 error = ?3
             WHERE id = ?4",
            params![status, generation_id, error, id],
        )? > 0)
    }

    pub fn queue_items(connection: &Connection, dataset_id: &str) -> Result<usize, CommandError> {
        Ok(connection.execute(
            "UPDATE dataset_rows SET status = 'queued', error = NULL
             WHERE dataset_id = ?1 AND status IN ('pending', 'failed')",
            [dataset_id],
        )?)
    }

    pub fn status_counts(
        connection: &Connection,
        dataset_id: &str,
    ) -> Result<Vec<(String, i64)>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT status, COUNT(*) FROM dataset_rows WHERE dataset_id = ?1 GROUP BY status",
        )?;
        let rows = statement.query_map([dataset_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_export_columns(
        connection: &Connection,
        id: &str,
        columns_json: &str,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE datasets SET export_columns_json = ?1 WHERE id = ?2",
            params![columns_json, id],
        )? > 0)
    }

    pub fn set_row_count(
        connection: &Connection,
        id: &str,
        row_count: i64,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE datasets SET row_count = ?1 WHERE id = ?2",
            params![row_count, id],
        )? > 0)
    }

    pub fn update_item_data(
        connection: &Connection,
        id: &str,
        data_json: &str,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE dataset_rows SET data_json = ?1 WHERE id = ?2",
            params![data_json, id],
        )? > 0)
    }
}

pub mod import_profiles {
    use rusqlite::{Connection, OptionalExtension, Row, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct ImportProfileRow {
        pub id: String,
        pub name: String,
        pub mapping_json: String,
        pub created_at: String,
    }

    fn map(row: &Row<'_>) -> rusqlite::Result<ImportProfileRow> {
        Ok(ImportProfileRow {
            id: row.get(0)?,
            name: row.get(1)?,
            mapping_json: row.get(2)?,
            created_at: row.get(3)?,
        })
    }

    pub fn upsert(
        connection: &Connection,
        id: &str,
        name: &str,
        mapping_json: &str,
    ) -> Result<ImportProfileRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO import_profiles (id, name, mapping_json, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (name) DO UPDATE SET mapping_json = excluded.mapping_json",
            params![id, name, mapping_json, created_at],
        )?;
        get_by_name(connection, name)?
            .ok_or_else(|| CommandError::internal("import profile upsert vanished"))
    }

    pub fn get_by_name(
        connection: &Connection,
        name: &str,
    ) -> Result<Option<ImportProfileRow>, CommandError> {
        Ok(connection
            .query_row(
                "SELECT id, name, mapping_json, created_at FROM import_profiles WHERE name = ?1",
                [name],
                map,
            )
            .optional()?)
    }

    pub fn list(connection: &Connection) -> Result<Vec<ImportProfileRow>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT id, name, mapping_json, created_at FROM import_profiles ORDER BY name",
        )?;
        let rows = statement.query_map([], map)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(connection: &Connection, name: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM import_profiles WHERE name = ?1", [name])? > 0)
    }
}

pub mod titles {
    use rusqlite::{Connection, OptionalExtension, params};

    use crate::commands::CommandError;

    pub fn get(connection: &Connection, id: &str) -> Result<Option<String>, CommandError> {
        Ok(connection
            .query_row(
                "SELECT title FROM generation_titles WHERE generation_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set(
        connection: &Connection,
        id: &str,
        title: &str,
        source: &str,
    ) -> Result<(), CommandError> {
        connection.execute(
            "INSERT INTO generation_titles (generation_id, title, source) VALUES (?1, ?2, ?3)
             ON CONFLICT(generation_id) DO UPDATE SET title = excluded.title, source = excluded.source",
            params![id, title, source],
        )?;
        Ok(())
    }

    /// Generations that have a prompt but no title yet, newest first.
    pub fn missing(connection: &Connection, limit: i64) -> Result<Vec<(String, String)>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT g.id, g.prompt FROM generations g
             LEFT JOIN generation_titles t ON t.generation_id = g.id
             WHERE t.generation_id IS NULL AND length(trim(g.prompt)) > 0
             ORDER BY g.created_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub mod tags {
    use crate::commands::CommandError;
    use rusqlite::{Connection, params};

    pub fn ensure(connection: &Connection, id: &str, name: &str) -> Result<String, CommandError> {
        connection.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2) ON CONFLICT (name) DO NOTHING",
            params![id, name],
        )?;
        Ok(
            connection.query_row("SELECT id FROM tags WHERE name = ?1", [name], |row| {
                row.get(0)
            })?,
        )
    }

    pub fn attach(
        connection: &Connection,
        generation_id: &str,
        tag_id: &str,
    ) -> Result<(), CommandError> {
        connection.execute(
            "INSERT INTO generation_tags (generation_id, tag_id) VALUES (?1, ?2)
             ON CONFLICT DO NOTHING",
            params![generation_id, tag_id],
        )?;
        Ok(())
    }

    pub fn detach(
        connection: &Connection,
        generation_id: &str,
        tag_id: &str,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "DELETE FROM generation_tags WHERE generation_id = ?1 AND tag_id = ?2",
            params![generation_id, tag_id],
        )? > 0)
    }

    pub fn list_for_generation(
        connection: &Connection,
        generation_id: &str,
    ) -> Result<Vec<String>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT tags.name FROM tags
             JOIN generation_tags ON generation_tags.tag_id = tags.id
             WHERE generation_tags.generation_id = ?1
             ORDER BY tags.name",
        )?;
        let rows = statement.query_map([generation_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(connection: &Connection, name: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM tags WHERE name = ?1", [name])? > 0)
    }

    pub fn list_counts(connection: &Connection) -> Result<Vec<(String, i64)>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT tags.name, COUNT(generation_tags.generation_id)
             FROM tags
             LEFT JOIN generation_tags ON generation_tags.tag_id = tags.id
             GROUP BY tags.id
             ORDER BY tags.name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub mod collections {
    use rusqlite::{Connection, params};

    use crate::commands::CommandError;

    use super::super::now_iso;

    #[derive(Debug, Clone, PartialEq)]
    pub struct CollectionRow {
        pub id: String,
        pub name: String,
        pub created_at: String,
    }

    pub fn create(
        connection: &Connection,
        id: &str,
        name: &str,
    ) -> Result<CollectionRow, CommandError> {
        let created_at = now_iso(connection)?;
        connection.execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, created_at],
        )?;
        Ok(CollectionRow {
            id: id.to_string(),
            name: name.to_string(),
            created_at,
        })
    }

    pub fn list(connection: &Connection) -> Result<Vec<CollectionRow>, CommandError> {
        let mut statement =
            connection.prepare("SELECT id, name, created_at FROM collections ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(CollectionRow {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn rename(connection: &Connection, id: &str, name: &str) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "UPDATE collections SET name = ?1 WHERE id = ?2",
            params![name, id],
        )? > 0)
    }

    pub fn add_item(
        connection: &Connection,
        collection_id: &str,
        generation_id: &str,
    ) -> Result<(), CommandError> {
        connection.execute(
            "INSERT INTO collection_items (collection_id, generation_id) VALUES (?1, ?2)
             ON CONFLICT DO NOTHING",
            params![collection_id, generation_id],
        )?;
        Ok(())
    }

    pub fn remove_item(
        connection: &Connection,
        collection_id: &str,
        generation_id: &str,
    ) -> Result<bool, CommandError> {
        Ok(connection.execute(
            "DELETE FROM collection_items WHERE collection_id = ?1 AND generation_id = ?2",
            params![collection_id, generation_id],
        )? > 0)
    }

    pub fn list_items(
        connection: &Connection,
        collection_id: &str,
    ) -> Result<Vec<String>, CommandError> {
        let mut statement = connection
            .prepare("SELECT generation_id FROM collection_items WHERE collection_id = ?1")?;
        let rows = statement.query_map([collection_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM collections WHERE id = ?1", [id])? > 0)
    }

    pub fn get(connection: &Connection, id: &str) -> Result<Option<CollectionRow>, CommandError> {
        use rusqlite::OptionalExtension;
        Ok(connection
            .query_row(
                "SELECT id, name, created_at FROM collections WHERE id = ?1",
                [id],
                |row| {
                    Ok(CollectionRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        created_at: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn counts(connection: &Connection) -> Result<Vec<(String, i64)>, CommandError> {
        let mut statement = connection.prepare(
            "SELECT collections.id, COUNT(collection_items.generation_id)
             FROM collections
             LEFT JOIN collection_items ON collection_items.collection_id = collections.id
             GROUP BY collections.id
             ORDER BY collections.name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub mod settings {
    use rusqlite::{Connection, OptionalExtension, params};

    use crate::commands::CommandError;

    pub fn set(connection: &Connection, key: &str, value_json: &str) -> Result<(), CommandError> {
        connection.execute(
            "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json",
            params![key, value_json],
        )?;
        Ok(())
    }

    pub fn get(connection: &Connection, key: &str) -> Result<Option<String>, CommandError> {
        Ok(connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn delete(connection: &Connection, key: &str) -> Result<bool, CommandError> {
        Ok(connection.execute("DELETE FROM settings WHERE key = ?1", [key])? > 0)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::{
        assets, collections, datasets, descriptions, edits, generations, jobs, presets, settings,
        tags,
    };
    use crate::db::Database;

    fn db() -> Database {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");
        database
    }

    #[allow(clippy::too_many_arguments)]
    fn seed_generation(
        connection: &rusqlite::Connection,
        asset_id: &str,
        id: &str,
        prompt: &str,
        negative: Option<&str>,
        description: Option<&str>,
        guidance: &str,
    ) -> String {
        generations::insert(
            connection,
            &generations::NewGeneration {
                id: id.into(),
                parent_id: None,
                asset_id: Some(asset_id.into()),
                edit_id: None,
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: "mmh3".into(),
                preset_version: "1.0.0".into(),
                preset_hash: "preset-hash".into(),
                prompt: prompt.into(),
                negative: negative.map(ToOwned::to_owned),
                params_json: "{}".into(),
                guidance_json: guidance.into(),
                description_json: description.map(ToOwned::to_owned),
                seed: Some(42),
                models_json: "[]".into(),
                status: "succeeded".into(),
                error: None,
                raw_response: None,
                tokens_in: None,
                tokens_out: None,
                latency_ms: None,
                cache_key: None,
            },
        )
        .expect("seed generation");
        id.to_string()
    }

    fn seed_asset(connection: &rusqlite::Connection) -> String {
        let asset = assets::insert(
            connection,
            &assets::NewAsset {
                id: "asset-1".into(),
                hash: "hash-1".into(),
                kind: "image".into(),
                source_path: "C:\\media\\a.png".into(),
                display_name: "a.png".into(),
                width: Some(1024),
                height: Some(1024),
                fps: None,
                duration_ms: None,
                codec: Some("png".into()),
                has_audio: false,
                color: Some("srgb".into()),
                rotation: 0,
                thumbnail_path: None,
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("insert asset");
        asset.id
    }

    #[test]
    fn trashed_assets_leave_the_normal_list_but_stay_restorable() {
        let database = db();
        let connection = database.connection();
        let id = seed_asset(connection);

        assert!(assets::soft_delete(connection, &id).expect("soft delete"));
        assert!(assets::list(connection, 10, 0).expect("list").is_empty());
        assert_eq!(assets::list_deleted(connection).expect("deleted").len(), 1);
        assert!(assets::get(connection, &id).expect("get").is_some());

        assert!(assets::restore(connection, &id).expect("restore"));
        assert_eq!(assets::list(connection, 10, 0).expect("list").len(), 1);
        assert!(
            assets::list_deleted(connection)
                .expect("deleted")
                .is_empty()
        );
    }

    #[test]
    fn assets_create_update_list_delete() {
        let database = db();
        let connection = database.connection();
        let id = seed_asset(connection);

        let fetched = assets::get(connection, &id).expect("get").expect("present");
        assert_eq!(fetched.width, Some(1024));

        assert!(assets::set_favorite(connection, &id, true).expect("favorite"));
        assert!(
            assets::get(connection, &id)
                .expect("get")
                .expect("present")
                .favorite
        );

        assert_eq!(assets::list(connection, 10, 0).expect("list").len(), 1);
        assert!(assets::delete(connection, &id).expect("delete"));
        assert!(assets::get(connection, &id).expect("get").is_none());
    }

    #[test]
    fn refresh_missing_marks_offline_assets() {
        let unique = format!(
            "prompt-studio-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&directory).expect("create dir");
        let source = directory.join("source.png");
        std::fs::write(&source, b"x").expect("write source");

        let database = db();
        let connection = database.connection();
        assets::insert(
            connection,
            &assets::NewAsset {
                id: "asset-missing".into(),
                hash: "hash-missing".into(),
                kind: "image".into(),
                source_path: source.display().to_string(),
                display_name: "source.png".into(),
                width: Some(10),
                height: Some(10),
                fps: None,
                duration_ms: None,
                codec: Some("png".into()),
                has_audio: false,
                color: None,
                rotation: 0,
                thumbnail_path: Some("C:\\cache\\thumb.jpg".into()),
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("insert");

        assert_eq!(assets::refresh_missing(connection).expect("refresh"), 0);
        std::fs::remove_file(&source).expect("remove source");
        assert_eq!(assets::refresh_missing(connection).expect("refresh"), 1);

        let asset = assets::get(connection, "asset-missing")
            .expect("get")
            .expect("present");
        assert!(asset.missing);
        assert_eq!(
            asset.thumbnail_path.as_deref(),
            Some("C:\\cache\\thumb.jpg")
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn edits_create_lookup_delete() {
        let database = db();
        let connection = database.connection();
        let asset_id = seed_asset(connection);

        let edit = edits::insert(
            connection,
            &edits::NewEdit {
                id: "edit-1".into(),
                asset_id,
                ops_json: "{}".into(),
                target_json: "{}".into(),
                output_path: "C:\\media\\edited\\a.png".into(),
                cache_key: "edit-key".into(),
                width: Some(1008),
                height: Some(560),
                fps: None,
                duration_ms: None,
            },
        )
        .expect("insert edit");
        assert_eq!(edit.width, Some(1008));
        assert_eq!(
            edits::find_by_cache_key(connection, "edit-key")
                .expect("lookup")
                .expect("present")
                .id,
            "edit-1"
        );
        assert!(edits::delete(connection, "edit-1").expect("delete"));
    }

    #[test]
    fn latest_edit_per_asset_returns_the_newest_only() {
        let database = db();
        let connection = database.connection();
        let asset_id = seed_asset(connection);
        for (index, id) in ["edit-old", "edit-new"].into_iter().enumerate() {
            edits::insert(
                connection,
                &edits::NewEdit {
                    id: id.into(),
                    asset_id: asset_id.clone(),
                    ops_json: format!("{{\"step\":{index}}}"),
                    target_json: "{}".into(),
                    output_path: format!("C:\\media\\edited\\{id}.png"),
                    cache_key: id.into(),
                    width: Some(640),
                    height: Some(480),
                    fps: None,
                    duration_ms: None,
                },
            )
            .expect("insert edit");
        }

        let latest = edits::latest_for_asset(connection, &asset_id)
            .expect("latest")
            .expect("present");
        assert_eq!(latest.id, "edit-new");

        let by_asset =
            edits::latest_by_assets(connection, &[asset_id, "missing-asset".into()]).expect("map");
        assert_eq!(by_asset.len(), 1);
        assert_eq!(by_asset[0].id, "edit-new");
    }

    #[test]
    fn descriptions_create_lookup_delete() {
        let database = db();
        let connection = database.connection();
        let asset_id = seed_asset(connection);

        descriptions::insert(
            connection,
            &descriptions::NewDescription {
                id: "desc-1".into(),
                asset_id: Some(asset_id),
                edit_id: None,
                model: "lfm3-vl-2b".into(),
                prompt_version: "1".into(),
                sampling_json: "{}".into(),
                cache_key: "desc-key".into(),
                json: "{\"subject\":\"cat\"}".into(),
                confidence: Some(0.9),
                cached: false,
            },
        )
        .expect("insert description");

        let found = descriptions::find_by_cache_key(connection, "desc-key")
            .expect("lookup")
            .expect("present");
        assert_eq!(found.confidence, Some(0.9));
        assert!(descriptions::delete(connection, "desc-1").expect("delete"));
    }

    #[test]
    fn presets_upsert_list_delete() {
        let database = db();
        let connection = database.connection();

        let preset = presets::upsert(
            connection,
            &presets::NewPreset {
                id: "mmh3".into(),
                owner: "builtin".into(),
                name: "mmh3".into(),
                kind: "video".into(),
                version: "1.0.0".into(),
                description: "video target".into(),
                document_json: "{}".into(),
                content_hash: "hash-a".into(),
            },
        )
        .expect("upsert");
        assert_eq!(preset.name, "mmh3");

        let updated = presets::upsert(
            connection,
            &presets::NewPreset {
                id: "mmh3".into(),
                owner: "builtin".into(),
                name: "mmh3 v2".into(),
                kind: "video".into(),
                version: "1.0.1".into(),
                description: "video target".into(),
                document_json: "{}".into(),
                content_hash: "hash-b".into(),
            },
        )
        .expect("update");
        assert_eq!(updated.name, "mmh3 v2");
        assert_eq!(updated.created_at, preset.created_at);

        assert_eq!(presets::list(connection, false).expect("list").len(), 1);
        assert!(presets::delete(connection, "mmh3").expect("delete"));
        assert!(presets::list(connection, false).expect("list").is_empty());
        assert_eq!(presets::list(connection, true).expect("list all").len(), 1);
    }

    #[test]
    fn generations_create_favorite_soft_delete_restore() {
        let database = db();
        let connection = database.connection();
        let asset_id = seed_asset(connection);

        generations::insert(
            connection,
            &generations::NewGeneration {
                id: "gen-1".into(),
                parent_id: None,
                asset_id: Some(asset_id),
                edit_id: None,
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: "mmh3".into(),
                preset_version: "1.0.0".into(),
                preset_hash: "preset-hash".into(),
                prompt: "a prompt".into(),
                negative: None,
                params_json: "{}".into(),
                guidance_json: "{}".into(),
                description_json: None,
                seed: Some(42),
                models_json: "[]".into(),
                status: "succeeded".into(),
                error: None,
                raw_response: None,
                tokens_in: None,
                tokens_out: None,
                latency_ms: Some(1200),
                cache_key: Some("gen-key".into()),
            },
        )
        .expect("insert generation");

        assert!(generations::set_favorite(connection, "gen-1", true).expect("favorite"));
        assert!(
            generations::get(connection, "gen-1")
                .expect("get")
                .expect("present")
                .favorite
        );

        assert_eq!(
            generations::list(connection, 10, 0, false)
                .expect("list")
                .len(),
            1
        );
        assert!(generations::soft_delete(connection, "gen-1").expect("soft delete"));
        assert!(
            generations::list(connection, 10, 0, false)
                .expect("list")
                .is_empty()
        );
        assert!(generations::restore(connection, "gen-1").expect("restore"));
        assert!(generations::hard_delete(connection, "gen-1").expect("hard delete"));
    }

    #[test]
    fn jobs_create_update_items_delete() {
        let database = db();
        let connection = database.connection();

        jobs::insert(
            connection,
            &jobs::NewJob {
                id: "job-1".into(),
                kind: "batch".into(),
                state: "queued".into(),
                payload_json: "{}".into(),
                priority: 0,
                dedupe_key: None,
                note: None,
            },
        )
        .expect("insert job");

        jobs::insert_item(
            connection,
            &jobs::NewJobItem {
                id: "item-1".into(),
                job_id: "job-1".into(),
                item_index: 0,
                state: "queued".into(),
                payload_json: "{}".into(),
                idempotency_key: Some("item-key".into()),
            },
        )
        .expect("insert item");

        assert_eq!(
            jobs::list_items(connection, "job-1").expect("items").len(),
            1
        );
        assert!(jobs::update_state(connection, "job-1", "running", 0.5, None).expect("update"));
        assert_eq!(
            jobs::get(connection, "job-1")
                .expect("get")
                .expect("present")
                .state,
            "running"
        );
        assert!(jobs::delete(connection, "job-1").expect("delete"));
        assert!(
            jobs::list_items(connection, "job-1")
                .expect("items")
                .is_empty()
        );
    }

    #[test]
    #[ignore = "performance measurement; run with --ignored"]
    fn measures_first_page_for_ten_thousand_generations() {
        let database = db();
        let connection = database.connection();
        let seed_started = std::time::Instant::now();
        connection.execute_batch("BEGIN").expect("begin");
        for index in 0..10_000_i64 {
            generations::insert(
                connection,
                &generations::NewGeneration {
                    id: format!("gen-{index:05}"),
                    parent_id: None,
                    asset_id: None,
                    edit_id: None,
                    description_id: None,
                    project_id: None,
                    shot_index: None,
                    preset_id: "mmh3".into(),
                    preset_version: "1.0.0".into(),
                    preset_hash: "hash".into(),
                    prompt: format!("prompt {index} a red fox in snow"),
                    negative: None,
                    params_json: "{}".into(),
                    guidance_json: "{}".into(),
                    description_json: None,
                    seed: Some(index),
                    models_json: "[\"gemma\"]".into(),
                    status: "succeeded".into(),
                    error: None,
                    raw_response: None,
                    tokens_in: Some(10),
                    tokens_out: Some(5),
                    latency_ms: Some(100),
                    cache_key: None,
                },
            )
            .expect("seed generation");
        }
        connection.execute_batch("COMMIT").expect("commit");
        let seed_ms = seed_started.elapsed().as_millis();

        let filter = generations::GenerationFilter {
            sort: "newest".into(),
            limit: 25,
            offset: 0,
            ..Default::default()
        };
        let query_started = std::time::Instant::now();
        let (rows, total) = generations::query(connection, &filter).expect("query");
        let first_page_ms = query_started.elapsed().as_millis();

        eprintln!(
            "seeded 10000 generations in {seed_ms} ms; first page in {first_page_ms} ms; total {total}"
        );
        assert_eq!(rows.len(), 25);
        assert_eq!(total, 10_000);
        assert!(
            first_page_ms < 300,
            "first page took {first_page_ms} ms, expected under 300 ms"
        );
    }

    #[test]
    fn datasets_create_rows_list_delete() {
        let database = db();
        let connection = database.connection();

        datasets::insert(
            connection,
            &datasets::NewDataset {
                id: "ds-1".into(),
                name: "prompts.csv".into(),
                source_path: "C:\\data\\prompts.csv".into(),
                mapping_json: None,
                export_columns_json: None,
                row_count: 1,
            },
        )
        .expect("insert dataset");

        datasets::insert_item(
            connection,
            &datasets::NewDatasetItem {
                id: "row-1".into(),
                dataset_id: "ds-1".into(),
                row_index: 0,
                data_json: "{\"prompt\":\"x\"}".into(),
                status: "pending".into(),
            },
        )
        .expect("insert row");

        assert_eq!(datasets::list(connection).expect("list").len(), 1);
        assert_eq!(
            datasets::list_items(connection, "ds-1")
                .expect("rows")
                .len(),
            1
        );
        assert!(datasets::delete(connection, "ds-1").expect("delete"));
        assert!(
            datasets::list_items(connection, "ds-1")
                .expect("rows")
                .is_empty()
        );
    }

    #[test]
    fn dataset_rows_store_media_asset_ids() {
        let database = db();
        let connection = database.connection();

        datasets::insert(
            connection,
            &datasets::NewDataset {
                id: "ds-media".into(),
                name: "media.csv".into(),
                source_path: "C:\\data\\media.csv".into(),
                mapping_json: None,
                export_columns_json: None,
                row_count: 1,
            },
        )
        .expect("insert dataset");
        datasets::insert_item(
            connection,
            &datasets::NewDatasetItem {
                id: "row-media".into(),
                dataset_id: "ds-media".into(),
                row_index: 0,
                data_json: "{}".into(),
                status: "pending".into(),
            },
        )
        .expect("insert row");

        let row = datasets::get_item(connection, "row-media")
            .expect("get")
            .expect("present");
        assert!(row.media_asset_ids.is_empty());

        datasets::set_item_media_assets(
            connection,
            "row-media",
            &["asset-1".to_string(), "asset-2".to_string()],
        )
        .expect("set ids");
        let row = datasets::get_item(connection, "row-media")
            .expect("get")
            .expect("present");
        assert_eq!(row.media_asset_ids, vec!["asset-1", "asset-2"]);

        let listed = datasets::list_items(connection, "ds-media").expect("list");
        assert_eq!(listed[0].media_asset_ids, vec!["asset-1", "asset-2"]);

        datasets::set_item_media_assets(connection, "row-media", &["asset-2".to_string()])
            .expect("replace ids");
        let row = datasets::get_item(connection, "row-media")
            .expect("get")
            .expect("present");
        assert_eq!(row.media_asset_ids, vec!["asset-2"]);
    }

    #[test]
    fn tags_and_collections_round_trip() {
        let database = db();
        let connection = database.connection();
        let asset_id = seed_asset(connection);

        generations::insert(
            connection,
            &generations::NewGeneration {
                id: "gen-1".into(),
                parent_id: None,
                asset_id: Some(asset_id),
                edit_id: None,
                description_id: None,
                project_id: None,
                shot_index: None,
                preset_id: "mmh3".into(),
                preset_version: "1".into(),
                preset_hash: "hash".into(),
                prompt: "x".into(),
                negative: None,
                params_json: "{}".into(),
                guidance_json: "{}".into(),
                description_json: None,
                seed: None,
                models_json: "[]".into(),
                status: "succeeded".into(),
                error: None,
                raw_response: None,
                tokens_in: None,
                tokens_out: None,
                latency_ms: None,
                cache_key: None,
            },
        )
        .expect("generation");

        let tag_id = tags::ensure(connection, "tag-1", "neon").expect("tag");
        tags::attach(connection, "gen-1", &tag_id).expect("attach");
        assert_eq!(
            tags::list_for_generation(connection, "gen-1").expect("list tags"),
            vec!["neon"]
        );
        assert!(tags::detach(connection, "gen-1", &tag_id).expect("detach"));

        collections::create(connection, "col-1", "city spots").expect("collection");
        collections::add_item(connection, "col-1", "gen-1").expect("add item");
        assert_eq!(
            collections::list_items(connection, "col-1").expect("items"),
            vec!["gen-1"]
        );
        assert!(collections::rename(connection, "col-1", "night").expect("rename"));
        assert_eq!(
            collections::list(connection).expect("list")[0].name,
            "night"
        );
        assert!(collections::remove_item(connection, "col-1", "gen-1").expect("remove"));
        assert!(collections::delete(connection, "col-1").expect("delete"));
    }

    #[test]
    fn fts_tracks_insert_update_delete() {
        let database = db();
        let connection = database.connection();
        let asset_id = seed_asset(connection);

        seed_generation(
            connection,
            &asset_id,
            "gen-1",
            "a red fox",
            Some("blurry"),
            Some("{\"subject\":\"neon alley\"}"),
            "{\"run\":\"cinematic\"}",
        );

        assert_eq!(
            generations::search(connection, "fox", 10)
                .expect("search")
                .len(),
            1
        );
        assert_eq!(
            generations::search(connection, "blurry", 10)
                .expect("search")
                .len(),
            1
        );
        assert_eq!(
            generations::search(connection, "alley", 10)
                .expect("search")
                .len(),
            1
        );
        assert_eq!(
            generations::search(connection, "cinematic", 10)
                .expect("search")
                .len(),
            1
        );

        connection
            .execute(
                "UPDATE generations SET prompt = 'a blue fox' WHERE id = 'gen-1'",
                [],
            )
            .expect("update prompt");
        assert_eq!(
            generations::search(connection, "red", 10)
                .expect("search")
                .len(),
            0
        );
        assert_eq!(
            generations::search(connection, "blue", 10)
                .expect("search")
                .len(),
            1
        );

        generations::soft_delete(connection, "gen-1").expect("soft delete");
        assert_eq!(
            generations::search(connection, "blue", 10)
                .expect("search")
                .len(),
            0
        );

        generations::restore(connection, "gen-1").expect("restore");
        assert_eq!(
            generations::search(connection, "blue", 10)
                .expect("search")
                .len(),
            1
        );

        generations::hard_delete(connection, "gen-1").expect("hard delete");
        assert_eq!(
            generations::search(connection, "blue", 10)
                .expect("search")
                .len(),
            0
        );
    }

    #[test]
    fn fts_handles_empty_and_special_queries() {
        let database = db();
        let connection = database.connection();
        assert!(
            generations::search(connection, "   ", 10)
                .expect("empty")
                .is_empty()
        );
        assert!(
            generations::search(connection, "\"quoted\"", 10)
                .expect("quoted")
                .is_empty()
        );
    }

    #[test]
    fn settings_round_trip() {
        let database = db();
        let connection = database.connection();

        settings::set(connection, "theme", "\"dark\"").expect("set");
        assert_eq!(
            settings::get(connection, "theme").expect("get"),
            Some("\"dark\"".to_string())
        );
        settings::set(connection, "theme", "\"light\"").expect("update");
        assert_eq!(
            settings::get(connection, "theme").expect("get"),
            Some("\"light\"".to_string())
        );
        assert!(settings::delete(connection, "theme").expect("delete"));
        assert!(settings::get(connection, "theme").expect("get").is_none());
    }

    #[test]
    fn transactions_roll_back_on_error() {
        let mut database = db();
        let transaction = database
            .connection_mut()
            .transaction()
            .expect("begin transaction");
        transaction
            .execute(
                "INSERT INTO tags (id, name) VALUES (?1, ?2)",
                params!["tag-x", "rolled-back"],
            )
            .expect("insert in transaction");
        transaction.rollback().expect("rollback");

        let count: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }
}
