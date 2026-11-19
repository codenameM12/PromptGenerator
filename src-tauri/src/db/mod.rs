use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

pub mod repository;

use crate::commands::CommandError;

const MIGRATION_001: &str = r#"
CREATE TABLE assets (
    id                TEXT PRIMARY KEY,
    hash              TEXT NOT NULL UNIQUE,
    kind              TEXT NOT NULL CHECK (kind IN ('image', 'video')),
    source_path       TEXT NOT NULL,
    display_name      TEXT NOT NULL,
    width             INTEGER,
    height            INTEGER,
    fps               REAL,
    duration_ms       INTEGER,
    codec             TEXT,
    has_audio         INTEGER NOT NULL DEFAULT 0 CHECK (has_audio IN (0, 1)),
    color             TEXT,
    rotation          INTEGER NOT NULL DEFAULT 0,
    thumbnail_path    TEXT,
    proxy_path        TEXT,
    embedded_metadata TEXT,
    favorite          INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    missing           INTEGER NOT NULL DEFAULT 0 CHECK (missing IN (0, 1)),
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_assets_created_at ON assets (created_at DESC);

CREATE TABLE asset_frames (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    asset_id     TEXT NOT NULL REFERENCES assets (id) ON DELETE CASCADE,
    frame_index  INTEGER NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    path         TEXT NOT NULL,
    UNIQUE (asset_id, frame_index)
);
CREATE INDEX idx_asset_frames_asset ON asset_frames (asset_id, frame_index);

CREATE TABLE edits (
    id          TEXT PRIMARY KEY,
    asset_id    TEXT NOT NULL REFERENCES assets (id) ON DELETE CASCADE,
    ops_json    TEXT NOT NULL,
    target_json TEXT NOT NULL,
    output_path TEXT NOT NULL,
    cache_key   TEXT NOT NULL UNIQUE,
    width       INTEGER,
    height      INTEGER,
    fps         REAL,
    duration_ms INTEGER,
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_edits_asset ON edits (asset_id, created_at DESC);

CREATE TABLE descriptions (
    id             TEXT PRIMARY KEY,
    asset_id       TEXT REFERENCES assets (id) ON DELETE CASCADE,
    edit_id        TEXT REFERENCES edits (id) ON DELETE CASCADE,
    model          TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    sampling_json  TEXT NOT NULL,
    cache_key      TEXT NOT NULL UNIQUE,
    json           TEXT NOT NULL,
    confidence     REAL,
    cached         INTEGER NOT NULL DEFAULT 0 CHECK (cached IN (0, 1)),
    created_at     TEXT NOT NULL
);
CREATE INDEX idx_descriptions_asset ON descriptions (asset_id, edit_id);

CREATE TABLE presets (
    id            TEXT PRIMARY KEY,
    owner         TEXT NOT NULL CHECK (owner IN ('builtin', 'user')),
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('image', 'video', 'edit')),
    version       TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    document_json TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    deleted_at    TEXT
);
CREATE INDEX idx_presets_owner ON presets (owner, kind);
CREATE INDEX idx_presets_hash ON presets (content_hash);

CREATE TABLE generations (
    id               TEXT PRIMARY KEY,
    parent_id        TEXT REFERENCES generations (id) ON DELETE SET NULL,
    asset_id         TEXT REFERENCES assets (id) ON DELETE SET NULL,
    edit_id          TEXT REFERENCES edits (id) ON DELETE SET NULL,
    description_id   TEXT REFERENCES descriptions (id) ON DELETE SET NULL,
    project_id       TEXT,
    shot_index       INTEGER,
    preset_id        TEXT NOT NULL,
    preset_version   TEXT NOT NULL,
    preset_hash      TEXT NOT NULL,
    prompt           TEXT NOT NULL,
    negative         TEXT,
    params_json      TEXT NOT NULL,
    guidance_json    TEXT NOT NULL,
    description_json TEXT,
    seed             INTEGER,
    models_json      TEXT NOT NULL,
    status           TEXT NOT NULL,
    error            TEXT,
    raw_response     TEXT,
    tokens_in        INTEGER,
    tokens_out       INTEGER,
    latency_ms       INTEGER,
    cache_key        TEXT UNIQUE,
    favorite         INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    rating           INTEGER,
    notes            TEXT,
    created_at       TEXT NOT NULL,
    deleted_at       TEXT
);
CREATE INDEX idx_generations_created_at ON generations (created_at DESC);
CREATE INDEX idx_generations_preset ON generations (preset_id);
CREATE INDEX idx_generations_parent ON generations (parent_id);
CREATE INDEX idx_generations_status ON generations (status);
CREATE INDEX idx_generations_favorite ON generations (favorite, created_at DESC);

CREATE TABLE tags (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE generation_tags (
    generation_id TEXT NOT NULL REFERENCES generations (id) ON DELETE CASCADE,
    tag_id        TEXT NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (generation_id, tag_id)
);
CREATE INDEX idx_generation_tags_tag ON generation_tags (tag_id);

CREATE TABLE asset_tags (
    asset_id TEXT NOT NULL REFERENCES assets (id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (asset_id, tag_id)
);

CREATE TABLE collections (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE collection_items (
    collection_id TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    generation_id TEXT NOT NULL REFERENCES generations (id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, generation_id)
);
CREATE INDEX idx_collection_items_generation ON collection_items (generation_id);

CREATE TABLE jobs (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    state       TEXT NOT NULL,
    progress    REAL NOT NULL DEFAULT 0,
    payload_json TEXT NOT NULL,
    error       TEXT,
    attempts    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    started_at  TEXT,
    finished_at TEXT
);
CREATE INDEX idx_jobs_state ON jobs (state, created_at);

CREATE TABLE job_items (
    id              TEXT PRIMARY KEY,
    job_id          TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    item_index      INTEGER NOT NULL,
    state           TEXT NOT NULL,
    payload_json    TEXT NOT NULL,
    result_json     TEXT,
    error           TEXT,
    attempts        INTEGER NOT NULL DEFAULT 0,
    idempotency_key TEXT,
    started_at      TEXT,
    finished_at     TEXT,
    UNIQUE (job_id, item_index)
);
CREATE INDEX idx_job_items_job ON job_items (job_id, item_index);
CREATE UNIQUE INDEX idx_job_items_idempotency ON job_items (job_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE datasets (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    source_path  TEXT NOT NULL,
    mapping_json TEXT,
    row_count    INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);

CREATE TABLE dataset_rows (
    id            TEXT PRIMARY KEY,
    dataset_id    TEXT NOT NULL REFERENCES datasets (id) ON DELETE CASCADE,
    row_index     INTEGER NOT NULL,
    data_json     TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    generation_id TEXT REFERENCES generations (id) ON DELETE SET NULL,
    error         TEXT,
    UNIQUE (dataset_id, row_index)
);
CREATE INDEX idx_dataset_rows_dataset ON dataset_rows (dataset_id, row_index);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
"#;

const MIGRATION_002: &str = r#"
CREATE VIRTUAL TABLE generations_fts USING fts5(
    prompt,
    negative,
    guidance,
    description,
    content = 'generations',
    content_rowid = 'rowid'
);

CREATE TRIGGER generations_fts_ai AFTER INSERT ON generations BEGIN
    INSERT INTO generations_fts (rowid, prompt, negative, guidance, description)
    VALUES (
        new.rowid,
        new.prompt,
        COALESCE(new.negative, ''),
        new.guidance_json,
        COALESCE(new.description_json, '')
    );
END;

CREATE TRIGGER generations_fts_ad AFTER DELETE ON generations BEGIN
    INSERT INTO generations_fts (generations_fts, rowid, prompt, negative, guidance, description)
    VALUES (
        'delete',
        old.rowid,
        old.prompt,
        COALESCE(old.negative, ''),
        old.guidance_json,
        COALESCE(old.description_json, '')
    );
END;

CREATE TRIGGER generations_fts_au AFTER UPDATE ON generations BEGIN
    INSERT INTO generations_fts (generations_fts, rowid, prompt, negative, guidance, description)
    VALUES (
        'delete',
        old.rowid,
        old.prompt,
        COALESCE(old.negative, ''),
        old.guidance_json,
        COALESCE(old.description_json, '')
    );
    INSERT INTO generations_fts (rowid, prompt, negative, guidance, description)
    VALUES (
        new.rowid,
        new.prompt,
        COALESCE(new.negative, ''),
        new.guidance_json,
        COALESCE(new.description_json, '')
    );
END;

INSERT INTO generations_fts (rowid, prompt, negative, guidance, description)
SELECT
    rowid,
    prompt,
    COALESCE(negative, ''),
    guidance_json,
    COALESCE(description_json, '')
FROM generations;
"#;

const MIGRATION_003: &str = r#"
ALTER TABLE jobs ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN dedupe_key TEXT;
ALTER TABLE jobs ADD COLUMN note TEXT;
CREATE INDEX idx_jobs_priority ON jobs (state, priority DESC, created_at);
CREATE UNIQUE INDEX idx_jobs_active_dedupe ON jobs (dedupe_key)
    WHERE dedupe_key IS NOT NULL AND state IN ('queued', 'waiting', 'running', 'interrupted');
"#;

const MIGRATION_004: &str = r#"
ALTER TABLE assets ADD COLUMN deleted_at TEXT;
CREATE INDEX idx_assets_deleted ON assets (deleted_at);
"#;

const MIGRATION_005: &str = r#"
ALTER TABLE datasets ADD COLUMN export_columns_json TEXT;
CREATE TABLE import_profiles (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    mapping_json TEXT NOT NULL,
    created_at   TEXT NOT NULL
);
"#;

const MIGRATION_006: &str = r#"
ALTER TABLE dataset_rows ADD COLUMN media_asset_ids_json TEXT;
"#;

struct Migration {
    version: i64,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: MIGRATION_001,
    },
    Migration {
        version: 2,
        sql: MIGRATION_002,
    },
    Migration {
        version: 3,
        sql: MIGRATION_003,
    },
    Migration {
        version: 4,
        sql: MIGRATION_004,
    },
    Migration {
        version: 5,
        sql: MIGRATION_005,
    },
    Migration {
        version: 6,
        sql: MIGRATION_006,
    },
];

const LATEST_VERSION: i64 = 6;

/// SQLite connection with embedded, forward-only migrations.
pub struct Database {
    connection: Connection,
    path: Option<PathBuf>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, CommandError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| CommandError::from_io(&error))?;
        }
        let connection = Connection::open(path)?;
        configure(&connection)?;
        Ok(Self {
            connection,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn open_in_memory() -> Result<Self, CommandError> {
        let connection = Connection::open_in_memory()?;
        configure(&connection)?;
        Ok(Self {
            connection,
            path: None,
        })
    }

    pub fn migrate(&mut self) -> Result<(), CommandError> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )?;

        let applied: HashSet<i64> = {
            let mut statement = self
                .connection
                .prepare("SELECT version FROM schema_migrations")?;
            let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };

        if let Some(version) = applied.iter().max()
            && *version > LATEST_VERSION
        {
            return Err(CommandError::conflict(format!(
                "Database schema version {version} is newer than this build supports ({LATEST_VERSION}). \
                 Install a newer Prompt Studio build or restore a backup."
            ))
            .with_details(self.describe_path()));
        }

        let pending: Vec<&Migration> = MIGRATIONS
            .iter()
            .filter(|migration| !applied.contains(&migration.version))
            .collect();

        if pending.is_empty() {
            return Ok(());
        }

        if let Some(highest) = applied.iter().max() {
            self.backup_before_migration(*highest)?;
        }

        for migration in pending {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, applied_at)
                 VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [migration.version],
            )?;
            transaction.commit()?;
        }

        Ok(())
    }

    fn describe_path(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<in-memory>".into())
    }

    fn backup_before_migration(&self, from_version: i64) -> Result<Option<PathBuf>, CommandError> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("prompt-studio.db");
        let candidate = path.with_file_name(format!("{file_name}.pre-v{from_version}.bak"));
        let backup = unique_path(candidate);

        self.connection
            .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;

        Ok(Some(backup))
    }

    pub fn applied_migrations(&self) -> Result<Vec<i64>, CommandError> {
        let mut statement = self
            .connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn table_names(&self) -> Result<Vec<String>, CommandError> {
        let mut statement = self.connection.prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Writes a consistent snapshot to `output` and returns its size in bytes.
    pub fn backup_to(&self, output: &Path) -> Result<u64, CommandError> {
        let parent = output
            .parent()
            .ok_or_else(|| CommandError::validation("Backup path has no parent directory"))?;
        if !parent.is_dir() {
            return Err(CommandError::validation(format!(
                "Backup folder does not exist: {}",
                parent.display()
            )));
        }
        if output.exists() {
            return Err(CommandError::conflict(format!(
                "Backup already exists: {}",
                output.display()
            )));
        }

        self.connection
            .execute("VACUUM INTO ?1", [output.to_string_lossy().as_ref()])?;

        Ok(fs::metadata(output)
            .map_err(|error| CommandError::from_io(&error))?
            .len())
    }

    /// Replaces this database with the contents of a backup. A pre-restore
    /// snapshot is written beside the live database first.
    pub fn restore_from(&mut self, backup_path: &Path) -> Result<Option<PathBuf>, CommandError> {
        validate_backup(backup_path)?;

        let pre_restore = if let Some(path) = &self.path {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("prompt-studio.db");
            let candidate = path.with_file_name(format!("{file_name}.pre-restore.bak"));
            let backup = unique_path(candidate);
            self.connection
                .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
            Some(backup)
        } else {
            None
        };

        let source = Connection::open_with_flags(
            backup_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        {
            let backup = rusqlite::backup::Backup::new(&source, &mut self.connection)?;
            backup.run_to_completion(100, std::time::Duration::from_millis(0), None)?;
        }

        self.migrate()?;
        Ok(pre_restore)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

fn validate_backup(path: &Path) -> Result<(), CommandError> {
    if !path.is_file() {
        return Err(CommandError::not_found(format!(
            "Backup file not found: {}",
            path.display()
        )));
    }

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|_| {
            CommandError::validation(
                "This file is not a Prompt Studio backup (missing schema history)",
            )
        })?;

    if version > LATEST_VERSION {
        return Err(CommandError::conflict(format!(
            "Backup schema version {version} is newer than this build supports ({LATEST_VERSION})"
        )));
    }

    Ok(())
}

fn unique_path(candidate: PathBuf) -> PathBuf {
    if !candidate.exists() {
        return candidate;
    }
    let stem = candidate
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("backup");
    let extension = candidate
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bak");
    for index in 2..1000 {
        let next = candidate.with_file_name(format!("{stem}-{index}.{extension}"));
        if !next.exists() {
            return next;
        }
    }
    candidate
}

/// Current UTC timestamp in SQLite's ISO-8601 form.
pub fn now_iso(connection: &Connection) -> Result<String, CommandError> {
    Ok(
        connection.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get(0)
        })?,
    )
}

fn configure(connection: &Connection) -> Result<(), CommandError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

impl From<rusqlite::Error> for CommandError {
    fn from(error: rusqlite::Error) -> Self {
        CommandError::internal(format!("database error: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use rusqlite::params;

    use super::{
        Database, MIGRATION_001, MIGRATION_002, MIGRATION_003, MIGRATION_004, MIGRATION_005,
    };
    use crate::db::repository::assets;

    fn expected_tables() -> Vec<&'static str> {
        vec![
            "asset_frames",
            "asset_tags",
            "assets",
            "collection_items",
            "collections",
            "dataset_rows",
            "datasets",
            "descriptions",
            "edits",
            "generation_tags",
            "generations",
            "job_items",
            "jobs",
            "presets",
            "schema_migrations",
            "settings",
            "tags",
        ]
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "prompt-studio-db-{label}-{}-{}",
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
    fn migrates_an_empty_database() {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");

        let tables = database.table_names().expect("tables");
        for expected in expected_tables() {
            assert!(
                tables.iter().any(|name| name == expected),
                "missing table {expected}; got {tables:?}"
            );
        }
        assert_eq!(
            database.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert!(tables.iter().any(|name| name == "generations_fts"));
    }

    #[test]
    fn upgrading_a_v5_database_adds_row_media_ids() {
        let temp = TempDir::new("media-migration");
        let path = temp.path.join("prompt-studio.db");
        {
            let connection = rusqlite::Connection::open(&path).expect("open raw");
            connection
                .execute_batch(
                    "CREATE TABLE schema_migrations (
                        version    INTEGER PRIMARY KEY,
                        applied_at TEXT NOT NULL
                    );",
                )
                .expect("migration table");
            for (version, sql) in [
                (1, MIGRATION_001),
                (2, MIGRATION_002),
                (3, MIGRATION_003),
                (4, MIGRATION_004),
                (5, MIGRATION_005),
            ] {
                connection.execute_batch(sql).expect("schema");
                connection
                    .execute(
                        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'legacy')",
                        [version],
                    )
                    .expect("mark");
            }
        }

        let mut database = Database::open(&path).expect("open");
        database.migrate().expect("upgrade");
        assert_eq!(
            database.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );

        let connection = database.connection();
        let column: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('dataset_rows')
                 WHERE name = 'media_asset_ids_json'",
                [],
                |row| row.get(0),
            )
            .expect("column");
        assert_eq!(column, 1);

        connection
            .execute(
                "INSERT INTO datasets (id, name, source_path, mapping_json, export_columns_json, row_count, created_at)
                 VALUES ('d1', 'd', 'p', NULL, NULL, 1, 'now')",
                [],
            )
            .expect("dataset");
        connection
            .execute(
                "INSERT INTO dataset_rows (id, dataset_id, row_index, data_json, status)
                 VALUES ('r1', 'd1', 0, '{}', 'pending')",
                [],
            )
            .expect("row");
        let stored: Option<String> = connection
            .query_row(
                "SELECT media_asset_ids_json FROM dataset_rows WHERE id = 'r1'",
                [],
                |row| row.get(0),
            )
            .expect("row query");
        assert_eq!(stored, None);
    }

    #[test]
    fn migrating_twice_is_idempotent() {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("first migrate");
        database.migrate().expect("second migrate");

        assert_eq!(
            database.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );
        let count: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(count, 6);
    }

    #[test]
    fn migrations_persist_across_reopen() {
        let temp = TempDir::new("reopen");
        let path = temp.path.join("prompt-studio.db");
        {
            let mut database = Database::open(&path).expect("open");
            database.migrate().expect("migrate");
        }

        let mut database = Database::open(&path).expect("reopen");
        database.migrate().expect("migrate again");
        assert_eq!(
            database.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert!(
            database
                .table_names()
                .expect("tables")
                .contains(&"assets".into())
        );
    }

    #[test]
    fn refuses_a_database_from_a_newer_build() {
        let temp = TempDir::new("newer");
        let path = temp.path.join("prompt-studio.db");
        {
            let mut database = Database::open(&path).expect("open");
            database.migrate().expect("migrate");
            database
                .connection()
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (99, 'future')",
                    [],
                )
                .expect("insert future version");
        }

        let mut database = Database::open(&path).expect("reopen");
        let error = database
            .migrate()
            .expect_err("newer schema must be refused");
        assert_eq!(error.category, crate::commands::ErrorCategory::Conflict);
        assert!(error.message.contains("newer"));
        assert!(error.details.as_deref().unwrap_or_default().contains(".db"));
    }

    #[test]
    fn creates_a_backup_before_applying_pending_migrations() {
        let temp = TempDir::new("backup");
        let path = temp.path.join("prompt-studio.db");

        {
            let connection = rusqlite::Connection::open(&path).expect("open raw");
            connection
                .execute_batch(
                    "CREATE TABLE schema_migrations (
                        version    INTEGER PRIMARY KEY,
                        applied_at TEXT NOT NULL
                    );",
                )
                .expect("migration table");
            connection.execute_batch(MIGRATION_001).expect("schema v1");
            connection
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (1, 'legacy')",
                    [],
                )
                .expect("mark v1");
        }

        let mut database = Database::open(&path).expect("open");
        database.migrate().expect("upgrade");
        assert_eq!(
            database.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );

        let backups: Vec<String> = fs::read_dir(&temp.path)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".pre-v1."))
            .collect();
        assert_eq!(
            backups.len(),
            1,
            "expected one pre-migration backup: {backups:?}"
        );
    }

    #[test]
    fn fresh_database_does_not_create_a_backup() {
        let temp = TempDir::new("fresh");
        let path = temp.path.join("prompt-studio.db");
        let mut database = Database::open(&path).expect("open");
        database.migrate().expect("migrate");

        let backups = fs::read_dir(&temp.path)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".pre-v"))
            .count();
        assert_eq!(backups, 0);
    }

    fn insert_asset(connection: &rusqlite::Connection, id: &str, hash: &str) {
        assets::insert(
            connection,
            &assets::NewAsset {
                id: id.into(),
                hash: hash.into(),
                kind: "image".into(),
                source_path: format!("C:\\media\\{id}.png"),
                display_name: format!("{id}.png"),
                width: Some(1024),
                height: Some(1024),
                fps: None,
                duration_ms: None,
                codec: Some("png".into()),
                has_audio: false,
                color: None,
                rotation: 0,
                thumbnail_path: None,
                proxy_path: None,
                embedded_metadata: None,
            },
        )
        .expect("insert asset");
    }

    #[test]
    fn backup_and_restore_round_trip() {
        let temp = TempDir::new("backup-restore");
        let source_path = temp.path.join("source.db");
        let mut source = Database::open(&source_path).expect("source");
        source.migrate().expect("migrate source");
        insert_asset(source.connection(), "asset-1", "hash-1");

        let backup_path = temp.path.join("library-backup.db");
        let bytes = source.backup_to(&backup_path).expect("backup");
        assert!(bytes > 0);
        assert!(backup_path.is_file());

        let target_path = temp.path.join("target.db");
        let mut target = Database::open(&target_path).expect("target");
        target.migrate().expect("migrate target");
        insert_asset(target.connection(), "asset-old", "hash-old");

        let pre_restore = target
            .restore_from(&backup_path)
            .expect("restore")
            .expect("pre-restore snapshot");
        assert!(pre_restore.is_file());
        assert!(
            assets::get(target.connection(), "asset-1")
                .expect("get")
                .is_some()
        );
        assert!(
            assets::get(target.connection(), "asset-old")
                .expect("get")
                .is_none()
        );
        assert_eq!(
            target.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );

        let pre_connection = rusqlite::Connection::open(&pre_restore).expect("open pre-restore");
        let old_count: i64 = pre_connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE id = 'asset-old'",
                [],
                |row| row.get(0),
            )
            .expect("count old");
        assert_eq!(old_count, 1);
    }

    #[test]
    fn restore_refuses_a_newer_backup() {
        let temp = TempDir::new("restore-newer");
        let newer_path = temp.path.join("newer.db");
        let mut newer = Database::open(&newer_path).expect("open newer");
        newer.migrate().expect("migrate newer");
        newer
            .connection()
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (99, 'future')",
                [],
            )
            .expect("future version");
        let backup_path = temp.path.join("newer-backup.db");
        newer.backup_to(&backup_path).expect("backup newer");

        let target_path = temp.path.join("target.db");
        let mut target = Database::open(&target_path).expect("open target");
        target.migrate().expect("migrate target");

        let error = target
            .restore_from(&backup_path)
            .expect_err("newer backup must be refused");
        assert_eq!(error.category, crate::commands::ErrorCategory::Conflict);
        assert_eq!(
            target.applied_migrations().expect("migrations"),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn restore_rejects_a_non_backup_file() {
        let temp = TempDir::new("not-backup");
        let bogus = temp.path.join("bogus.db");
        fs::write(&bogus, b"not a database").expect("write bogus");

        let target_path = temp.path.join("target.db");
        let mut target = Database::open(&target_path).expect("open");
        target.migrate().expect("migrate");

        let error = target.restore_from(&bogus).expect_err("must reject");
        assert_eq!(error.category, crate::commands::ErrorCategory::Validation);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let mut database = Database::open_in_memory().expect("open");
        database.migrate().expect("migrate");

        let result = database.connection_mut().execute(
            "INSERT INTO asset_frames (asset_id, frame_index, timestamp_ms, path)
             VALUES (?1, ?2, ?3, ?4)",
            params!["missing-asset", 0_i64, 0_i64, "C:\\frame.png"],
        );

        assert!(result.is_err(), "foreign key must reject orphan frames");
    }
}
