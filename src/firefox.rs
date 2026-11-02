use std::path::Path;

use crate::cookie::Cookie;
use crate::{Error, Result};

/// Read cookies from a Firefox `cookies.sqlite` database.
///
/// Firefox stores cookie values in plain text, so no decryption is needed.
pub(crate) fn read(path: &Path) -> Result<Vec<Cookie>> {
  let db = crate::sqlite::open_copy(path)?;
  let conn = &db.conn;

  let mut stmt = conn
    .prepare("SELECT host, name, value, path, expiry, isSecure, isHttpOnly FROM moz_cookies")
    .map_err(Error::database)?;

  let rows = stmt
    .query_map([], |row| {
      Ok(Cookie {
        domain: row.get(0)?,
        name: row.get(1)?,
        value: row.get(2)?,
        path: row.get(3)?,
        expires: row.get(4)?,
        secure: row.get::<_, i64>(5)? != 0,
        http_only: row.get::<_, i64>(6)? != 0,
      })
    })
    .map_err(Error::database)?;

  let mut out = Vec::new();
  for row in rows {
    out.push(row.map_err(Error::database)?);
  }

  Ok(out)
}
