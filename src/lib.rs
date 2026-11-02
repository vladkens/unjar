#![warn(missing_docs)]

//! Read and export cookies from local browser profiles.
//!
//! `unjar` provides access to cookies stored by locally installed browsers.
//!
//! # Example
//!
//! ```no_run
//! use unjar::Browser;
//!
//! fn main() -> unjar::Result<()> {
//!   let jar = Browser::Chrome.cookies()?.domain("x.com");
//!   println!("{}", jar.to_netscape());
//!   Ok(())
//! }
//! ```

mod browser;
mod cookie;
mod error;

mod chromium;
mod firefox;
mod safari;
mod sqlite;

use browser::Kind;

pub use browser::{Browser, Profile, find_profile, profiles};
pub use cookie::{Cookie, CookieJar};
pub use error::{DatabaseError, Error, Result};

impl Browser {
  /// Load all cookies from the profile selected by default for this browser.
  pub fn cookies(&self) -> Result<CookieJar> {
    let db = self.cookie_db()?;

    let cookies = match self.kind() {
      Kind::Chromium => chromium::read(&db, *self)?,
      Kind::Firefox => firefox::read(&db)?,
      Kind::Safari => safari::read(&db)?,
    };

    Ok(CookieJar::new(cookies))
  }
}

impl Profile {
  /// Load all cookies from this profile.
  pub fn cookies(&self) -> Result<CookieJar> {
    let cookies = match self.browser().kind() {
      Kind::Chromium => chromium::read(self.cookie_db(), self.browser())?,
      Kind::Firefox => firefox::read(self.cookie_db())?,
      Kind::Safari => safari::read(self.cookie_db())?,
    };

    Ok(CookieJar::new(cookies))
  }
}
