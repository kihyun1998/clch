//! The snapshot store: the sizes every scan recorded, kept in SQLite.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, TransactionBehavior, params};
use serde::Serialize;

use crate::catalog::Kind;
use crate::env::Env;
use crate::scan::{ChangeState, Scan};

/// How long a store operation waits for another clch run.
const BUSY_WAIT: Duration = Duration::from_secs(10);

const SCHEMA_VERSION: u32 = 2;

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

/// Records `scan` as a new snapshot without comparing it.
pub fn record(env: &Env, scan: &Scan) -> Result<(), StoreError> {
    Store::in_data_folder(env)?.record(scan)
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

    /// Fills each reclaimable's `change_bytes` from the latest snapshot and
    /// lists what that snapshot had and `scan` did not find.
    pub fn compare(&self, scan: &mut Scan) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT reclaimable_id, kind, path, size_bytes, complete FROM reclaimable_size
             WHERE snapshot_id = (SELECT max(id) FROM snapshot)",
        )?;
        let mut previous: HashMap<(String, Option<String>), Gone> = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let kind = row.get::<_, String>(1)?.parse::<Kind>().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?;
                let path: String = row.get(2)?;
                let key = identity(kind, &id, &path);
                let previous_bytes = row.get::<_, i64>(3)? as u64;
                let complete: bool = row.get(4)?;
                let path = PathBuf::from(path);
                let gone = Gone {
                    id,
                    kind,
                    path,
                    previous_bytes,
                    complete,
                };
                Ok((key, gone))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for r in &mut scan.reclaimables {
            let key = identity(r.kind, &r.id, &r.path.display().to_string());
            match previous.remove(&key) {
                None => r.change_state = ChangeState::New,
                Some(before) if before.complete && r.complete => {
                    r.change_state = ChangeState::Known;
                    r.change_bytes = Some(r.size_bytes as i64 - before.previous_bytes as i64);
                }
                Some(_) => r.change_state = ChangeState::Unknown,
            }
        }
        scan.gone = previous.into_values().collect();
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
            "INSERT INTO snapshot (taken_at_ms) VALUES (?1)",
            params![scan.taken_at_ms],
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

/// What makes a reclaimable the same one across snapshots: a tool cache is one
/// per machine wherever it lives.
fn identity(kind: Kind, id: &str, _path: &str) -> (String, Option<String>) {
    match kind {
        Kind::ToolCache => (id.to_string(), None),
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
