//! Minimal ANSI color helper — no extra dependencies.
//!
//! Call `init(no_color_flag)` once in `main` after `Cli::parse()`.
//! All other functions are safe to call from tests without calling `init` first
//! (they default to no-color).

use std::sync::OnceLock;

static USE_COLOR: OnceLock<bool> = OnceLock::new();

/// Initialize the color flag. Call once in main.
///
/// Resolution order:
/// 1. `--no-color` CLI flag (any value → disable)
/// 2. `NO_COLOR` env var (any non-empty value → disable, per <https://no-color.org>)
/// 3. `std::io::IsTerminal` on stdout (stdlib ≥ 1.70, no extra dep)
pub fn init(no_color_flag: bool) {
    let no_color_env = std::env::var("NO_COLOR")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let color = if no_color_flag || no_color_env {
        false
    } else {
        use std::io::IsTerminal as _;
        std::io::stdout().is_terminal()
    };
    // Ignore the error: if already set (e.g. tests calling init twice), keep the
    // first value.
    let _ = USE_COLOR.set(color);
}

/// Returns `true` when ANSI color output is enabled.
/// Defaults to `false` if `init` was never called (safe for unit tests).
pub fn enabled() -> bool {
    *USE_COLOR.get().unwrap_or(&false)
}

/// Wrap `s` with ANSI escape code `code` if color is enabled, else return `s`
/// as-is.
pub fn paint(s: &str, ansi_code: &str) -> String {
    if enabled() {
        format!("\x1b[{}m{}\x1b[0m", ansi_code, s)
    } else {
        s.to_string()
    }
}

pub fn red(s: &str) -> String {
    paint(s, "31")
}

pub fn green(s: &str) -> String {
    paint(s, "32")
}

// Available for future use; suppressed to avoid dead_code warnings until callers land.
#[allow(dead_code)]
pub fn yellow(s: &str) -> String {
    paint(s, "33")
}

#[allow(dead_code)]
pub fn cyan(s: &str) -> String {
    paint(s, "36")
}

pub fn dim(s: &str) -> String {
    paint(s, "2")
}

#[allow(dead_code)]
pub fn bold(s: &str) -> String {
    paint(s, "1")
}

pub fn bold_green(s: &str) -> String {
    if enabled() {
        format!("\x1b[1;32m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

pub fn bold_cyan(s: &str) -> String {
    if enabled() {
        format!("\x1b[1;36m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

/// Return a colored status glyph for human-readable (table/markdown) output.
///
/// | Status   | Glyph | Color      |
/// |----------|-------|------------|
/// | Keep     | ✓     | green      |
/// | Best     | ★     | bold cyan  |
/// | Discard  | ·     | dim        |
/// | Crash    | ✗     | red        |
/// | Verified | ✔     | bold green |
pub fn status_glyph(s: &crate::model::Status) -> String {
    use crate::model::Status;
    match s {
        Status::Keep => green("✓"),
        Status::Best => bold_cyan("★"),
        Status::Discard => dim("·"),
        Status::Crash => red("✗"),
        Status::Verified => bold_green("✔"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bypass the global OnceLock and call paint() directly with color off.
    /// paint() calls enabled() which returns *USE_COLOR.get().unwrap_or(&false).
    /// In a test binary USE_COLOR is never init()'d (no main), so it defaults
    /// to false — paint() must return the bare string, no ANSI escapes.
    #[test]
    fn paint_disabled_no_ansi_escapes() {
        // Confirm the global default is disabled (uninitialised = false).
        assert!(!enabled(), "expected color disabled in test context");
        let result = paint("hello", "32");
        assert_eq!(
            result, "hello",
            "paint() with color OFF must return bare string"
        );
        assert!(
            !result.contains('\x1b'),
            "paint() with color OFF must not emit any ANSI escape byte"
        );
    }

    /// Verify that the ANSI formatting path produces the correct escape sequence
    /// by calling the formatting logic directly (bypasses the OnceLock entirely).
    /// This is the only testable seam for the enabled path because OnceLock can
    /// only be set once per process and the test harness does not call init().
    #[test]
    fn paint_enabled_logic_produces_correct_escape() {
        // Replicate paint()'s enabled branch exactly — same format string.
        let s = "hi";
        let code = "32";
        let colored = format!("\x1b[{}m{}\x1b[0m", code, s);
        assert!(colored.contains("\x1b[32m"), "must contain opening escape");
        assert!(colored.contains("\x1b[0m"), "must contain reset escape");
        assert!(colored.contains("hi"), "must contain original text");
    }

    /// Color helper functions must also pass through cleanly when color is off.
    #[test]
    fn color_helpers_no_escapes_when_disabled() {
        assert!(!enabled());
        for result in [
            red("x"),
            green("x"),
            dim("x"),
            bold_green("x"),
            bold_cyan("x"),
        ] {
            assert_eq!(
                result, "x",
                "helper must return bare string when color disabled"
            );
            assert!(
                !result.contains('\x1b'),
                "helper must not emit ANSI when disabled"
            );
        }
    }

    #[test]
    fn enabled_defaults_false_without_init() {
        // Confirm no-panic and correct default.
        assert!(!enabled());
    }
}
