use std::path::Path;

use crate::Result;
use crate::browser::Browser;
use crate::cookie::Cookie;

/// Read cookies from a chromium-family `Cookies` database (Chrome, Edge, Brave, ...).
///
/// Values are encrypted; the key is derived per platform and per browser (see [`decrypt`]).
pub(crate) fn read(path: &Path, browser: Browser) -> Result<Vec<Cookie>> {
  let db = crate::sqlite::open(path)?;
  let conn = &db.conn;

  let key = decrypt::key(browser)?;

  let mut stmt = conn.prepare(
    "SELECT host_key, name, value, encrypted_value, path, expires_utc, is_secure, is_httponly \
     FROM cookies",
  )?;

  let rows = stmt.query_map([], |row| {
    Ok((
      row.get::<_, String>(0)?,  // host_key
      row.get::<_, String>(1)?,  // name
      row.get::<_, String>(2)?,  // value (plaintext, usually empty)
      row.get::<_, Vec<u8>>(3)?, // encrypted_value
      row.get::<_, String>(4)?,  // path
      row.get::<_, i64>(5)?,     // expires_utc
      row.get::<_, i64>(6)? != 0,
      row.get::<_, i64>(7)? != 0,
    ))
  })?;

  let mut out = Vec::new();
  for row in rows {
    let (domain, name, plain, enc, path, expires_utc, secure, http_only) = row?;
    let value = if !plain.is_empty() { plain } else { decrypt::value(&key, &domain, &enc)? };
    out.push(Cookie {
      domain,
      name,
      value,
      path,
      expires: chrome_epoch(expires_utc),
      secure,
      http_only,
    });
  }

  Ok(out)
}

/// Convert a chromium timestamp (microseconds since 1601-01-01) to unix seconds.
fn chrome_epoch(us: i64) -> i64 {
  if us == 0 {
    return 0;
  }
  (us - 11_644_473_600_000_000) / 1_000_000
}

mod decrypt {
  use crate::Result;
  use crate::browser::Browser;

  /// macOS Keychain entry (account, service) that holds a browser's Safe Storage key.
  #[cfg(target_os = "macos")]
  fn keychain_entry(browser: Browser) -> Result<(&'static str, &'static str)> {
    Ok(match browser {
      Browser::Chrome => ("Chrome", "Chrome Safe Storage"),
      Browser::Chromium => ("Chromium", "Chromium Safe Storage"),
      Browser::Edge => ("Microsoft Edge", "Microsoft Edge Safe Storage"),
      Browser::Brave => ("Brave", "Brave Safe Storage"),
      Browser::Vivaldi => ("Vivaldi", "Vivaldi Safe Storage"),
      Browser::Opera => ("Opera", "Opera Safe Storage"),
      Browser::Arc => ("Arc", "Arc Safe Storage"),
      // Helium renames the Keychain item: https://github.com/imputnet/helium-macos/blob/main/patches/helium/macos/change-keychain-name.patch
      Browser::Helium => ("Helium", "Helium Storage Key"),
      other => return Err(format!("{other}: no chromium Keychain entry").into()),
    })
  }

  /// Read the browser's AES key from the macOS Keychain and stretch it via PBKDF2.
  #[cfg(target_os = "macos")]
  pub(super) fn key(browser: Browser) -> Result<Vec<u8>> {
    use std::process::Command;

    let (account, service) = keychain_entry(browser)?;
    let out = Command::new("security")
      .args(["find-generic-password", "-w", "-a", account, "-s", service])
      .output()?;
    if !out.status.success() {
      return Err(format!("could not read '{service}' key from Keychain").into());
    }

    let password = String::from_utf8(out.stdout)?.trim().to_string();
    Ok(derive(password.as_bytes(), 1003))
  }

  /// Linux v10 uses the well-known fallback password `peanuts` for all chromium browsers.
  ///
  /// TODO: read the libsecret-derived key (v11) via the Secret Service API.
  #[cfg(target_os = "linux")]
  pub(super) fn key(_browser: Browser) -> Result<Vec<u8>> {
    Ok(derive(b"peanuts", 1))
  }

  #[cfg(target_os = "windows")]
  pub(super) fn key(_browser: Browser) -> Result<Vec<u8>> {
    Err("chromium cookie decryption on Windows is not supported because current browsers use App-Bound Encryption".into())
  }

  #[cfg(any(target_os = "macos", target_os = "linux"))]
  fn derive(password: &[u8], rounds: u32) -> Vec<u8> {
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, b"saltysalt", rounds, &mut key);
    key.to_vec()
  }

  /// Decrypt an `encrypted_value` blob using AES-128-CBC (v10 / v11).
  #[cfg(any(target_os = "macos", target_os = "linux"))]
  pub(super) fn value(key: &[u8], domain: &str, enc: &[u8]) -> Result<String> {
    use cbc::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

    if enc.len() < 3 {
      return Ok(String::new());
    }

    let ct = &enc[3..]; // strip the "v10" / "v11" version prefix
    let iv = [0x20u8; 16];
    let pt = Aes128CbcDec::new_from_slices(key, &iv)
      .map_err(|e| format!("cipher init failed: {e}"))?
      .decrypt_padded_vec::<Pkcs7>(ct)
      .map_err(|e| format!("decrypt failed: {e}"))?;

    Ok(String::from_utf8_lossy(strip_domain_hash(domain, &pt)).into_owned())
  }

  /// Newer Chromium prepends `SHA256(host_key)` (32 bytes) to the plaintext to
  /// bind a cookie to its domain. Strip it when present, leave older values as-is.
  #[cfg(any(target_os = "macos", target_os = "linux"))]
  fn strip_domain_hash<'a>(domain: &str, pt: &'a [u8]) -> &'a [u8] {
    use sha2::{Digest, Sha256};

    if pt.len() >= 32 && pt[..32] == Sha256::digest(domain.as_bytes())[..] { &pt[32..] } else { pt }
  }

  #[cfg(target_os = "windows")]
  pub(super) fn value(_key: &[u8], _domain: &str, _enc: &[u8]) -> Result<String> {
    Err("chromium cookie decryption on Windows is not supported because current browsers use App-Bound Encryption".into())
  }
}
