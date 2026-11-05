//! The snapshot store: the sizes every scan recorded, kept in SQLite.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, TransactionBehavior, params};
use serde::Serialize;

use crate::catalog::Kind;
use crate::env::Env;
use crate::scan::{ChangeState, Scan, UNKNOWN_DISK};

/// How long a store operation waits for another clch run.
const BUSY_WAIT: Duration = Duration::from_secs(10);

const SCHEMA_VERSION: u32 = 3;

const SCHEMA_V1: &str = "
CREATE TABLE snapshot (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    taken_at_ms INTEGER NOT NULL
);
CREATE TABLE reclaimable_size (
    snapshot_id INTEGER NOT NULL REFERENCES snapshot(id),
    reclaimable_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, reclaimable_id, path)
);
";

const SCHEMA_V2: &str = "
ALTER TABLE reclaimable_size ADD COLUMN disk TEXT;
ALTER TABLE reclaimable_size ADD COLUMN complete INTEGER NOT NULL DEFAULT 1;
UPDATE reclaimable_size SET complete = 0;
CREATE TABLE disk_total (
    snapshot_id INTEGER NOT NULL REFERENCES snapshot(id),
    disk TEXT NOT NULL,
    free_bytes INTEGER,
    reclaimable_bytes INTEGER NOT NULL,
    complete INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, disk)
);
";

const SCHEMA_V3: &str = "
ALTER TABLE reclaimable_size ADD COLUMN gone INTEGER NOT NULL DEFAULT 0;
ALTER TABLE snapshot ADD COLUMN partial INTEGER NOT NULL DEFAULT 0;
CREATE INDEX reclaimable_size_history ON reclaimable_size (reclaimable_id, path, snapshot_id);
";

/// A reclaimable that the latest snapshot had and a new scan did not find.
#[derive(Debug, Serialize)]
pub struct Gone {
    pub id: String,
    pub kind: Kind,
    pub path: PathBuf,
    pub previous_bytes: u64,
    /// False when `previous_bytes` was a lower bound.
    pub complete: bool,
}

#[derive(Debug)]
pub enum StoreError {
    NoDataFolder,
    Io(std::io::Error),
    Sql(rusqlite::Error),
    NewerStore(u32),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NoDataFolder => write!(f, "no home folder to keep snapshots in"),
            StoreError::Io(e) => write!(f, "{e}"),
            StoreError::Sql(e) => write!(f, "{e}"),
            StoreError::NewerStore(version) => write!(
                f,
                "the snapshot store was written by a newer clch (schema {version}); update clch"
            ),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sql(e)
    }
}

/// Compares `scan` with the latest snapshot, then records it as a new one.
pub fn track(env: &Env, scan: &mut Scan) -> Result<(), StoreError> {
    let mut store = Store::in_data_folder(env)?;
    store.compare(scan)?;
    store.record(scan)
}

/// Like `track` for the scan after a cleanup, where only what the cleanup itself
/// removed counts as settled; anything else that went missing stays to be
/// reported by the next scan.
pub fn track_after_cleanup(
    env: &Env,
    scan: &mut Scan,
    cleaned: &[(String, PathBuf)],
) -> Result<(), StoreError> {
    let mut store = Store::in_data_folder(env)?;
    store.compare(scan)?;
    scan.gone.retain(|g| {
        cleaned
            .iter()
            .any(|(id, path)| *id == g.id && *path == g.path)
    });
    store.record(scan)
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn in_data_folder(env: &Env) -> Result<Store, StoreError> {
        let dir = env.data_dir().ok_or(StoreError::NoDataFolder)?;
        Store::open(&dir)
    }

    /// Opens the store in `dir`, creating or upgrading it as needed.
    pub fn open(dir: &Path) -> Result<Store, StoreError> {
        std::fs::create_dir_all(dir).map_err(StoreError::Io)?;
        let mut conn = Connection::open(dir.join("clch.db"))?;
        conn.busy_timeout(BUSY_WAIT)?;
        while_busy(|| conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(())))?;
        migrate(&mut conn).map_err(|e| match e {
            Migration::Sql(e) => StoreError::Sql(e),
            Migration::Newer(version) => StoreError::NewerStore(version),
        })?;
        Ok(Store { conn })
    }

    /// Fills each reclaimable's change from the last size recorded for it, and
    /// lists what was last seen, should be found by `scan`, and was not.
    pub fn compare(&self, scan: &mut Scan) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT r.snapshot_id, r.reclaimable_id, r.kind, r.path, r.size_bytes, r.complete,
                    r.disk, r.gone
             FROM reclaimable_size r
             JOIN (SELECT reclaimable_id, path, max(snapshot_id) AS latest
                   FROM reclaimable_size GROUP BY reclaimable_id, path) l
               ON r.reclaimable_id = l.reclaimable_id AND r.path = l.path
              AND r.snapshot_id = l.latest",
        )?;
        let rows = stmt.query_map([], |row| {
            let kind = row.get::<_, String>(2)?.parse::<Kind>().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
            })?;
            Ok(LastSeen {
                snapshot_id: row.get(0)?,
                id: row.get(1)?,
                kind,
                path: row.get(3)?,
                size_bytes: row.get(4)?,
                complete: row.get(5)?,
                disk: row.get(6)?,
                gone: row.get(7)?,
            })
        })?;
        let mut last_seen: HashMap<(String, Option<String>), LastSeen> = HashMap::new();
        for row in rows {
            let row = row?;
            let key = identity(row.kind, &row.id, &row.path);
            match last_seen.get(&key) {
                Some(newer) if newer.snapshot_id > row.snapshot_id => {}
                _ => {
                    last_seen.insert(key, row);
                }
            }
        }
        for r in &mut scan.reclaimables {
            let key = identity(r.kind, &r.id, &r.path.display().to_string());
            match last_seen.remove(&key) {
                None => r.change_state = ChangeState::New,
                Some(before) if before.gone => r.change_state = ChangeState::New,
                Some(before) if before.complete && r.complete => {
                    r.change_state = ChangeState::Known;
                    r.change_bytes = Some(r.size_bytes as i64 - before.size_bytes);
                }
                Some(_) => r.change_state = ChangeState::Unknown,
            }
        }
        scan.gone = last_seen
            .into_values()
            .filter(|row| !row.gone && should_be_found(row, scan))
            .map(|row| Gone {
                id: row.id,
                kind: row.kind,
                path: PathBuf::from(row.path),
                previous_bytes: row.size_bytes as u64,
                complete: row.complete,
            })
            .collect();
        scan.gone
            .sort_by(|a, b| (&a.id, &a.path).cmp(&(&b.id, &b.path)));
        scan.history_available = true;
        Ok(())
    }

    /// Records `scan` as a new snapshot.
    pub fn record(&mut self, scan: &Scan) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO snapshot (taken_at_ms, partial) VALUES (?1, ?2)",
            params![scan.taken_at_ms, scan.partial],
        )?;
        let snapshot_id = tx.last_insert_rowid();
        {
            let mut insert = tx.prepare(
                "INSERT INTO reclaimable_size
                    (snapshot_id, reclaimable_id, kind, path, size_bytes, disk, complete)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for r in &scan.reclaimables {
                insert.execute(params![
                    snapshot_id,
                    r.id,
                    r.kind.as_str(),
                    r.path.display().to_string(),
                    r.size_bytes as i64,
                    r.disk,
                    r.complete
                ])?;
            }
            let mut insert = tx.prepare(
                "INSERT INTO reclaimable_size
                    (snapshot_id, reclaimable_id, kind, path, size_bytes, complete, gone)
                 VALUES (?1, ?2, ?3, ?4, 0, 1, 1)",
            )?;
            for g in &scan.gone {
                insert.execute(params![
                    snapshot_id,
                    g.id,
                    g.kind.as_str(),
                    g.path.display().to_string()
                ])?;
            }
            let mut insert = tx.prepare(
                "INSERT INTO disk_total
                    (snapshot_id, disk, free_bytes, reclaimable_bytes, complete)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for d in &scan.disks {
                insert.execute(params![
                    snapshot_id,
                    d.disk,
                    d.free_bytes.map(|b| b as i64),
                    d.reclaimable_bytes as i64,
                    d.complete
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

/// The last row recorded for a reclaimable.
struct LastSeen {
    snapshot_id: i64,
    id: String,
    kind: Kind,
    path: String,
    size_bytes: i64,
    complete: bool,
    disk: Option<String>,
    gone: bool,
}

/// Whether `scan` should have found what `row` recorded, were it still there.
fn should_be_found(row: &LastSeen, scan: &Scan) -> bool {
    match row.kind {
        Kind::ToolCache => {
            !Path::new(&row.path).is_dir()
                && row
                    .disk
                    .as_deref()
                    .is_none_or(|disk| disk == UNKNOWN_DISK || Path::new(disk).is_dir())
        }
        Kind::ProjectArtifact => scan.covers(Path::new(&row.path)),
    }
}

/// What makes a reclaimable the same one across snapshots: a tool cache is one
/// per machine wherever it lives, a project artifact is the one at its path.
fn identity(kind: Kind, id: &str, path: &str) -> (String, Option<String>) {
    match kind {
        Kind::ToolCache => (id.to_string(), None),
        Kind::ProjectArtifact => (id.to_string(), Some(path.to_string())),
    }
}

enum Migration {
    Sql(rusqlite::Error),
    Newer(u32),
}

impl From<rusqlite::Error> for Migration {
    fn from(e: rusqlite::Error) -> Self {
        Migration::Sql(e)
    }
}

fn migrate(conn: &mut Connection) -> Result<(), Migration> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(Migration::Newer(version));
    }
    if version < 1 {
        tx.execute_batch(SCHEMA_V1)?;
    }
    if version < 2 {
        tx.execute_batch(SCHEMA_V2)?;
    }
    if version < 3 {
        tx.execute_batch(SCHEMA_V3)?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

/// Runs `operation` again for as long as another run holds the database, up
/// to `BUSY_WAIT`.
fn while_busy<T>(mut operation: impl FnMut() -> rusqlite::Result<T>) -> rusqlite::Result<T> {
    let deadline = Instant::now() + BUSY_WAIT;
    loop {
        match operation() {
            Err(e)
                if matches!(
                    e.sqlite_error_code(),
                    Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
                ) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => return result,
        }
    }
}
