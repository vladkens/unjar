use std::fmt;

/// An error returned by unjar.
#[derive(thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  /// A user-facing error without a lower-level source.
  #[error("{0}")]
  Message(String),
  /// An I/O operation failed.
  #[error(transparent)]
  Io(#[from] std::io::Error),
  /// A browser database operation failed.
  #[error(transparent)]
  Database(DatabaseError),
  /// Text returned by a platform service was not valid UTF-8.
  #[error(transparent)]
  Utf8(#[from] std::string::FromUtf8Error),
}

impl Error {
  pub(crate) fn database(err: rusqlite::Error) -> Self {
    Self::Database(DatabaseError(err))
  }
}

impl fmt::Debug for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(self, f)
  }
}

impl From<String> for Error {
  fn from(message: String) -> Self {
    Self::Message(message)
  }
}

impl From<&str> for Error {
  fn from(message: &str) -> Self {
    message.to_owned().into()
  }
}

/// An error from reading a browser database.
///
/// It displays the database engine's message; [`source`](std::error::Error::source) returns the
/// original error.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DatabaseError(#[source] rusqlite::Error);

/// A result returned by unjar.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
  use std::error::Error as _;

  use super::*;

  #[test]
  fn database_error_keeps_message_and_source() {
    let err = Error::database(rusqlite::Error::InvalidQuery);
    assert_eq!(err.to_string(), rusqlite::Error::InvalidQuery.to_string());
    assert!(err.source().unwrap().is::<rusqlite::Error>());
  }

  #[test]
  fn error_is_thread_safe() {
    fn check<T: Send + Sync + 'static>() {}
    check::<Error>();
  }
}
