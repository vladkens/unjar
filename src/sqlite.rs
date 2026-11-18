use std::ffi::OsString;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;

use crate::{Error, Result};

/// A SQLite connection, plus an optional temp copy cleaned up when dropped.
pub(crate) struct Db {
  pub conn: Connection,
  _temp: Option<TempCopy>,
}

/// Open a browser cookie database read-only, keeping SQLite's locking and WAL so the browser
/// writes on; one locked, as on Windows, is read from a temporary copy with its `-wal` and `-shm`.
pub(crate) fn open(path: &Path) -> Result<Db> {
  match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
    Ok(conn) => Ok(Db { conn, _temp: None }),
    Err(_) => open_copy(path),
  }
}

pub(crate) fn open_copy(path: &Path) -> Result<Db> {
  let temp = TempCopy::of(path)?;
  // The copy is ours alone, so a normal read-write open is safe and lets SQLite
  // replay the copied WAL to surface the latest committed cookies.
  let conn = Connection::open(&temp.db).map_err(Error::database)?;
  Ok(Db { conn, _temp: Some(temp) })
}

struct TempCopy {
  /// Kept for its Drop, which deletes the directory.
  _dir: TempDir,
  db: PathBuf,
}

impl TempCopy {
  fn of(path: &Path) -> Result<Self> {
    let dir = tempfile::Builder::new().prefix("unjar-").tempdir()?;

    let name = path.file_name().ok_or("invalid database path")?;
    let db = dir.path().join(name);
    std::fs::copy(path, &db)?;

    // Copy the WAL/SHM sidecars if present so no committed writes are missed.
    for suffix in ["-wal", "-shm"] {
      let src = with_suffix(path.as_os_str(), suffix);
      if Path::new(&src).exists() {
        std::fs::copy(&src, with_suffix(db.as_os_str(), suffix))?;
      }
    }

    Ok(Self { _dir: dir, db })
  }
}

fn with_suffix(base: &std::ffi::OsStr, suffix: &str) -> PathBuf {
  let mut s = OsString::from(base);
  s.push(suffix);
  PathBuf::from(s)
}
