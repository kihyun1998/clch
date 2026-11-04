//! The snapshot store: the sizes every scan recorded, kept in SQLite.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, TransactionBehavior, params};
use serde::Serialize;

use crate::catalog::Kind;
use crate::env::Env;
use crate::scan::Scan;

/// How long a store operation waits for another clch run.
const BUSY_WAIT: Duration = Duration::from_secs(10);

const SCHEMA_VERSION: u32 = 1;

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

/// A reclaimable that the latest snapshot had and a new scan did not find.
#[derive(Debug, Serialize)]
pub struct Gone {
    pub id: String,
    pub kind: Kind,
    pub path: PathBuf,
    pub previous_bytes: u64,
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
            "SELECT reclaimable_id, kind, path, size_bytes FROM reclaimable_size
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
                let path = PathBuf::from(path);
                Ok((
                    key,
                    Gone {
                        id,
                        kind,
                        path,
                        previous_bytes,
                    },
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for r in &mut scan.reclaimables {
            let key = identity(r.kind, &r.id, &r.path.display().to_string());
            r.change_bytes = previous
                .remove(&key)
                .map(|before| r.size_bytes as i64 - before.previous_bytes as i64);
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
                "INSERT INTO reclaimable_size (snapshot_id, reclaimable_id, kind, path, size_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for r in &scan.reclaimables {
                insert.execute(params![
                    snapshot_id,
                    r.id,
                    r.kind.as_str(),
                    r.path.display().to_string(),
                    r.size_bytes as i64
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
