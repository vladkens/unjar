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

/// Open a browser cookie database for reading without disturbing the live file.
///
/// A plain read-only open is tried first: it keeps SQLite's locking and WAL
/// handling intact, so the browser can keep writing and we still read a
/// consistent snapshot. If that fails (e.g. the file is exclusively locked, as
/// on Windows), the database and its `-wal`/`-shm` sidecars are copied to a temp
/// directory and the copy is opened instead.
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
  #[allow(dead_code, reason = "keeps the temporary directory alive until Drop")]
  dir: TempDir,
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

    Ok(Self { dir, db })
  }
}

fn with_suffix(base: &std::ffi::OsStr, suffix: &str) -> PathBuf {
  let mut s = OsString::from(base);
  s.push(suffix);
  PathBuf::from(s)
}
