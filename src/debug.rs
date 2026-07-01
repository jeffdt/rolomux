//! Opt-in diagnostic logging, off unless `SMUX_DEBUG` is set in the environment.
//!
//! smux takes over the alternate screen, so anything printed to stderr during a
//! normal run is either lost or corrupts the TUI. To debug field reports we
//! instead append timestamped lines to a file (`SMUX_LOG`, else
//! `/tmp/smux-debug.log`) whenever `SMUX_DEBUG` is set to a non-empty value. All
//! call sites live outside the TUI's active window (gather runs before the
//! screen is entered; action dispatch runs after it is torn down), so the log is
//! safe and self-contained.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Whether debug logging is enabled for this process.
pub fn enabled() -> bool {
    std::env::var("SMUX_DEBUG").map(|v| !v.is_empty()).unwrap_or(false)
}

fn log_path() -> String {
    std::env::var("SMUX_LOG")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/tmp/smux-debug.log".to_string())
}

/// Append one diagnostic line. A no-op (and never allocates the message) unless
/// `SMUX_DEBUG` is set. Failures to write are swallowed: logging must never take
/// down the picker.
pub fn log(msg: impl FnOnce() -> String) {
    if !enabled() {
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "[{ts}] {}", msg());
    }
}
