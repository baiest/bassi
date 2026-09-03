//! Persists the overlay's chosen window size across restarts. Deliberately
//! tiny (one `size=<f32>` line, no serde) since it's the only setting the
//! overlay has right now — see `overlay.rs`'s scroll-to-resize handling.
//! Parsing/formatting is pure and tested; the filesystem I/O around it
//! (`load`/`save`) is not, same split as `voice_client.rs`'s socket wrapper.

/// Smallest window size the overlay allows, in egui points.
pub const MIN_SIZE: f32 = 64.0;

/// Largest window size the overlay allows, in egui points.
pub const MAX_SIZE: f32 = 320.0;

/// Window size used the first time the overlay ever runs (or if its config
/// is missing/unreadable) — big enough that the sphere's edge doesn't look
/// jagged, unlike the old fixed 80x80 window.
pub const DEFAULT_SIZE: f32 = 160.0;

const KEY: &str = "size";

/// Parses a `size=<f32>` line into the size it names, clamped to
/// `[MIN_SIZE, MAX_SIZE]`. Returns `None` for anything that isn't a valid,
/// finite number — garbage config never panics the overlay, it just falls
/// back to [`DEFAULT_SIZE`].
pub fn parse(text: &str) -> Option<f32> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix(KEY)?.strip_prefix('='))?
        .trim()
        .parse::<f32>()
        .ok()?;

    if !value.is_finite() {
        return None;
    }

    Some(value.clamp(MIN_SIZE, MAX_SIZE))
}

/// Formats `size` back into the one-line config format `parse` reads.
pub fn format(size: f32) -> String {
    format!("{KEY}={size}\n")
}

#[cfg(windows)]
mod persist {
    use super::{DEFAULT_SIZE, format, parse};
    use std::path::PathBuf;

    /// Where the overlay's config file lives — `%APPDATA%\nala-overlay\overlay.cfg`.
    /// `None` if `APPDATA` isn't set, which callers treat as "can't persist,
    /// not fatal."
    fn config_path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("nala-overlay")
                .join("overlay.cfg"),
        )
    }

    /// Loads the persisted window size, falling back to [`DEFAULT_SIZE`] if
    /// there's no config file, it can't be read, or it doesn't parse.
    pub fn load() -> f32 {
        config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| parse(&text))
            .unwrap_or(DEFAULT_SIZE)
    }

    /// Saves `size` to disk, creating the config directory if needed.
    /// Failures are non-fatal — the overlay just won't remember the size
    /// for next time — and are reported the same way every other
    /// non-fatal I/O failure in this crate is (see `overlay.rs`).
    pub fn save(size: f32) {
        let Some(path) = config_path() else {
            return;
        };
        if let Some(dir) = path.parent()
            && let Err(error) = std::fs::create_dir_all(dir)
        {
            eprintln!("Warning: could not create the overlay config directory: {error}");
            return;
        }
        if let Err(error) = std::fs::write(&path, format(size)) {
            eprintln!("Warning: could not save the overlay's window size: {error}");
        }
    }
}

#[cfg(windows)]
pub use persist::{load, save};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_then_parse_round_trips() {
        assert_eq!(parse(&format(180.0)), Some(180.0));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(parse("not a config file"), None);
    }

    #[test]
    fn parse_rejects_an_empty_string() {
        assert_eq!(parse(""), None);
    }

    #[test]
    fn parse_clamps_a_too_small_size() {
        assert_eq!(parse("size=1.0"), Some(MIN_SIZE));
    }

    #[test]
    fn parse_clamps_a_too_large_size() {
        assert_eq!(parse("size=99999.0"), Some(MAX_SIZE));
    }

    #[test]
    fn parse_rejects_non_finite_values() {
        assert_eq!(parse("size=NaN"), None);
        assert_eq!(parse("size=inf"), None);
    }

    #[test]
    fn parse_ignores_unrelated_lines() {
        assert_eq!(parse("# comment\nsize=200.0\nother=stuff"), Some(200.0));
    }
}
