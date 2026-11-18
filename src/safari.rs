use std::path::Path;

use crate::cookie::Cookie;
use crate::{Error, Result};

const FILE_MAGIC: &[u8; 4] = b"cook";
const PAGE_MAGIC: &[u8; 4] = b"\x00\x00\x01\x00";
const COOKIE_HEADER_LEN: usize = 56;
const MAX_COOKIE_DATA_LEN: usize = 4096;
const MAC_EPOCH_OFFSET: f64 = 978_307_200.0;

/// Read cookies from Safari's `Cookies.binarycookies` store.
pub(crate) fn read(path: &Path) -> Result<Vec<Cookie>> {
  let data = std::fs::read(path)?;
  parse(&data).map_err(|message| {
    Error::Message(format!("could not parse Safari cookies at {}: {message}", path.display()))
  })
}

fn parse(data: &[u8]) -> std::result::Result<Vec<Cookie>, &'static str> {
  let mut file = Cursor::new(data);
  if file.take(4)? != FILE_MAGIC {
    return Err("invalid file signature");
  }

  let page_count = file.u32_be()? as usize;
  let table_len = page_count.checked_mul(4).ok_or("page table is too large")?;
  let page_sizes = file.take(table_len)?;

  let mut cookies = Vec::new();
  for index in 0..page_count {
    let start = index * 4;
    let size = read_u32_be(page_sizes, start)? as usize;
    parse_page(file.take(size)?, &mut cookies)?;
  }

  Ok(cookies)
}

fn parse_page(page: &[u8], cookies: &mut Vec<Cookie>) -> std::result::Result<(), &'static str> {
  let mut header = Cursor::new(page);
  if header.take(4)? != PAGE_MAGIC {
    return Err("invalid page signature");
  }

  let cookie_count = header.u32_le()? as usize;
  let offsets_len = cookie_count.checked_mul(4).ok_or("cookie offset table is too large")?;
  let offsets = header.take(offsets_len)?;
  if header.take(4)? != [0; 4] {
    return Err("invalid page header");
  }

  let header_len = header.position();

  for index in 0..cookie_count {
    let start = index * 4;
    let offset = read_u32_le(offsets, start)? as usize;
    if offset < header_len {
      return Err("cookie offset points into the page header");
    }

    let record = page.get(offset..).ok_or("cookie offset is outside its page")?;
    cookies.push(parse_cookie(record)?);
  }

  Ok(())
}

fn parse_cookie(data: &[u8]) -> std::result::Result<Cookie, &'static str> {
  let size = read_u32_le(data, 0)? as usize;
  if !(COOKIE_HEADER_LEN..=COOKIE_HEADER_LEN + MAX_COOKIE_DATA_LEN).contains(&size) {
    return Err("invalid cookie size");
  }

  let record = data.get(..size).ok_or("cookie extends past its page")?;
  if record.get(36..40) != Some(&[0; 4]) {
    return Err("invalid cookie header");
  }

  let flags = read_u32_le(record, 8)?;
  let domain = read_string(record, read_u32_le(record, 16)?)?;
  let name = read_string(record, read_u32_le(record, 20)?)?;
  let path = read_string(record, read_u32_le(record, 24)?)?;
  let value = read_string(record, read_u32_le(record, 28)?)?;
  let expires = read_f64_le(record, 40)? + MAC_EPOCH_OFFSET;
  if !expires.is_finite() || expires < i64::MIN as f64 || expires > i64::MAX as f64 {
    return Err("invalid cookie expiration time");
  }

  Ok(Cookie {
    domain,
    name,
    value,
    path,
    expires: expires as i64,
    secure: flags & 0x1 != 0,
    http_only: flags & 0x4 != 0,
  })
}

fn read_string(data: &[u8], offset: u32) -> std::result::Result<String, &'static str> {
  let offset = offset as usize;
  if offset < COOKIE_HEADER_LEN {
    return Err("cookie string offset points into its header");
  }

  let tail = data.get(offset..).ok_or("cookie string offset is outside its record")?;
  let end = tail.iter().position(|byte| *byte == 0).ok_or("unterminated cookie string")?;
  Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
}

fn read_u32_le(data: &[u8], offset: usize) -> std::result::Result<u32, &'static str> {
  data
    .get(offset..offset.checked_add(4).ok_or("invalid integer offset")?)
    .ok_or("unexpected end of input")?
    .try_into()
    .map(u32::from_le_bytes)
    .map_err(|_| "unexpected end of input")
}

fn read_u32_be(data: &[u8], offset: usize) -> std::result::Result<u32, &'static str> {
  data
    .get(offset..offset.checked_add(4).ok_or("invalid integer offset")?)
    .ok_or("unexpected end of input")?
    .try_into()
    .map(u32::from_be_bytes)
    .map_err(|_| "unexpected end of input")
}

fn read_f64_le(data: &[u8], offset: usize) -> std::result::Result<f64, &'static str> {
  data
    .get(offset..offset.checked_add(8).ok_or("invalid float offset")?)
    .ok_or("unexpected end of input")?
    .try_into()
    .map(f64::from_le_bytes)
    .map_err(|_| "unexpected end of input")
}

struct Cursor<'a> {
  data: &'a [u8],
  position: usize,
}

impl<'a> Cursor<'a> {
  const fn new(data: &'a [u8]) -> Self {
    Self { data, position: 0 }
  }

  const fn position(&self) -> usize {
    self.position
  }

  fn take(&mut self, len: usize) -> std::result::Result<&'a [u8], &'static str> {
    let end = self.position.checked_add(len).ok_or("input offset overflow")?;
    let value = self.data.get(self.position..end).ok_or("unexpected end of input")?;
    self.position = end;
    Ok(value)
  }

  fn u32_be(&mut self) -> std::result::Result<u32, &'static str> {
    self.take(4)?.try_into().map(u32::from_be_bytes).map_err(|_| "unexpected end of input")
  }

  fn u32_le(&mut self) -> std::result::Result<u32, &'static str> {
    self.take(4)?.try_into().map(u32::from_le_bytes).map_err(|_| "unexpected end of input")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
  }

  fn fixture() -> Vec<u8> {
    let mut record = vec![0; COOKIE_HEADER_LEN];

    // Deliberately store fields in a different order than their header entries.
    let value_offset = record.len() as u32;
    record.extend_from_slice(b"secret\0");
    let path_offset = record.len() as u32;
    record.extend_from_slice(b"/account\0");
    let domain_offset = record.len() as u32;
    record.extend_from_slice(b".example.com\0");
    let name_offset = record.len() as u32;
    record.extend_from_slice(b"session\0");

    let record_len = record.len() as u32;
    set_u32(&mut record, 0, record_len);
    set_u32(&mut record, 8, 0x4205);
    set_u32(&mut record, 16, domain_offset);
    set_u32(&mut record, 20, name_offset);
    set_u32(&mut record, 24, path_offset);
    set_u32(&mut record, 28, value_offset);
    record[40..48].copy_from_slice(&100.75f64.to_le_bytes());

    let mut page = Vec::new();
    page.extend_from_slice(PAGE_MAGIC);
    page.extend_from_slice(&1u32.to_le_bytes());
    page.extend_from_slice(&16u32.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    page.extend_from_slice(&record);

    let mut file = Vec::new();
    file.extend_from_slice(FILE_MAGIC);
    file.extend_from_slice(&1u32.to_be_bytes());
    file.extend_from_slice(&(page.len() as u32).to_be_bytes());
    file.extend_from_slice(&page);
    file.extend_from_slice(&[0; 4]); // checksum and optional footer are irrelevant
    file
  }

  #[test]
  fn parses_cookie_fields_by_offset() {
    let cookies = parse(&fixture()).unwrap();
    assert_eq!(cookies.len(), 1);

    let cookie = &cookies[0];
    assert_eq!(cookie.domain, ".example.com");
    assert_eq!(cookie.name, "session");
    assert_eq!(cookie.value, "secret");
    assert_eq!(cookie.path, "/account");
    assert_eq!(cookie.expires, 978_307_300);
    assert!(cookie.secure);
    assert!(cookie.http_only);
  }

  #[test]
  fn rejects_truncated_files() {
    let mut file = fixture();
    file.truncate(file.len() - 10);
    assert_eq!(parse(&file).unwrap_err(), "unexpected end of input");
  }

  #[test]
  fn rejects_cookie_offsets_outside_the_page() {
    let mut file = fixture();
    set_u32(&mut file, 20, u32::MAX);
    assert_eq!(parse(&file).unwrap_err(), "cookie offset is outside its page");
  }

  #[test]
  fn rejects_unterminated_strings() {
    let mut file = fixture();
    let page_offset = 12;
    let record_offset = page_offset + 16;
    let record_len = read_u32_le(&file[record_offset..], 0).unwrap() as usize;
    set_u32(&mut file, record_offset + 16, (record_len - 1) as u32);
    file[record_offset + record_len - 1] = b'x';
    assert_eq!(parse(&file).unwrap_err(), "unterminated cookie string");
  }
}
