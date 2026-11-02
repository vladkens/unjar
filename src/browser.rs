use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::{Error, Result};

/// A browser whose local profiles unjar can discover.
///
/// Chromium-family variants share a cookie format but use distinct profile locations and decryption
/// credentials.
#[allow(missing_docs)]
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Browser {
  Chrome,
  Chromium,
  Edge,
  Brave,
  Vivaldi,
  Opera,
  Arc,
  Helium,
  Firefox,
  Safari,
}

/// A discovered browser profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
  browser: Browser,
  id: String,
  name: String,
  is_default: bool,
  path: PathBuf,
  cookie_db: PathBuf,
}

/// Which storage engine a browser uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
  Chromium,
  Firefox,
  Safari,
}

const ALL: [Browser; 10] = [
  Browser::Chrome,
  Browser::Chromium,
  Browser::Edge,
  Browser::Brave,
  Browser::Vivaldi,
  Browser::Opera,
  Browser::Arc,
  Browser::Helium,
  Browser::Firefox,
  Browser::Safari,
];

impl Browser {
  /// Iterate over all browsers known to this version of unjar.
  ///
  /// # Examples
  ///
  /// ```
  /// for browser in unjar::Browser::all() {
  ///   println!("{browser}");
  /// }
  /// ```
  pub fn all() -> impl Iterator<Item = Browser> {
    ALL.into_iter()
  }

  pub(crate) fn kind(&self) -> Kind {
    match self {
      Browser::Firefox => Kind::Firefox,
      Browser::Safari => Kind::Safari,
      _ => Kind::Chromium,
    }
  }

  pub(crate) fn cookie_db(&self) -> Result<PathBuf> {
    match self.kind() {
      Kind::Chromium | Kind::Firefox | Kind::Safari => {
        default_profile(*self).map(|profile| profile.cookie_db)
      }
    }
  }

  /// Select a profile in this browser by ID, name, unique partial match, or explicit path.
  ///
  /// The browser is supplied by `self`, not encoded in `selector`. An explicit path may point to a
  /// profile directory or its cookie database.
  ///
  /// # Examples
  ///
  /// ```no_run
  /// use unjar::Browser;
  ///
  /// let profile = Browser::Firefox.find_profile("abc")?;
  /// println!("{}", profile.path().display());
  /// # Ok::<(), unjar::Error>(())
  /// ```
  pub fn find_profile(&self, selector: &str) -> Result<Profile> {
    let candidates: Vec<_> =
      profiles().into_iter().filter(|profile| profile.browser == *self).collect();

    match select_discovered(&candidates, selector)? {
      Some(profile) => Ok(profile),
      None if looks_like_path(selector) => {
        let path = Path::new(selector);
        match candidates
          .iter()
          .find(|profile| same_path(path, &profile.path) || same_path(path, &profile.cookie_db))
        {
          Some(profile) => Ok(profile.clone()),
          None => Profile::from_path(*self, path),
        }
      }
      None => Err(format!("{self}: profile '{selector}' not found; run `unjar list`").into()),
    }
  }
}

impl Profile {
  /// Build a profile from a profile directory or a cookie database.
  pub fn from_path(browser: Browser, path: impl AsRef<Path>) -> Result<Self> {
    let path = path.as_ref();
    let (profile_path, cookie_db) = match browser.kind() {
      Kind::Chromium if path.is_file() => {
        if path.file_name().and_then(|name| name.to_str()) != Some("Cookies") {
          return Err(format!("{}: expected a Cookies database", path.display()).into());
        }
        let parent = path.parent().unwrap_or(path);
        let profile = if parent.file_name().and_then(|name| name.to_str()) == Some("Network") {
          parent.parent().unwrap_or(parent)
        } else {
          parent
        };
        (profile.to_path_buf(), path.to_path_buf())
      }
      Kind::Chromium => {
        let db = profile_cookie_db(path).ok_or_else(|| {
          Error::from(format!(
            "{}: Cookies not found (expected Cookies or Network/Cookies)",
            path.display()
          ))
        })?;
        (path.to_path_buf(), db)
      }
      Kind::Firefox if path.is_file() => {
        if path.file_name().and_then(|name| name.to_str()) != Some("cookies.sqlite") {
          return Err(format!("{}: expected a cookies.sqlite database", path.display()).into());
        }
        (path.parent().unwrap_or(path).to_path_buf(), path.to_path_buf())
      }
      Kind::Firefox => {
        let db = path.join("cookies.sqlite");
        if !db.is_file() {
          return Err(format!("{}: cookies.sqlite not found", path.display()).into());
        }
        (path.to_path_buf(), db)
      }
      Kind::Safari if path.is_file() => {
        if path.file_name().and_then(|name| name.to_str()) != Some("Cookies.binarycookies") {
          return Err(format!("{}: expected a Cookies.binarycookies file", path.display()).into());
        }
        let parent = path.parent().unwrap_or(path);
        let profile = if parent.file_name().and_then(|name| name.to_str()) == Some("Cookies") {
          parent.parent().unwrap_or(parent)
        } else {
          parent
        };
        (profile.to_path_buf(), path.to_path_buf())
      }
      Kind::Safari => {
        let db = first_existing(&[
          path.join("Cookies").join("Cookies.binarycookies"),
          path.join("Cookies.binarycookies"),
        ])
        .ok_or_else(|| {
          Error::from(format!(
            "{}: Cookies.binarycookies not found (expected Cookies.binarycookies or Cookies/Cookies.binarycookies)",
            path.display()
          ))
        })?;
        (path.to_path_buf(), db)
      }
    };

    let id =
      profile_path.file_name().and_then(|name| name.to_str()).unwrap_or("custom").to_string();
    Ok(Self {
      browser,
      name: if browser.kind() == Kind::Chromium {
        profile_name(&profile_path).unwrap_or_else(|| id.clone())
      } else {
        id.clone()
      },
      id,
      is_default: false,
      path: profile_path,
      cookie_db,
    })
  }

  /// Return the browser that owns this profile.
  pub fn browser(&self) -> Browser {
    self.browser
  }

  /// Return the browser-local profile identifier.
  pub fn id(&self) -> &str {
    &self.id
  }

  /// Return the profile's display name.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Return the profile directory.
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Return the cookie database path.
  pub fn cookie_db(&self) -> &Path {
    &self.cookie_db
  }

  /// Return whether unjar selects this profile when no profile is specified.
  pub fn is_default(&self) -> bool {
    self.is_default
  }
}

impl fmt::Display for Browser {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      Browser::Chrome => "chrome",
      Browser::Chromium => "chromium",
      Browser::Edge => "edge",
      Browser::Brave => "brave",
      Browser::Vivaldi => "vivaldi",
      Browser::Opera => "opera",
      Browser::Arc => "arc",
      Browser::Helium => "helium",
      Browser::Firefox => "firefox",
      Browser::Safari => "safari",
    };
    f.write_str(s)
  }
}

impl FromStr for Browser {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self> {
    match s.to_ascii_lowercase().as_str() {
      "chrome" => Ok(Browser::Chrome),
      "chromium" => Ok(Browser::Chromium),
      "edge" => Ok(Browser::Edge),
      "brave" => Ok(Browser::Brave),
      "vivaldi" => Ok(Browser::Vivaldi),
      "opera" => Ok(Browser::Opera),
      "arc" => Ok(Browser::Arc),
      "helium" => Ok(Browser::Helium),
      "firefox" | "ff" => Ok(Browser::Firefox),
      "safari" => Ok(Browser::Safari),
      other => Err(format!("unknown browser: {other}").into()),
    }
  }
}

/// Discover browser profiles in known locations.
///
/// # Examples
///
/// ```no_run
/// for profile in unjar::profiles() {
///   println!("{}: {}", profile.browser(), profile.path().display());
/// }
/// ```
pub fn profiles() -> Vec<Profile> {
  let mut profiles = Vec::new();

  for browser in Browser::all().filter(|browser| browser.kind() == Kind::Chromium) {
    if let Some(root) = chromium_root(browser) {
      profiles.extend(profiles_in_root(browser, &root));
    }
  }
  if let Some(root) = firefox_root() {
    profiles.extend(firefox_profiles_in_root(&root));
  }
  profiles.extend(safari_profiles());

  profiles.sort_by(|a, b| {
    a.browser
      .cmp(&b.browser)
      .then_with(|| b.is_default.cmp(&a.is_default))
      .then_with(|| a.id.cmp(&b.id))
  });
  profiles
}

/// Select a discovered profile by ID, name, unique partial match, or known path.
///
/// `selector` does not include a browser name. Use [`Browser::find_profile`] to constrain the
/// lookup to one browser; that method also accepts an explicit profile directory or cookie database
/// path.
///
/// # Examples
///
/// ```no_run
/// use unjar::{Browser, find_profile};
///
/// // Assume `unjar list` shows Chrome's `Profile 2` and a Firefox profile named `Work`.
/// let by_id = find_profile("Profile 2")?;
/// assert_eq!(by_id.browser(), Browser::Chrome);
///
/// let by_name = find_profile("Work")?;
/// assert_eq!(by_name.browser(), Browser::Firefox);
///
/// let by_path = find_profile("/Users/alice/Library/Application Support/Firefox/Profiles/abc.work")?;
/// assert_eq!(by_path.browser(), Browser::Firefox);
/// # Ok::<(), unjar::Error>(())
/// ```
pub fn find_profile(selector: &str) -> Result<Profile> {
  let candidates = profiles();
  match select_discovered(&candidates, selector)? {
    Some(profile) => Ok(profile),
    None if looks_like_path(selector) => {
      let path = Path::new(selector);
      let found: Vec<_> = candidates
        .iter()
        .filter(|profile| same_path(path, &profile.path) || same_path(path, &profile.cookie_db))
        .cloned()
        .collect();

      match found.as_slice() {
        [profile] => Ok(profile.clone()),
        [] if path.exists() => Err(
          format!("cannot determine the browser for '{}'; pass --browser", path.display()).into(),
        ),
        [] => Err(format!("{}: path does not exist", path.display()).into()),
        _ => Err(ambiguous_error(selector, &found)),
      }
    }
    None => Err(format!("profile '{selector}' not found; run `unjar list`").into()),
  }
}

fn select_discovered(candidates: &[Profile], selector: &str) -> Result<Option<Profile>> {
  let selector_lower = selector.to_lowercase();
  let by_id: Vec<_> = candidates
    .iter()
    .filter(|profile| profile.id.to_lowercase() == selector_lower)
    .cloned()
    .collect();
  match by_id.as_slice() {
    [profile] => return Ok(Some(profile.clone())),
    [] => {}
    _ => return Err(ambiguous_error(selector, &by_id)),
  }

  let by_name: Vec<_> = candidates
    .iter()
    .filter(|profile| profile.name.to_lowercase() == selector_lower)
    .cloned()
    .collect();
  match by_name.as_slice() {
    [profile] => return Ok(Some(profile.clone())),
    [] => {}
    _ => return Err(ambiguous_error(selector, &by_name)),
  }

  if selector_lower.is_empty() {
    return Ok(None);
  }

  let partial: Vec<_> = candidates
    .iter()
    .filter(|profile| {
      profile.id.to_lowercase().contains(&selector_lower)
        || profile.name.to_lowercase().contains(&selector_lower)
    })
    .cloned()
    .collect();
  match partial.as_slice() {
    [profile] => Ok(Some(profile.clone())),
    [] => Ok(None),
    _ => Err(ambiguous_error(selector, &partial)),
  }
}

fn ambiguous_error(selector: &str, profiles: &[Profile]) -> Error {
  let choices = profiles
    .iter()
    .map(|profile| format!("{} / {}", profile.browser, profile.id))
    .collect::<Vec<_>>()
    .join(", ");
  let one_browser = profiles
    .first()
    .is_some_and(|first| profiles.iter().all(|profile| profile.browser == first.browser));
  let hint = if one_browser { "use a profile ID or path" } else { "pass --browser or use a path" };
  format!("profile '{selector}' is ambiguous ({choices}); {hint}").into()
}

fn looks_like_path(selector: &str) -> bool {
  let path = Path::new(selector);
  path.is_absolute() || path.components().count() > 1 || path.exists()
}

fn same_path(a: &Path, b: &Path) -> bool {
  a == b
    || match (a.canonicalize(), b.canonicalize()) {
      (Ok(a), Ok(b)) => a == b,
      _ => false,
    }
}

pub(crate) fn default_profile(browser: Browser) -> Result<Profile> {
  let candidates: Vec<_> =
    profiles().into_iter().filter(|profile| profile.browser == browser).collect();

  let defaults: Vec<_> = candidates.iter().filter(|profile| profile.is_default).collect();
  match defaults.as_slice() {
    [profile] => return Ok((*profile).clone()),
    [] => {}
    _ => {
      return Err(
        format!("{browser}: multiple default profiles found; select one by name or ID").into(),
      );
    }
  }

  match candidates.as_slice() {
    [profile] => Ok(profile.clone()),
    [] => Err(format!("{browser}: cookie database not found").into()),
    _ => Err(format!("{browser}: multiple profiles found; select one by name or ID").into()),
  }
}

fn profiles_in_root(browser: Browser, root: &Path) -> Vec<Profile> {
  let Ok(entries) = std::fs::read_dir(root) else {
    return Vec::new();
  };

  let mut profiles: Vec<_> = entries
    .filter_map(std::result::Result::ok)
    .filter_map(|entry| {
      let file_type = entry.file_type().ok()?;
      let directory = entry.file_name().to_string_lossy().into_owned();
      // `Network` holds the cookies of a profile kept in the root, not a profile of its own.
      if !file_type.is_dir() || file_type.is_symlink() || directory == "Network" {
        return None;
      }
      let path = entry.path();
      let cookie_db = profile_cookie_db(&path)?;
      Some(Profile {
        browser,
        id: directory.clone(),
        name: profile_name(&path).unwrap_or_else(|| directory.clone()),
        is_default: false,
        path,
        cookie_db,
      })
    })
    .collect();

  // Some browsers, such as Opera, keep their only profile in the user-data directory itself; treat
  // it as `Default` unless a `Default` subdirectory already holds one.
  if !profiles.iter().any(|profile| profile.id == "Default")
    && let Some(cookie_db) = profile_cookie_db(root)
  {
    profiles.push(Profile {
      browser,
      id: "Default".into(),
      name: profile_name(root).unwrap_or_else(|| "Default".into()),
      is_default: false,
      path: root.to_path_buf(),
      cookie_db,
    });
  }

  let default_id = chromium_default_profile_id(root)
    .filter(|id| profiles.iter().any(|profile| profile.id == *id))
    .or_else(|| profiles.iter().any(|profile| profile.id == "Default").then(|| "Default".into()))
    .or_else(|| (profiles.len() == 1).then(|| profiles[0].id.clone()));
  if let Some(default_id) = default_id
    && let Some(profile) = profiles.iter_mut().find(|profile| profile.id == default_id)
  {
    profile.is_default = true;
  }

  profiles
}

fn profile_cookie_db(profile: &Path) -> Option<PathBuf> {
  first_existing(&[profile.join("Network").join("Cookies"), profile.join("Cookies")])
}

fn profile_name(profile: &Path) -> Option<String> {
  let data = std::fs::read_to_string(profile.join("Preferences")).ok()?;
  let value: serde_json::Value = serde_json::from_str(&data).ok()?;
  value.get("profile")?.get("name")?.as_str().filter(|name| !name.is_empty()).map(str::to_owned)
}

fn chromium_default_profile_id(root: &Path) -> Option<String> {
  let data = std::fs::read_to_string(root.join("Local State")).ok()?;
  let value: serde_json::Value = serde_json::from_str(&data).ok()?;
  value.get("profile")?.get("last_used")?.as_str().filter(|id| !id.is_empty()).map(str::to_owned)
}

/// Return the first path in `candidates` that exists.
fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
  candidates.iter().find(|p| p.is_file()).cloned()
}

/// Base user-data directory for a Chromium-family browser.
fn chromium_root(browser: Browser) -> Option<PathBuf> {
  #[cfg(target_os = "macos")]
  let base = dirs::config_dir()?;
  #[cfg(target_os = "linux")]
  let base = match browser {
    Browser::Chrome => std::env::var_os("CHROME_CONFIG_HOME"),
    // https://github.com/imputnet/helium-linux/blob/main/patches/helium/linux/change-chromium-branding.patch
    Browser::Helium => std::env::var_os("HELIUM_CONFIG_HOME"),
    _ => None,
  }
  .map(PathBuf::from)
  .or_else(dirs::config_dir)?;
  #[cfg(target_os = "windows")]
  let base = if browser == Browser::Opera { dirs::config_dir()? } else { dirs::data_local_dir()? };

  #[cfg(target_os = "macos")]
  let rel: &[&str] = match browser {
    Browser::Chrome => &["Google", "Chrome"],
    Browser::Chromium => &["Chromium"],
    Browser::Edge => &["Microsoft Edge"],
    Browser::Brave => &["BraveSoftware", "Brave-Browser"],
    Browser::Vivaldi => &["Vivaldi"],
    Browser::Opera => &["com.operasoftware.Opera"],
    Browser::Arc => &["Arc", "User Data"],
    Browser::Helium => &["net.imput.helium"],
    _ => return None,
  };
  #[cfg(target_os = "linux")]
  let rel: &[&str] = match browser {
    Browser::Chrome => &["google-chrome"],
    Browser::Chromium => &["chromium"],
    Browser::Edge => &["microsoft-edge"],
    Browser::Brave => &["BraveSoftware", "Brave-Browser"],
    Browser::Vivaldi => &["vivaldi"],
    Browser::Opera => &["opera"],
    Browser::Helium => &["net.imput.helium"],
    _ => return None,
  };
  #[cfg(target_os = "windows")]
  let rel: &[&str] = match browser {
    Browser::Chrome => &["Google", "Chrome", "User Data"],
    Browser::Chromium => &["Chromium", "User Data"],
    Browser::Edge => &["Microsoft", "Edge", "User Data"],
    Browser::Brave => &["BraveSoftware", "Brave-Browser", "User Data"],
    Browser::Vivaldi => &["Vivaldi", "User Data"],
    Browser::Opera => &["Opera Software", "Opera Stable"],
    Browser::Helium => &["imput", "Helium", "User Data"],
    _ => return None,
  };

  let mut root = base;
  for part in rel {
    root.push(part);
  }

  Some(root)
}

fn firefox_root() -> Option<PathBuf> {
  #[cfg(target_os = "macos")]
  let root = dirs::config_dir()?.join("Firefox");
  #[cfg(target_os = "linux")]
  let root = dirs::home_dir()?.join(".mozilla").join("firefox");
  #[cfg(target_os = "windows")]
  let root = dirs::config_dir()?.join("Mozilla").join("Firefox");

  Some(root)
}

fn firefox_profiles_in_root(root: &Path) -> Vec<Profile> {
  let contents = std::fs::read_to_string(root.join("profiles.ini")).unwrap_or_default();
  let sections = parse_ini(&contents);

  let mut default_paths: Vec<PathBuf> = sections
    .iter()
    .filter(|(section, _)| section.starts_with("Install"))
    .filter_map(|(_, values)| values.get("Default"))
    .map(|path| root.join(path))
    .collect();
  if let Ok(contents) = std::fs::read_to_string(root.join("installs.ini")) {
    default_paths.extend(
      parse_ini(&contents)
        .into_iter()
        .filter_map(|(_, values)| values.get("Default").cloned())
        .map(|path| root.join(path)),
    );
  }

  let mut profiles = firefox_profile_group_profiles(root, &default_paths);
  let legacy_profiles = sections
    .into_iter()
    .filter(|(section, _)| section.starts_with("Profile"))
    .filter_map(|(_, values)| {
      let name = values.get("Name")?.clone();
      let configured_path = values.get("Path")?;
      let path = if values.get("IsRelative").is_none_or(|value| value != "0") {
        root.join(configured_path)
      } else {
        PathBuf::from(configured_path)
      };
      let cookie_db = path.join("cookies.sqlite");
      if !cookie_db.is_file() {
        return None;
      }

      let is_default = if default_paths.is_empty() {
        values.get("Default").is_some_and(|value| value == "1")
      } else {
        default_paths.iter().any(|default| same_path(default, &path))
      };
      Some(Profile {
        browser: Browser::Firefox,
        id: name.clone(),
        name,
        is_default,
        path,
        cookie_db,
      })
    })
    .collect::<Vec<_>>();

  for profile in legacy_profiles {
    if !profiles.iter().any(|candidate| same_path(&candidate.path, &profile.path)) {
      profiles.push(profile);
    }
  }

  profiles
}

fn firefox_profile_group_profiles(root: &Path, default_paths: &[PathBuf]) -> Vec<Profile> {
  let Ok(entries) = std::fs::read_dir(root.join("Profile Groups")) else {
    return Vec::new();
  };

  let mut profiles = Vec::new();
  for database in entries.filter_map(std::result::Result::ok).map(|entry| entry.path()) {
    if database.extension().and_then(|extension| extension.to_str()) != Some("sqlite") {
      continue;
    }
    let Ok(db) = crate::sqlite::open_copy(&database) else {
      continue;
    };
    let Ok(mut statement) = db.conn.prepare("SELECT path, name FROM Profiles") else {
      continue;
    };
    let Ok(rows) =
      statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
    else {
      continue;
    };

    for row in rows.filter_map(std::result::Result::ok) {
      let (configured_path, name) = row;
      let path = PathBuf::from(&configured_path);
      let path = if path.is_absolute() { path } else { root.join(path) };
      let cookie_db = path.join("cookies.sqlite");
      if !cookie_db.is_file()
        || profiles.iter().any(|profile: &Profile| same_path(&profile.path, &path))
      {
        continue;
      }

      profiles.push(Profile {
        browser: Browser::Firefox,
        id: name.clone(),
        name,
        is_default: default_paths.iter().any(|default| same_path(default, &path)),
        path,
        cookie_db,
      });
    }
  }

  profiles
}

fn parse_ini(contents: &str) -> Vec<(String, HashMap<String, String>)> {
  let mut sections = Vec::new();

  for line in contents.lines().map(str::trim) {
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
      continue;
    }
    if let Some(section) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
      sections.push((section.to_string(), HashMap::new()));
    } else if let Some((key, value)) = line.split_once('=')
      && let Some((_, values)) = sections.last_mut()
    {
      values.insert(key.trim().to_string(), value.trim().to_string());
    }
  }

  sections
}

fn safari_profiles() -> Vec<Profile> {
  #[cfg(target_os = "macos")]
  {
    let Some(home) = dirs::home_dir() else {
      return Vec::new();
    };
    let container = home.join("Library/Containers/com.apple.Safari/Data/Library");
    let legacy_db = home.join("Library/Cookies/Cookies.binarycookies");
    safari_profiles_in_root(&container, &legacy_db)
  }
  #[cfg(not(target_os = "macos"))]
  Vec::new()
}

fn safari_profiles_in_root(container: &Path, legacy_db: &Path) -> Vec<Profile> {
  let modern_db = container.join("Cookies").join("Cookies.binarycookies");
  let default_db = first_existing(&[modern_db, legacy_db.to_path_buf()]);
  let mut profiles = Vec::new();

  if let Some(cookie_db) = default_db {
    let path = if cookie_db.starts_with(container) {
      container.to_path_buf()
    } else {
      cookie_db.parent().and_then(Path::parent).unwrap_or(legacy_db).to_path_buf()
    };
    profiles.push(Profile {
      browser: Browser::Safari,
      id: "default".into(),
      name: "default".into(),
      is_default: true,
      path,
      cookie_db,
    });
  }

  let named = safari_named_profiles_from_db(container)
    .unwrap_or_else(|| safari_named_profiles_from_directories(container));
  for (uuid, title) in named {
    let store = container.join("WebKit").join("WebsiteDataStore").join(uuid.to_ascii_lowercase());
    let cookie_db = store.join("Cookies").join("Cookies.binarycookies");
    if !cookie_db.is_file() {
      continue;
    }

    let fallback = format!("profile-{}", uuid[..8].to_ascii_lowercase());
    let name = title.trim();
    let name = if name.is_empty() { fallback } else { name.to_owned() };
    let id = unique_profile_id(&profiles, &name);
    profiles.push(Profile {
      browser: Browser::Safari,
      id: id.clone(),
      name: id,
      is_default: false,
      path: store,
      cookie_db,
    });
  }

  profiles
}

fn safari_named_profiles_from_db(container: &Path) -> Option<Vec<(String, String)>> {
  let path = container.join("Safari").join("SafariTabs.db");
  let db = crate::sqlite::open_copy(&path).ok()?;
  let mut statement = db
    .conn
    .prepare(
      "SELECT external_uuid, COALESCE(title, '') FROM bookmarks \
       WHERE subtype = 2 AND external_uuid != 'DefaultProfile'",
    )
    .ok()?;
  let rows =
    statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).ok()?;

  Some(rows.filter_map(std::result::Result::ok).filter(|(uuid, _)| is_uuid(uuid)).collect())
}

fn safari_named_profiles_from_directories(container: &Path) -> Vec<(String, String)> {
  let Ok(entries) = std::fs::read_dir(container.join("Safari").join("Profiles")) else {
    return Vec::new();
  };
  entries
    .filter_map(std::result::Result::ok)
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir() && !kind.is_symlink()))
    .filter_map(|entry| entry.file_name().into_string().ok())
    .filter(|uuid| is_uuid(uuid))
    .map(|uuid| (uuid, String::new()))
    .collect()
}

fn is_uuid(value: &str) -> bool {
  let mut parts = value.split('-');
  [8, 4, 4, 4, 12].into_iter().all(|len| {
    parts
      .next()
      .is_some_and(|part| part.len() == len && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
  }) && parts.next().is_none()
}

fn unique_profile_id(profiles: &[Profile], name: &str) -> String {
  if profiles.iter().all(|profile| profile.id != name) {
    return name.to_owned();
  }
  for suffix in 2.. {
    let candidate = format!("{name}-{suffix}");
    if profiles.iter().all(|profile| profile.id != candidate) {
      return candidate;
    }
  }
  unreachable!()
}

#[cfg(test)]
mod tests {
  use std::fs;

  use tempfile::tempdir;

  use super::*;

  fn profile(root: &Path, directory: &str, name: Option<&str>, network: bool) {
    let path = root.join(directory);
    let db = if network { path.join("Network").join("Cookies") } else { path.join("Cookies") };
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    fs::write(db, []).unwrap();
    if let Some(name) = name {
      fs::write(path.join("Preferences"), format!(r#"{{"profile":{{"name":"{name}"}}}}"#)).unwrap();
    }
  }

  fn firefox_profile(root: &Path, directory: &str) {
    let path = root.join("Profiles").join(directory);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("cookies.sqlite"), []).unwrap();
  }

  #[test]
  fn discovers_standard_profiles_with_stable_ids_and_names() {
    let dir = tempdir().unwrap();
    profile(dir.path(), "Profile 1", Some("Work"), true);
    profile(dir.path(), "Default", Some("Personal"), false);
    fs::create_dir(dir.path().join("System Profile")).unwrap();

    let mut found = profiles_in_root(Browser::Chrome, dir.path());
    found.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id(), "Default");
    assert_eq!(found[0].name(), "Personal");
    assert_eq!(found[1].id(), "Profile 1");
    assert_eq!(found[1].name(), "Work");
    assert!(found[0].is_default());
    assert!(!found[1].is_default());
    assert!(found[1].cookie_db().ends_with("Network/Cookies"));
  }

  #[test]
  fn marks_last_used_profile_as_default() {
    let dir = tempdir().unwrap();
    profile(dir.path(), "Default", Some("Personal"), false);
    profile(dir.path(), "Profile 1", Some("Work"), false);
    fs::write(dir.path().join("Local State"), r#"{"profile":{"last_used":"Profile 1"}}"#).unwrap();

    let found = profiles_in_root(Browser::Chrome, dir.path());
    assert!(found.iter().find(|profile| profile.id() == "Profile 1").unwrap().is_default());
    assert!(!found.iter().find(|profile| profile.id() == "Default").unwrap().is_default());
  }

  #[test]
  fn discovers_profile_kept_in_user_data_root() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cookies"), []).unwrap();
    fs::write(dir.path().join("Preferences"), r#"{"profile":{"name":"Personal"}}"#).unwrap();

    let found = profiles_in_root(Browser::Opera, dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id(), "Default");
    assert_eq!(found[0].name(), "Personal");
    assert!(found[0].is_default());
    assert_eq!(found[0].path(), dir.path());
    assert_eq!(found[0].cookie_db(), dir.path().join("Cookies"));

    profile(dir.path(), "Default", Some("Work"), false);
    let found = profiles_in_root(Browser::Opera, dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id(), "Default");
    assert_eq!(found[0].name(), "Work");
    assert_eq!(found[0].path(), dir.path().join("Default"));
    assert_eq!(found[0].cookie_db(), dir.path().join("Default").join("Cookies"));
  }

  #[test]
  fn root_network_cookies_are_not_a_separate_profile() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("Network").join("Cookies");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    fs::write(&db, []).unwrap();

    let found = profiles_in_root(Browser::Opera, dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id(), "Default");
    assert!(found[0].is_default());
    assert_eq!(found[0].path(), dir.path());
    assert_eq!(found[0].cookie_db(), db);
  }

  #[test]
  fn browser_names_round_trip() {
    for browser in Browser::all() {
      assert_eq!(browser.to_string().parse::<Browser>().unwrap(), browser);
    }
    assert_eq!("Helium".parse::<Browser>().unwrap(), Browser::Helium);
  }

  #[test]
  fn explicit_path_accepts_profile_directory_and_database() {
    let dir = tempdir().unwrap();
    profile(dir.path(), "custom", None, true);
    let path = dir.path().join("custom");
    let db = path.join("Network").join("Cookies");

    let from_dir = Profile::from_path(Browser::Chromium, &path).unwrap();
    let from_db = Profile::from_path(Browser::Chromium, &db).unwrap();
    assert_eq!(from_dir.cookie_db(), db);
    assert_eq!(from_db.cookie_db(), db);
    assert_eq!(from_db.path(), path);
  }

  #[test]
  fn discovers_firefox_profiles_and_install_default() {
    let dir = tempdir().unwrap();
    firefox_profile(dir.path(), "abc.default-release");
    fs::write(
      dir.path().join("profiles.ini"),
      "[Profile1]\nName=default\nIsRelative=1\nPath=Profiles/old.default\nDefault=1\n\n[Profile0]\nName=default-release\nIsRelative=1\nPath=Profiles/abc.default-release\n\n[InstallABC]\nDefault=Profiles/abc.default-release\nLocked=1\n",
    )
    .unwrap();

    let found = firefox_profiles_in_root(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].browser(), Browser::Firefox);
    assert_eq!(found[0].id(), "default-release");
    assert!(found[0].is_default());
    assert!(found[0].cookie_db().ends_with("Profiles/abc.default-release/cookies.sqlite"));
  }

  #[test]
  fn discovers_new_firefox_profile_groups_without_duplicating_legacy_profiles() {
    let dir = tempdir().unwrap();
    firefox_profile(dir.path(), "abc.default-release");
    firefox_profile(dir.path(), "def.Profile 1");
    fs::write(
      dir.path().join("profiles.ini"),
      "[Profile0]\nName=default-release\nIsRelative=1\nPath=Profiles/abc.default-release\n\n[InstallABC]\nDefault=Profiles/abc.default-release\n",
    )
    .unwrap();

    let groups = dir.path().join("Profile Groups");
    fs::create_dir(&groups).unwrap();
    let connection = rusqlite::Connection::open(groups.join("group.sqlite")).unwrap();
    connection
      .execute("CREATE TABLE Profiles (path TEXT NOT NULL, name TEXT NOT NULL)", [])
      .unwrap();
    connection
      .execute(
        "INSERT INTO Profiles (path, name) VALUES (?1, ?2), (?3, ?4)",
        ["Profiles/abc.default-release", "Original profile", "Profiles/def.Profile 1", "WORK"],
      )
      .unwrap();
    drop(connection);

    let mut found = firefox_profiles_in_root(dir.path());
    found.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id(), "Original profile");
    assert!(found[0].is_default());
    assert_eq!(found[1].id(), "WORK");
    assert!(!found[1].is_default());
  }

  #[test]
  fn explicit_firefox_path_accepts_profile_directory_and_database() {
    let dir = tempdir().unwrap();
    firefox_profile(dir.path(), "abc.default-release");
    let path = dir.path().join("Profiles/abc.default-release");
    let db = path.join("cookies.sqlite");

    let from_dir = Profile::from_path(Browser::Firefox, &path).unwrap();
    let from_db = Profile::from_path(Browser::Firefox, &db).unwrap();
    assert_eq!(from_dir.cookie_db(), db);
    assert_eq!(from_db.cookie_db(), db);
    assert_eq!(from_db.path(), path);
  }

  #[test]
  fn discovers_default_and_named_safari_profiles() {
    let dir = tempdir().unwrap();
    let container = dir.path().join("container");
    let default_db = container.join("Cookies").join("Cookies.binarycookies");
    fs::create_dir_all(default_db.parent().unwrap()).unwrap();
    fs::write(&default_db, []).unwrap();

    let uuid = "49B7B395-EC54-4474-BC94-7654492BB176";
    let named_db = container
      .join("WebKit")
      .join("WebsiteDataStore")
      .join(uuid.to_ascii_lowercase())
      .join("Cookies")
      .join("Cookies.binarycookies");
    fs::create_dir_all(named_db.parent().unwrap()).unwrap();
    fs::write(&named_db, []).unwrap();

    let tabs = container.join("Safari").join("SafariTabs.db");
    fs::create_dir_all(tabs.parent().unwrap()).unwrap();
    let connection = rusqlite::Connection::open(tabs).unwrap();
    connection
      .execute("CREATE TABLE bookmarks (external_uuid TEXT, title TEXT, subtype INTEGER)", [])
      .unwrap();
    connection
      .execute(
        "INSERT INTO bookmarks (external_uuid, title, subtype) VALUES (?1, ?2, 2)",
        [uuid, "WORK"],
      )
      .unwrap();
    drop(connection);

    let found = safari_profiles_in_root(&container, &dir.path().join("legacy.binarycookies"));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id(), "default");
    assert!(found[0].is_default());
    assert_eq!(found[0].cookie_db(), default_db);
    assert_eq!(found[1].id(), "WORK");
    assert!(!found[1].is_default());
    assert_eq!(found[1].cookie_db(), named_db);
  }

  #[test]
  fn explicit_safari_path_accepts_profile_directory_and_cookie_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("profile");
    let db = path.join("Cookies").join("Cookies.binarycookies");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    fs::write(&db, []).unwrap();

    let from_dir = Profile::from_path(Browser::Safari, &path).unwrap();
    let from_db = Profile::from_path(Browser::Safari, &db).unwrap();
    assert_eq!(from_dir.cookie_db(), db);
    assert_eq!(from_db.cookie_db(), db);
    assert_eq!(from_db.path(), path);
  }

  #[test]
  fn selection_is_case_insensitive_accepts_partials_and_rejects_ambiguity() {
    let dir = tempdir().unwrap();
    profile(dir.path(), "Default", Some("Same"), false);
    profile(dir.path(), "Profile 1", Some("Same"), false);
    profile(dir.path(), "Profile 2", Some("Personal"), false);
    let found = profiles_in_root(Browser::Chrome, dir.path());

    assert_eq!(select_discovered(&found, "profile 1").unwrap().unwrap().id(), "Profile 1");
    assert_eq!(select_discovered(&found, "file 1").unwrap().unwrap().id(), "Profile 1");
    assert_eq!(select_discovered(&found, "PERSONAL").unwrap().unwrap().id(), "Profile 2");
    assert_eq!(select_discovered(&found, "sona").unwrap().unwrap().id(), "Profile 2");
    let error = select_discovered(&found, "sam").unwrap_err();
    let error = error.to_string();
    assert!(error.contains("ambiguous"));
    assert!(error.contains("chrome / Default"));
    assert!(error.contains("use a profile ID or path"));
  }

  #[test]
  fn global_selection_rejects_duplicate_ids_across_browsers() {
    let dir = tempdir().unwrap();
    profile(dir.path(), "Default", Some("Personal"), false);
    let mut found = profiles_in_root(Browser::Chrome, dir.path());
    found.extend(profiles_in_root(Browser::Brave, dir.path()));

    let error = select_discovered(&found, "Default").unwrap_err();
    let error = error.to_string();
    assert!(error.contains("chrome / Default"));
    assert!(error.contains("brave / Default"));
    assert!(error.contains("pass --browser"));
  }
}
