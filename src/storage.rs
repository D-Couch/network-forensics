//! Versioned local SQLite storage for observed events.

use crate::event::{Event, EventKind, SCHEMA_VERSION};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const STORAGE_SCHEMA_VERSION: i64 = 1;
const DATABASE_FILE_NAME: &str = "events.sqlite3";
const MAX_DATABASE_SIZE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_READ_PAGE_SIZE: usize = 1_000;
const MAX_WRITE_BATCH_SIZE: usize = 4_096;
const MILLIS_PER_DAY: i64 = 86_400_000;

pub const DEFAULT_RETENTION_DAYS: u32 = 30;
pub const DEFAULT_MAX_EVENTS: usize = 100_000;
pub const MAX_RETENTION_DAYS: u32 = 3_650;
pub const MAX_EVENT_COUNT: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    retention_days: u32,
    max_events: usize,
}

impl RetentionPolicy {
    pub fn new(retention_days: u32, max_events: usize) -> Result<Self, StorageError> {
        if !(1..=MAX_RETENTION_DAYS).contains(&retention_days) {
            return Err(StorageError::InvalidPolicy(
                "retention days must be between 1 and 3650",
            ));
        }
        if !(1..=MAX_EVENT_COUNT).contains(&max_events) {
            return Err(StorageError::InvalidPolicy(
                "maximum event count must be between 1 and 1000000",
            ));
        }
        Ok(Self {
            retention_days,
            max_events,
        })
    }

    pub fn retention_days(self) -> u32 {
        self.retention_days
    }

    pub fn max_events(self) -> usize {
        self.max_events
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            max_events: DEFAULT_MAX_EVENTS,
        }
    }
}

#[derive(Debug)]
pub enum StorageError {
    Sql(rusqlite::Error),
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedSchema(i64),
    InvalidEventSchema(u16),
    InvalidPolicy(&'static str),
    InvalidDataHome,
    InvalidCursor,
    InvalidPageSize,
    InvalidDatabasePath,
    BatchTooLarge,
    DatabaseTooLarge,
    SymlinkPath,
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sql(error) => write!(formatter, "SQLite error: {error}"),
            Self::Io(error) => write!(formatter, "storage I/O error: {error}"),
            Self::Json(error) => write!(formatter, "stored event JSON error: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported database schema version {version}")
            }
            Self::InvalidEventSchema(version) => {
                write!(formatter, "unsupported event schema version {version}")
            }
            Self::InvalidPolicy(message) => formatter.write_str(message),
            Self::InvalidDataHome => formatter.write_str(
                "an absolute XDG_DATA_HOME or HOME directory is required for local storage",
            ),
            Self::InvalidCursor => formatter.write_str("event sequence cursor must be nonnegative"),
            Self::InvalidPageSize => {
                formatter.write_str("event page size must be between 1 and 1000")
            }
            Self::InvalidDatabasePath => {
                formatter.write_str("event database path must be a regular file")
            }
            Self::BatchTooLarge => formatter.write_str("event batch may not exceed 4096 records"),
            Self::DatabaseTooLarge => {
                formatter.write_str("event database exceeds the configured 256 MiB limit")
            }
            Self::SymlinkPath => {
                formatter.write_str("refusing to use a symbolic link as the event database path")
            }
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sql(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    /// Monotonic insertion sequence; use this cursor rather than wall-clock time for stable paging.
    pub sequence: i64,
    pub event: Event,
}

pub struct EventStore {
    connection: Connection,
    policy: RetentionPolicy,
}

impl EventStore {
    /// Opens the default database under `$XDG_DATA_HOME/netforensics`, or
    /// `$HOME/.local/share/netforensics` when XDG_DATA_HOME is unset.
    pub fn open_default(policy: RetentionPolicy) -> Result<Self, StorageError> {
        let data_home = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let home = env::var_os("HOME").map(PathBuf::from);
        let path = resolve_database_path(data_home.as_deref(), home.as_deref())?;
        let directory = path.parent().ok_or(StorageError::InvalidDataHome)?;
        create_private_data_dir(directory)?;
        open_database(&path, policy)
    }

    /// Opens a database path whose parent directory already exists.
    pub fn open(path: impl AsRef<Path>, policy: RetentionPolicy) -> Result<Self, StorageError> {
        open_database(path.as_ref(), policy)
    }

    /// Creates an in-memory store for deterministic tests.
    pub fn open_in_memory(policy: RetentionPolicy) -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        initialize(connection, policy)
    }

    /// Inserts an observation cycle and applies retention in the same transaction.
    pub fn store_events(&mut self, events: &[Event]) -> Result<(), StorageError> {
        self.store_events_at(events, DateTime::<Utc>::from(SystemTime::now()))
    }

    /// `now` is injectable so age retention can be tested without sleeping.
    pub fn store_events_at(
        &mut self,
        events: &[Event],
        now: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        if events.len() > MAX_WRITE_BATCH_SIZE {
            return Err(StorageError::BatchTooLarge);
        }
        for event in events {
            if event.schema_version != SCHEMA_VERSION {
                return Err(StorageError::InvalidEventSchema(event.schema_version));
            }
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let cutoff = now
            .timestamp_millis()
            .saturating_sub(i64::from(self.policy.retention_days) * MILLIS_PER_DAY);
        transaction.execute(
            "DELETE FROM events WHERE recorded_at_unix_ms < ?1",
            [cutoff],
        )?;

        for event in events {
            let event_json = serde_json::to_string(event)?;
            transaction.execute(
                "INSERT INTO events (event_id, schema_version, recorded_at_utc, recorded_at_unix_ms, source, event_kind, event_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.id,
                    event.schema_version,
                    event.recorded_at_utc.to_rfc3339(),
                    event.recorded_at_utc.timestamp_millis(),
                    event.source,
                    event_kind_name(&event.kind),
                    event_json,
                ],
            )?;
        }

        let maximum = self.policy.max_events as i64;
        transaction.execute(
            "DELETE FROM events WHERE sequence IN (SELECT sequence FROM events ORDER BY sequence DESC LIMIT -1 OFFSET ?1)",
            [maximum],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Reads a bounded page after an insertion-sequence cursor.
    pub fn read_events_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, StorageError> {
        if after_sequence < 0 {
            return Err(StorageError::InvalidCursor);
        }
        if !(1..=MAX_READ_PAGE_SIZE).contains(&limit) {
            return Err(StorageError::InvalidPageSize);
        }
        let mut statement = self.connection.prepare(
            "SELECT sequence, event_json FROM events WHERE sequence > ?1 ORDER BY sequence ASC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![after_sequence, limit as i64], |row| {
            let sequence = row.get(0)?;
            let event_json: String = row.get(1)?;
            Ok((sequence, event_json))
        })?;

        let mut stored = Vec::new();
        for row in rows {
            let (sequence, event_json) = row?;
            stored.push(StoredEvent {
                sequence,
                event: Event::from_json(&event_json)?,
            });
        }
        Ok(stored)
    }

    pub fn event_count(&self) -> Result<usize, StorageError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn storage_schema_version(&self) -> Result<i64, StorageError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }
}

pub fn resolve_database_path(
    xdg_data_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, StorageError> {
    let base = if let Some(xdg_data_home) = xdg_data_home {
        if !xdg_data_home.is_absolute() {
            return Err(StorageError::InvalidDataHome);
        }
        xdg_data_home.to_path_buf()
    } else if let Some(home) = home {
        if !home.is_absolute() {
            return Err(StorageError::InvalidDataHome);
        }
        home.join(".local").join("share")
    } else {
        return Err(StorageError::InvalidDataHome);
    };
    Ok(base.join("netforensics").join(DATABASE_FILE_NAME))
}

fn open_database(path: &Path, policy: RetentionPolicy) -> Result<EventStore, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(StorageError::SymlinkPath),
        Ok(metadata) if !metadata.is_file() => return Err(StorageError::InvalidDatabasePath),
        Ok(metadata) if metadata.len() > MAX_DATABASE_SIZE_BYTES => {
            return Err(StorageError::DatabaseTooLarge)
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(StorageError::Io(error)),
    }
    set_private_file_permissions(path)?;
    reject_symlink(path)?;

    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_NOFOLLOW
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags)?;
    initialize(connection, policy)
}

fn initialize(
    mut connection: Connection,
    policy: RetentionPolicy,
) -> Result<EventStore, StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != 0 && version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchema(version));
    }
    connection.pragma_update(None, "page_size", 4096)?;
    migrate(&mut connection)?;
    let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let max_pages = (MAX_DATABASE_SIZE_BYTES / page_size as u64) as i64;
    let page_count: i64 = connection.pragma_query_value(None, "page_count", |row| row.get(0))?;
    if page_count > max_pages {
        return Err(StorageError::DatabaseTooLarge);
    }
    let configured_pages: i64 =
        connection.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    if configured_pages > max_pages {
        connection.pragma_update(None, "max_page_count", max_pages)?;
    }
    Ok(EventStore { connection, policy })
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    schema_version INTEGER NOT NULL,
                    recorded_at_utc TEXT NOT NULL,
                    recorded_at_unix_ms INTEGER NOT NULL,
                    source TEXT NOT NULL,
                    event_kind TEXT NOT NULL,
                    event_json TEXT NOT NULL
                ) STRICT;
                CREATE INDEX events_recorded_at_idx ON events(recorded_at_unix_ms, sequence);
                CREATE INDEX events_kind_time_idx ON events(event_kind, recorded_at_unix_ms, sequence);
                PRAGMA user_version = 1;",
            )?;
            transaction.commit()?;
        }
        STORAGE_SCHEMA_VERSION => {}
        other => return Err(StorageError::UnsupportedSchema(other)),
    }
    Ok(())
}

fn event_kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::LinkStateChanged { .. } => "link_state_changed",
        EventKind::ProbeResult { .. } => "probe_result",
        EventKind::InterfaceSnapshot { .. } => "interface_snapshot",
    }
}

fn create_private_data_dir(path: &Path) -> Result<(), StorageError> {
    reject_symlink(path)?;
    fs::create_dir_all(path)?;
    reject_symlink(path)?;
    set_private_directory_permissions(path)?;
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StorageError::SymlinkPath),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::Io(error)),
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), StorageError> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if !path.exists() {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(file) => drop(file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    reject_symlink(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ProbeMethod, ProbeOutcome, ProbeRole};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir() -> PathBuf {
        let path = env::temp_dir().join(format!(
            "netforensics-storage-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn event(id: &str, recorded_at: &str) -> Event {
        Event {
            schema_version: SCHEMA_VERSION,
            id: id.into(),
            recorded_at_utc: recorded_at.parse().unwrap(),
            source: "probe".into(),
            kind: EventKind::ProbeResult {
                role: ProbeRole::External,
                target: "192.0.2.1:443".parse().unwrap(),
                method: ProbeMethod::TcpConnect,
                started_at_utc: recorded_at.parse().unwrap(),
                elapsed_ms: 10,
                outcome: ProbeOutcome::Success,
            },
        }
    }

    #[test]
    fn resolves_xdg_and_home_database_paths_and_rejects_relative_paths() {
        assert_eq!(
            resolve_database_path(Some(Path::new("/data")), Some(Path::new("/home/test"))).unwrap(),
            Path::new("/data/netforensics/events.sqlite3")
        );
        assert_eq!(
            resolve_database_path(None, Some(Path::new("/home/test"))).unwrap(),
            Path::new("/home/test/.local/share/netforensics/events.sqlite3")
        );
        assert!(resolve_database_path(Some(Path::new("relative")), None).is_err());
        assert!(resolve_database_path(None, Some(Path::new("relative"))).is_err());
        assert!(resolve_database_path(None, None).is_err());
    }

    #[test]
    fn migrates_persists_and_reads_events_in_insertion_order() {
        let root = temp_dir();
        let path = root.join(DATABASE_FILE_NAME);
        let now = "2026-10-13T00:00:00Z".parse().unwrap();
        let first = event("event-1", "2026-10-12T23:59:00Z");
        let second = event("event-2", "2026-10-12T23:58:00Z");

        {
            let mut store = EventStore::open(&path, RetentionPolicy::default()).unwrap();
            assert_eq!(store.storage_schema_version().unwrap(), 1);
            store
                .store_events_at(&[first.clone(), second.clone()], now)
                .unwrap();
            assert_eq!(store.event_count().unwrap(), 2);
        }
        let store = EventStore::open(&path, RetentionPolicy::default()).unwrap();
        let page = store.read_events_after(0, 10).unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].sequence, 1);
        assert_eq!(page[0].event, first);
        assert_eq!(page[1].sequence, 2);
        assert_eq!(page[1].event, second);
        assert!(store.read_events_after(page[0].sequence, 10).unwrap().len() == 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_insert_rolls_back_when_any_event_fails() {
        let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        let now = "2026-10-13T00:00:00Z".parse().unwrap();
        store
            .store_events_at(&[event("existing", "2026-10-12T23:00:00Z")], now)
            .unwrap();
        let result = store.store_events_at(
            &[
                event("new", "2026-10-12T23:01:00Z"),
                event("existing", "2026-10-12T23:02:00Z"),
            ],
            now,
        );
        assert!(result.is_err());
        assert_eq!(store.event_count().unwrap(), 1);
        assert_eq!(
            store.read_events_after(0, 10).unwrap()[0].event.id,
            "existing"
        );
    }

    #[test]
    fn retention_applies_age_and_count_limits() {
        let policy = RetentionPolicy::new(1, 2).unwrap();
        let mut store = EventStore::open_in_memory(policy).unwrap();
        let now = "2026-10-13T00:00:00Z".parse().unwrap();
        store
            .store_events_at(
                &[
                    event("old", "2026-10-11T23:59:59Z"),
                    event("one", "2026-10-12T23:00:00Z"),
                    event("two", "2026-10-12T23:30:00Z"),
                    event("three", "2026-10-12T23:45:00Z"),
                ],
                now,
            )
            .unwrap();
        assert_eq!(store.event_count().unwrap(), 2);
        let rows = store.read_events_after(0, 10).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.event.id.as_str())
                .collect::<Vec<_>>(),
            ["two", "three"]
        );
    }

    #[test]
    fn rejects_future_database_and_event_schema_versions() {
        let connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "user_version", 42).unwrap();
        assert!(matches!(
            initialize(connection, RetentionPolicy::default()),
            Err(StorageError::UnsupportedSchema(42))
        ));

        let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        let mut unsupported = event("future", "2026-10-12T23:59:00Z");
        unsupported.schema_version = 2;
        assert!(matches!(
            store.store_events_at(&[unsupported], "2026-10-13T00:00:00Z".parse().unwrap()),
            Err(StorageError::InvalidEventSchema(2))
        ));
        assert_eq!(store.event_count().unwrap(), 0);
    }

    #[test]
    fn rejects_future_file_schema_without_modifying_it() {
        let root = temp_dir();
        let path = root.join(DATABASE_FILE_NAME);
        {
            let connection = Connection::open(&path).unwrap();
            connection.pragma_update(None, "page_size", 1024).unwrap();
            connection
                .execute_batch("CREATE TABLE marker (value INTEGER);")
                .unwrap();
            connection.pragma_update(None, "user_version", 42).unwrap();
        }
        assert!(matches!(
            EventStore::open(&path, RetentionPolicy::default()),
            Err(StorageError::UnsupportedSchema(42))
        ));
        let connection = Connection::open(&path).unwrap();
        let page_size: i64 = connection
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let marker_exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'marker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(page_size, 1024);
        assert_eq!(version, 42);
        assert_eq!(marker_exists, 1);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enforces_the_database_page_limit() {
        let store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        let page_size: i64 = store
            .connection
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .unwrap();
        let max_pages: i64 = store
            .connection
            .pragma_query_value(None, "max_page_count", |row| row.get(0))
            .unwrap();
        assert_eq!(page_size * max_pages, MAX_DATABASE_SIZE_BYTES as i64);
    }

    #[test]
    fn validates_retention_and_page_limits() {
        assert!(RetentionPolicy::new(0, 1).is_err());
        assert!(RetentionPolicy::new(MAX_RETENTION_DAYS + 1, 1).is_err());
        assert!(RetentionPolicy::new(1, 0).is_err());
        assert!(RetentionPolicy::new(1, MAX_EVENT_COUNT + 1).is_err());
        let store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        assert!(matches!(
            store.read_events_after(-1, 1),
            Err(StorageError::InvalidCursor)
        ));
        assert!(matches!(
            store.read_events_after(0, 0),
            Err(StorageError::InvalidPageSize)
        ));
        assert!(matches!(
            store.read_events_after(0, MAX_READ_PAGE_SIZE + 1),
            Err(StorageError::InvalidPageSize)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn secures_database_and_data_directory_and_rejects_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = temp_dir();
        let private_dir = root.join("private");
        create_private_data_dir(&private_dir).unwrap();
        assert_eq!(
            fs::metadata(&private_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let database = private_dir.join(DATABASE_FILE_NAME);
        drop(EventStore::open(&database, RetentionPolicy::default()).unwrap());
        assert_eq!(
            fs::metadata(&database).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let linked_db = root.join("linked.sqlite3");
        symlink(&database, &linked_db).unwrap();
        assert!(matches!(
            EventStore::open(&linked_db, RetentionPolicy::default()),
            Err(StorageError::SymlinkPath)
        ));

        let linked_dir = root.join("linked-dir");
        symlink(&private_dir, &linked_dir).unwrap();
        assert!(matches!(
            create_private_data_dir(&linked_dir),
            Err(StorageError::SymlinkPath)
        ));

        let directory_as_database = root.join("directory.sqlite3");
        fs::create_dir(&directory_as_database).unwrap();
        let directory_mode = fs::metadata(&directory_as_database)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert!(matches!(
            EventStore::open(&directory_as_database, RetentionPolicy::default()),
            Err(StorageError::InvalidDatabasePath)
        ));
        assert_eq!(
            fs::metadata(&directory_as_database)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            directory_mode
        );
        fs::remove_dir_all(root).unwrap();
    }
}
