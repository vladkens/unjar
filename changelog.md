## v0.2.0 – 2026-11-02

### Breaking Changes

- Replaced `Browser::ALL` with `Browser::all()`, an iterator over the supported browsers, so new browsers no longer break the API
- `Error::Database` now holds unjar's own `DatabaseError` instead of a rusqlite error, so rusqlite upgrades no longer break unjar's API
- `unjar::Error` no longer converts from rusqlite errors; the message is unchanged and `source()` returns the original rusqlite error

### Features

- Added Vivaldi, Opera, Arc and Helium, with their profiles found for you; Arc is macOS-only (#4)
- Added `HELIUM_CONFIG_HOME` support for finding Helium profiles on Linux (#4)

### Maintenance

- Updated dependencies, including rusqlite 0.41

**Full Changelog**: https://github.com/vladkens/unjar/compare/v0.1.1...v0.2.0

---

## v0.1.1 – 2026-10-21

### Maintenance

- Updated dependencies, including comfy-table 8 and dirs 7

**Full Changelog**: https://github.com/vladkens/unjar/compare/v0.1.0...v0.1.1

---

## v0.1.0 – 2026-08-02

### Features

- Added a CLI and Rust library for reading cookies from local browser profiles.
- Added profile discovery for Chrome, Chromium, Edge, Brave, Firefox, and Safari, including Firefox profile groups and Safari named profiles.
- Added profile selection by browser, profile ID, display name, or path, with case-insensitive partial matching.
- Added domain filtering and exports in Netscape, JSON, and `Cookie` header formats.

**Full Changelog**: https://github.com/vladkens/unjar/commits/v0.1.0
