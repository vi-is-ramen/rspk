//! Progress pattern parser.
//!
//! Recognizes known progress patterns in command stdout and emits
//! structured [`ProgressEvent`]s. Used by
//! [`CommandBuilder::run_streaming`](crate::CommandBuilder::run_streaming)
//! to drive progress bars without each manager needing its own
//! parser.

use regex::Regex;

/// A progress event extracted from a single line of output.
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent
{
    /// A known percentage (0..=100) with a human-readable message.
    ///
    /// Examples:
    /// - apt: `Progress: [ 42%]`
    /// - apt-get: `55% [Working]`
    Percent(u8, String),

    /// A named stage of a multi-stage operation.
    ///
    /// Examples:
    /// - cargo: `Downloading foo v1.2.3`
    /// - cargo: `Compiling foo v1.2.3`
    /// - brew: `==> Pouring foo--1.2.3.el_capitan.bottle.tar.gz`
    Stage(String),

    /// A byte-level progress indicator (download).
    ///
    /// `total` is `None` when the size is unknown.
    BytesDownloaded(u64, Option<u64>, String),

    /// Line does not match any known progress pattern.
    None,
}

// ───────────────────────────────────────────────────────────────
//  Compiled regexes (lazy, thread-safe).
// ───────────────────────────────────────────────────────────────

static APT_PERCENT: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        Regex::new(r"Progress:\s*\[\s*(\d{1,3})%\s*\]").unwrap()
    });

static APT_ALT_PERCENT: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^(\d{1,3})%\s+\[").unwrap());

static CARGO_DOWNLOAD: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        Regex::new(r"^\s*Downloading\s+(\S+)\s+v?(\S+)").unwrap()
    });

static CARGO_COMPILE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        Regex::new(r"^\s*Compiling\s+(\S+)\s+v?(\S+)").unwrap()
    });

static CARGO_INSTALLING: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\s*Installing\s+(.+)").unwrap());

static BREW_POURING: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^==>\s*Pouring\s+(.+)").unwrap());

static BREW_DOWNLOADING: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        Regex::new(r"^==>\s*Downloading\s+(.+)").unwrap()
    });

static BREW_INSTALLING: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        Regex::new(r"^==>\s*(?:Installing|Caveats|Summary)").unwrap()
    });

static PACMAN_PROGRESS: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        // pacman prints bars like: ` core        1.2 MiB  10.2M/s 00:00
        // [######################] 100%`
        Regex::new(r"(\d{1,3})%$").unwrap()
    });

static CURL_WGET_BYTES: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| {
        // curl-style: ` 42 1234k   42  520k    0:00:02 ...`
        Regex::new(r"^\s*(\d{1,3})\s+(\d+)([kKmMgG])").unwrap()
    });

/// Parses a single line and returns the recognized progress event.
pub fn parse_line(line: &str) -> ProgressEvent
{
    let trimmed = line.trim();
    if trimmed.is_empty()
    {
        return ProgressEvent::None;
    }

    // apt / aptitude: `Progress: [ 42%]`
    if let Some(caps) = APT_PERCENT.captures(trimmed)
    {
        let pct: u8 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
        return ProgressEvent::Percent(pct, "installing".to_string());
    }
    if let Some(caps) = APT_ALT_PERCENT.captures(trimmed)
    {
        let pct: u8 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
        return ProgressEvent::Percent(pct, "working".to_string());
    }

    // pacman: trailing `NN%`
    if let Some(caps) = PACMAN_PROGRESS.captures(trimmed)
    {
        let pct: u8 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
        return ProgressEvent::Percent(pct, "syncing".to_string());
    }

    // cargo stages
    if let Some(caps) = CARGO_DOWNLOAD.captures(trimmed)
    {
        return ProgressEvent::Stage(format!(
            "downloading {} v{}",
            caps.get(1).unwrap().as_str(),
            caps.get(2).unwrap().as_str()
        ));
    }
    if let Some(caps) = CARGO_COMPILE.captures(trimmed)
    {
        return ProgressEvent::Stage(format!(
            "compiling {} v{}",
            caps.get(1).unwrap().as_str(),
            caps.get(2).unwrap().as_str()
        ));
    }
    if let Some(caps) = CARGO_INSTALLING.captures(trimmed)
    {
        return ProgressEvent::Stage(format!(
            "installing {}",
            caps.get(1).unwrap().as_str()
        ));
    }

    // brew stages
    if let Some(caps) = BREW_DOWNLOADING.captures(trimmed)
    {
        return ProgressEvent::Stage(format!(
            "downloading {}",
            caps.get(1).unwrap().as_str()
        ));
    }
    if let Some(caps) = BREW_POURING.captures(trimmed)
    {
        return ProgressEvent::Stage(format!(
            "pouring {}",
            caps.get(1).unwrap().as_str()
        ));
    }
    if BREW_INSTALLING.is_match(trimmed)
    {
        return ProgressEvent::Stage("installing".to_string());
    }

    // curl/wget byte-level progress
    if let Some(caps) = CURL_WGET_BYTES.captures(trimmed)
    {
        let pct: u8 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
        let num: u64 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
        let unit = caps.get(3).unwrap().as_str();
        let bytes = match unit
        {
            "k" | "K" => num * 1024,
            "m" | "M" => num * 1024 * 1024,
            "g" | "G" => num * 1024 * 1024 * 1024,
            _ => num,
        };
        return ProgressEvent::BytesDownloaded(
            bytes,
            None,
            format!("{pct}% downloaded"),
        );
    }

    ProgressEvent::None
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_apt_progress()
    {
        match parse_line("Progress: [ 42%]")
        {
            ProgressEvent::Percent(42, _) =>
            {},
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_apt_alt_progress()
    {
        match parse_line("55% [Working]")
        {
            ProgressEvent::Percent(55, _) =>
            {},
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_cargo_download()
    {
        match parse_line("  Downloading ripgrep v14.1.0")
        {
            ProgressEvent::Stage(msg) => assert!(msg.contains("ripgrep")),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_cargo_compile()
    {
        match parse_line("   Compiling regex v1.10.2")
        {
            ProgressEvent::Stage(msg) => assert!(msg.contains("regex")),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_brew_pouring()
    {
        match parse_line(
            "==> Pouring ripgrep--14.1.0.arm64_sonoma.bottle.tar.gz",
        )
        {
            ProgressEvent::Stage(msg) => assert!(msg.contains("ripgrep")),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_pacman_trailing_percent()
    {
        match parse_line(
            " core        1.2 MiB  10.2M/s 00:00 [##############] 100%",
        )
        {
            ProgressEvent::Percent(100, _) =>
            {},
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_unknown_line()
    {
        assert_eq!(
            parse_line("Setting up foo (1.2.3) ..."),
            ProgressEvent::None
        );
    }
}
