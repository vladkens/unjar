use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table, presets::NOTHING};
use unjar::{Browser, CookieJar, Profile, Result, find_profile, profiles};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
  /// Netscape cookies.txt (curl, wget, yt-dlp)
  Netscape,
  /// JSON array
  Json,
  /// Cookie header value (k=v; k2=v2)
  Header,
}

/// Read and export cookies from local browser profiles.
#[derive(Debug, Parser)]
#[command(name = "unjar", version, about, args_conflicts_with_subcommands = true)]
struct Cli {
  #[command(subcommand)]
  command: Option<Command>,

  #[command(flatten)]
  export: Export,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// List discovered browser profiles
  List,
}

#[derive(Debug, Args)]
struct Export {
  /// Domains to export (e.g. x.com t.co), or `all` to dump every cookie
  #[arg(value_name = "DOMAIN")]
  domains: Vec<String>,

  /// Browser to use (defaults to chrome when --profile is omitted)
  #[arg(short, long)]
  browser: Option<Browser>,

  /// Profile ID, name, unique partial match, profile directory, or cookie database
  #[arg(short, long)]
  profile: Option<String>,

  /// Output format
  #[arg(short, long, value_enum, default_value = "netscape")]
  format: Format,

  /// Write to a file instead of stdout
  #[arg(short, long)]
  output: Option<PathBuf>,
}

fn main() -> Result<()> {
  if std::env::args_os().len() == 1 {
    Cli::command().print_help()?;
    println!();
    return Ok(());
  }

  let cli = Cli::parse();

  if matches!(cli.command, Some(Command::List)) {
    print_profiles();
    return Ok(());
  }

  export(cli.export)
}

fn print_profiles() {
  let mut table = Table::new();
  table.load_style(NOTHING);
  table.set_content_arrangement(ContentArrangement::Dynamic);
  table.set_header(
    ["Browser", "Profile", "Default", "Path"]
      .map(|heading| Cell::new(heading).add_attribute(Attribute::Bold)),
  );

  for profile in profiles() {
    table.add_row([
      Cell::new(profile.browser()).fg(Color::Cyan),
      Cell::new(profile.id()).fg(Color::Magenta),
      Cell::new(if profile.is_default() { "yes" } else { "" }).fg(Color::Green),
      Cell::new(profile.path().display()),
    ]);
  }

  println!("{table}");
}

fn export(args: Export) -> Result<()> {
  if args.domains.is_empty() {
    return Err("missing domain; pass one or more domains, or `all`".into());
  }
  if args.domains.iter().any(|domain| domain == "list") {
    return Err("`list` is a command and cannot be combined with export arguments".into());
  }

  let all = matches!(args.domains.as_slice(), [domain] if domain == "all");
  if !all && args.domains.iter().any(|domain| domain == "all") {
    return Err("`all` cannot be combined with domains".into());
  }
  if args.format == Format::Header && (all || args.domains.len() != 1) {
    return Err("header format requires exactly one domain".into());
  }

  let profile = match &args.profile {
    Some(selector) => Some(select_profile(args.browser, selector)?),
    None => None,
  };
  let browser = args.browser.unwrap_or(Browser::Chrome);

  let jar = match profile {
    Some(profile) => profile.cookies()?,
    None => browser.cookies()?,
  };
  let jar = if all {
    jar
  } else {
    let domains: Vec<_> = args.domains.iter().map(String::as_str).collect();
    jar.domains(&domains)
  };

  let out = format_jar(&jar, args.format);

  match args.output {
    Some(path) => {
      let mut options = OpenOptions::new();
      options.write(true).create(true).truncate(true);
      #[cfg(unix)]
      options.mode(0o600);

      let mut file = options.open(&path)?;
      #[cfg(unix)]
      file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
      file.write_all(out.as_bytes())?;
      eprintln!("wrote {} cookie(s) to {}", jar.len(), path.display());
    }
    None => println!("{out}"),
  }

  Ok(())
}

fn select_profile(browser: Option<Browser>, selector: &str) -> Result<Profile> {
  match browser {
    Some(browser) => browser.find_profile(selector),
    None => find_profile(selector),
  }
}

fn format_jar(jar: &CookieJar, format: Format) -> String {
  match format {
    Format::Netscape => jar.to_netscape(),
    Format::Json => jar.to_json(),
    Format::Header => jar.to_header(),
  }
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  #[test]
  fn list_is_a_subcommand() {
    let cli = Cli::try_parse_from(["unjar", "list"]).unwrap();
    assert!(matches!(cli.command, Some(Command::List)));
  }

  #[test]
  fn export_arguments_conflict_with_list() {
    for args in [
      vec!["unjar", "x.com", "list"],
      vec!["unjar", "-f", "json", "list"],
      vec!["unjar", "-b", "chrome", "list"],
    ] {
      let cli = Cli::try_parse_from(args).unwrap();
      assert!(export(cli.export).unwrap_err().to_string().contains("`list` is a command"));
    }
    assert!(Cli::try_parse_from(["unjar", "list", "x.com"]).is_err());
  }

  #[test]
  fn browser_and_profile_flags_are_independent() {
    let cli = Cli::try_parse_from([
      "unjar",
      "example.com",
      "--browser",
      "chrome",
      "--profile",
      "Profile 1",
    ])
    .unwrap();
    assert_eq!(cli.export.domains, ["example.com"]);
    assert_eq!(cli.export.browser, Some(Browser::Chrome));
    assert_eq!(cli.export.profile.as_deref(), Some("Profile 1"));
  }

  #[test]
  fn browser_is_optional() {
    let cli = Cli::try_parse_from(["unjar", "example.com", "-p", "Work"]).unwrap();
    assert_eq!(cli.export.browser, None);
    assert_eq!(cli.export.profile.as_deref(), Some("Work"));
  }

  #[test]
  fn multiple_domains_are_positional_arguments() {
    let cli = Cli::try_parse_from(["unjar", "x.com", "t.co", "-p", "Work"]).unwrap();
    assert_eq!(cli.export.domains, ["x.com", "t.co"]);
  }

  #[test]
  fn options_can_surround_domains() {
    for args in [
      ["unjar", "-f", "json", "x.com", "t.co"],
      ["unjar", "x.com", "-f", "json", "t.co"],
      ["unjar", "x.com", "t.co", "-f", "json"],
    ] {
      let cli = Cli::try_parse_from(args).unwrap();
      assert_eq!(cli.export.domains, ["x.com", "t.co"]);
      assert_eq!(cli.export.format, Format::Json);
    }
  }

  #[test]
  fn all_is_an_explicit_exclusive_target() {
    let cli = Cli::try_parse_from(["unjar", "all", "-b", "firefox"]).unwrap();
    assert_eq!(cli.export.domains, ["all"]);

    let cli = Cli::try_parse_from(["unjar", "x.com", "all"]).unwrap();
    assert!(export(cli.export).unwrap_err().to_string().contains("cannot be combined"));
  }

  #[test]
  fn a_target_is_required_for_export() {
    let cli = Cli::try_parse_from(["unjar", "-b", "chrome"]).unwrap();
    assert!(export(cli.export).unwrap_err().to_string().contains("missing domain"));
  }

  #[test]
  fn header_format_requires_one_domain() {
    for args in
      [vec!["unjar", "-f", "header", "all"], vec!["unjar", "-f", "header", "x.com", "t.co"]]
    {
      let cli = Cli::try_parse_from(args).unwrap();
      assert!(export(cli.export).unwrap_err().to_string().contains("exactly one domain"));
    }
  }
}
