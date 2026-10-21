<div align="center">

# unjar 🍪

**Your browser's cookies, ready for curl, yt-dlp and scripts**

[<img src="https://badges.ws/crates/v/unjar" alt="version" />](https://crates.io/crates/unjar)
[<img src="https://badges.ws/crates/docs/unjar" alt="docs" />](https://docs.rs/unjar)
[<img src="https://badges.ws/github/license/vladkens/unjar" alt="license" />](https://github.com/vladkens/unjar/blob/main/LICENSE)
[<img src="https://badges.ws/badge/follow-@uid127-black" alt="follow @uid127" />](https://x.com/uid127)

</div>

`unjar` reads the cookies of a browser profile on your machine and exports them as a `cookies.txt` file, JSON or a `Cookie:` header. Log in once in your browser, then give that session to curl, yt-dlp or your own script, without copying cookies by hand or a browser extension. Use it as a command-line tool or as a Rust library.

## 🌟 Features

- 🌐 Chrome, Chromium, Edge, Brave, Firefox and Safari, with their profiles found for you
- 🔓 Decrypts Chromium cookies with the key from the macOS Keychain
- 📄 Netscape `cookies.txt` (curl, wget, yt-dlp), JSON, or a `Cookie:` header
- 🎯 Pick a profile by ID, name, a unique part of it, or a path
- 🧩 One domain, several, or every cookie of the profile
- 🦀 A Rust library without the CLI's dependencies

## 📥 Installation

Download the binary for macOS (Apple Silicon) from the [latest release](https://github.com/vladkens/unjar/releases/latest) and put `unjar` into a folder on your `PATH`, such as `~/.local/bin`. Or build it with Rust:

```sh
cargo install unjar
```

### Newest version

This readme follows main, which may be ahead of the latest release. The binary of main is in the [dev release](https://github.com/vladkens/unjar/releases/tag/dev), or build it with Rust:

```sh
cargo install --git https://github.com/vladkens/unjar
```

## 🚀 Usage

```sh
unjar list                          # the browser profiles found on this machine
unjar x.com                         # cookies for x.com from Chrome's default profile, as cookies.txt
unjar x.com t.co                    # or for several domains
unjar all                           # every cookie of the profile
unjar -p Work x.com                 # from another profile, by ID or name
unjar -b firefox x.com              # from another browser
unjar -b chromium -p /path/to/profile x.com   # from a profile directory or cookie database
unjar -f json x.com                 # as JSON
unjar -f header x.com               # as a Cookie: header value
unjar -o cookies.txt x.com          # into a file
```

Without `--browser` a profile is looked up in every browser: a unique ID or name is enough, and `--browser` tells duplicates apart. A path may point to a profile directory or straight to its cookie database; for a copied or unknown path, add `--browser` so `unjar` picks the right decryption. The `header` format takes one domain and includes every cookie stored for it, without checking path, scheme or expiry; use cookies.txt or JSON for several domains or `all`.

### Recipes

Send your browser session to an API with curl, here the GitLab user you are logged in as (`-b -` reads cookies.txt from stdin):

```sh
unjar gitlab.com | curl -b - https://gitlab.com/api/v4/user
```

Add an X account to [twscrape](https://github.com/vladkens/twscrape) from your browser's login; `my_account` is twscrape's local name for it, and the cookies include the `auth_token` and `ct0` it needs:

```sh
unjar -f header x.com | twscrape add_cookie my_account
```

## 🌐 Supported browsers

✅ tested · 🟡 implemented, not yet tested · 🚧 not implemented

| Browser  | macOS | Linux | Windows |
| -------- | :---: | :---: | :-----: |
| Chrome   |  ✅   |  🟡   |   🚧    |
| Chromium |  🟡   |  🟡   |   🚧    |
| Edge     |  ✅   |  🟡   |   🚧    |
| Brave    |  🟡   |  🟡   |   🚧    |
| Firefox  |  ✅   |  🟡   |   🟡    |
| Safari   |  ✅   |   —   |    —    |

Chrome, Edge, Firefox and Safari, with Safari's named profiles, are verified end to end on macOS. On Linux, Chromium cookies are decrypted with the `peanuts` fallback key only, so profiles whose key is in the system keyring (v11) don't decrypt yet. Current Chromium-based browsers on Windows protect cookies with App-Bound Encryption that only the browser itself can decrypt; supporting them needs an explicit browser integration such as an extension, and `unjar` doesn't bypass it through elevation or process injection.

## 📚 Library usage

```sh
cargo add unjar --no-default-features
```

```rust
use unjar::Browser;

let profile = Browser::Chrome.find_profile("Default")?;
let jar = profile.cookies()?.domain("x.com");

for c in jar.iter() {
  println!("{}={}", c.name, c.value);
}

let header = jar.to_header(); // or hand them straight to an HTTP client
```

`--no-default-features` leaves out the command line and its dependencies. The [API docs](https://docs.rs/unjar) have the details.

## 🤝 Contributing

Pull requests are welcome. When reporting a bug, include the output of `unjar list` and your browser's version.

## 📝 License

Distributed under the [MIT License](LICENSE).
