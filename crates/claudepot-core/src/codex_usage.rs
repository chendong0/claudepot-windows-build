//! Codex CLI usage / rate-limit snapshot — read-only.
//!
//! The `openai/codex` CLI records a rate-limit snapshot into its
//! per-session rollout log after most turns. Each snapshot is a
//! `type: "event_msg"` line whose `payload.type` is `"token_count"`
//! and whose `payload.rate_limits` carries two rolling windows plus
//! any pay-as-you-go overage state:
//!
//! | field       | meaning                                    |
//! |-------------|-------------------------------------------|
//! | `primary`   | the 5-hour window  (`window_minutes` 300) |
//! | `secondary` | the weekly window  (`window_minutes` 10080) |
//! | `credits`   | overage-credit balance                    |
//!
//! Codex writes no dedicated "current usage" file, so this module
//! finds the newest `sessions/<yyyy>/<mm>/<dd>/rollout-*.jsonl` and
//! scans it backwards for the last line carrying `rate_limits`. That
//! line is authoritative: Codex only ever appends, so the last
//! snapshot in the newest session is the most recent one the CLI has
//! seen. Nothing here writes to `~/.codex`.
//!
//! `read_latest` returns `Ok(None)` — not an error — when Codex is
//! present but has produced no snapshot yet (no sessions, or none
//! carrying `rate_limits`). The caller renders that as "no data yet".

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Failures that stop us from even looking. A missing `sessions/`
/// directory is *not* one of these — that is `Ok(None)`.
#[derive(Debug, thiserror::Error)]
pub enum CodexUsageError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// One rolling-limit window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateWindow {
    /// Percent of the window consumed. Codex can report slightly over
    /// 100 right at the cap; callers clamp for display, not here.
    pub used_percent: f64,
    /// Window length in minutes (`300` = 5h, `10080` = 7d).
    #[serde(default)]
    pub window_minutes: u64,
    /// Unix seconds at which this window rolls over. `0` when Codex
    /// did not send it.
    #[serde(default)]
    pub resets_at: i64,
}

/// Pay-as-you-go overage state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Credits {
    #[serde(default)]
    pub has_credits: bool,
    #[serde(default)]
    pub unlimited: bool,
    /// Codex sends the balance as a string; passed through verbatim.
    #[serde(default)]
    pub balance: String,
}

/// A full snapshot, as read from the newest rollout log.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexRateLimits {
    /// 5-hour window. `None` if Codex omitted it.
    pub primary: Option<RateWindow>,
    /// Weekly window. `None` if Codex omitted it.
    pub secondary: Option<RateWindow>,
    pub credits: Credits,
    /// Codex plan string, e.g. `"plus"` / `"pro"`. Empty if absent.
    pub plan_type: String,
    /// ISO-8601 timestamp of the snapshot line, verbatim from the log.
    pub captured_at: String,
    /// The rollout file the snapshot came from (diagnostics only).
    pub source_file: PathBuf,
}

// ─── log line shapes (only the fields we consume) ──────────────────

#[derive(Deserialize)]
struct RolloutLine {
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(default)]
    rate_limits: Option<RateLimitsRaw>,
}

#[derive(Deserialize)]
struct RateLimitsRaw {
    #[serde(default)]
    primary: Option<RateWindow>,
    #[serde(default)]
    secondary: Option<RateWindow>,
    #[serde(default)]
    credits: Option<Credits>,
    #[serde(default)]
    plan_type: Option<String>,
}

/// `$CODEX_HOME`, else `~/.codex`. `None` only when the home
/// directory itself cannot be resolved.
pub fn codex_home() -> Option<PathBuf> {
    match std::env::var("CODEX_HOME") {
        Ok(h) if !h.is_empty() => Some(PathBuf::from(h)),
        _ => dirs::home_dir().map(|h| h.join(".codex")),
    }
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>, CodexUsageError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| CodexUsageError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .flatten()
        .map(|e| e.path())
        .collect();
    v.sort();
    v
        .retain(|p| p.is_dir());
    Ok(v)
}

/// How many of the most-recently-modified rollout files to open when
/// hunting for the freshest snapshot. A resumed old session can have
/// a *newer file mtime* than the session that last recorded a
/// `rate_limits` line, so picking "newest file by mtime" and reading
/// only that one returned a stale reading. Scanning a handful and
/// choosing by the snapshot's own timestamp fixes it; 25 is well
/// past any realistic number of sessions touched between two Codex
/// rate-limit reports.
const SCAN_FILES: usize = 25;

/// Collect every `rollout-*.jsonl` under `<home>/sessions/`, newest
/// file mtime first, capped at [`SCAN_FILES`]. Codex lays these out
/// as `sessions/<yyyy>/<mm>/<dd>/…`; we walk exactly those three
/// levels rather than a full recursive scan so an unrelated deep
/// tree under `sessions/` cannot slow this to a crawl.
fn recent_rollouts(home: &Path) -> Result<Vec<PathBuf>, CodexUsageError> {
    let root = home.join("sessions");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut all: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for year in read_dir_sorted(&root)? {
        for month in read_dir_sorted(&year)? {
            for day in read_dir_sorted(&month)? {
                let entries = std::fs::read_dir(&day).map_err(|source| CodexUsageError::Io {
                    path: day.clone(),
                    source,
                })?;
                for ent in entries.flatten() {
                    let path = ent.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
                        continue;
                    }
                    let mtime = ent
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    all.push((mtime, path));
                }
            }
        }
    }
    all.sort_by(|a, b| b.0.cmp(&a.0));
    all.truncate(SCAN_FILES);
    Ok(all.into_iter().map(|(_, p)| p).collect())
}

/// Pull the last `rate_limits` snapshot out of one rollout file.
/// Scans from the end; the `contains` pre-filter keeps us from
/// JSON-parsing every unrelated line in a long transcript.
pub fn parse_latest_snapshot(path: &Path) -> Result<Option<CodexRateLimits>, CodexUsageError> {
    let text = std::fs::read_to_string(path).map_err(|source| CodexUsageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(extract_from_text(&text, path))
}

fn extract_from_text(text: &str, path: &Path) -> Option<CodexRateLimits> {
    for line in text.lines().rev() {
        let line = line.trim();
        if line.is_empty() || !line.contains("\"rate_limits\"") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<RolloutLine>(line) else {
            continue;
        };
        let Some(raw) = parsed.payload.and_then(|p| p.rate_limits) else {
            continue;
        };
        // A line can carry `"rate_limits": null`; skip to an older one
        // that actually has windows.
        if raw.primary.is_none() && raw.secondary.is_none() {
            continue;
        }
        return Some(CodexRateLimits {
            primary: raw.primary,
            secondary: raw.secondary,
            credits: raw.credits.unwrap_or_default(),
            plan_type: raw.plan_type.unwrap_or_default(),
            captured_at: parsed.timestamp,
            source_file: path.to_path_buf(),
        });
    }
    None
}

/// Read the most recent Codex rate-limit snapshot from `home`
/// (usually [`codex_home`]). Scans the most-recently-modified rollout
/// files and returns the snapshot with the newest **internal**
/// timestamp — not the newest file — so a resumed old session cannot
/// shadow a fresher reading. `Ok(None)` when no rollout carries a
/// usable `rate_limits` line.
pub fn read_latest(home: &Path) -> Result<Option<CodexRateLimits>, CodexUsageError> {
    let mut best: Option<CodexRateLimits> = None;
    for path in recent_rollouts(home)? {
        let Some(snap) = parse_latest_snapshot(&path)? else {
            continue;
        };
        // `captured_at` is ISO-8601 with a fixed shape
        // (`2026-09-06T08:17:39.819Z`), so lexical order is
        // chronological order.
        let is_newer = best
            .as_ref()
            .is_none_or(|b| snap.captured_at > b.captured_at);
        if is_newer {
            best = Some(snap);
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"timestamp":"2026-09-05T15:12:34.748Z","ordinal":27,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1}},"rate_limits":{"limit_id":"codex","limit_name":null,"primary":{"used_percent":52.0,"window_minutes":300,"resets_at":1788633169},"secondary":{"used_percent":29.0,"window_minutes":10080,"resets_at":1788747963},"credits":{"has_credits":false,"unlimited":false,"balance":"0"},"plan_type":"plus"}}}"#;

    #[test]
    fn parses_a_real_snapshot_line() {
        let got = extract_from_text(LINE, Path::new("r.jsonl")).expect("snapshot");
        let primary = got.primary.expect("primary");
        assert_eq!(primary.used_percent, 52.0);
        assert_eq!(primary.window_minutes, 300);
        assert_eq!(primary.resets_at, 1788633169);
        let secondary = got.secondary.expect("secondary");
        assert_eq!(secondary.window_minutes, 10080);
        assert_eq!(secondary.used_percent, 29.0);
        assert!(!got.credits.has_credits);
        assert_eq!(got.credits.balance, "0");
        assert_eq!(got.plan_type, "plus");
        assert_eq!(got.captured_at, "2026-09-05T15:12:34.748Z");
    }

    #[test]
    fn takes_the_last_snapshot_when_several_are_present() {
        let older = LINE.replace("52.0", "10.0");
        let newer = LINE.replace("52.0", "88.0");
        let text = format!("{older}\n{{\"unrelated\":true}}\n{newer}\n");
        let got = extract_from_text(&text, Path::new("r.jsonl")).unwrap();
        assert_eq!(got.primary.unwrap().used_percent, 88.0);
    }

    #[test]
    fn ignores_lines_without_rate_limits_and_malformed_json() {
        let text = format!(
            "not json at all\n{{\"type\":\"event_msg\"}}\n{}\n{{ broken\n",
            LINE
        );
        let got = extract_from_text(&text, Path::new("r.jsonl")).unwrap();
        assert_eq!(got.primary.unwrap().used_percent, 52.0);
    }

    #[test]
    fn skips_a_null_rate_limits_line_and_falls_back() {
        let null_line = r#"{"timestamp":"t","type":"event_msg","payload":{"type":"token_count","rate_limits":null}}"#;
        let text = format!("{LINE}\n{null_line}\n");
        let got = extract_from_text(&text, Path::new("r.jsonl")).unwrap();
        // The null line is newest but unusable; we fall back to LINE.
        assert_eq!(got.primary.unwrap().used_percent, 52.0);
    }

    #[test]
    fn returns_none_when_no_snapshot_present() {
        let text = "{\"type\":\"event_msg\"}\n{\"foo\":1}\n";
        assert!(extract_from_text(text, Path::new("r.jsonl")).is_none());
    }

    #[test]
    fn read_latest_on_missing_sessions_dir_is_ok_none() {
        let tmp = std::env::temp_dir().join(format!("codex-usage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert_eq!(read_latest(&tmp).unwrap(), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Smoke test against this machine's real `~/.codex`. `#[ignore]`
    /// so CI never depends on Codex being installed; run with
    /// `cargo test -p claudepot-core --lib codex_usage -- --ignored`.
    #[test]
    #[ignore = "reads the developer's real ~/.codex"]
    fn read_latest_against_real_codex_home() {
        let home = codex_home().expect("home dir");
        match read_latest(&home).expect("read ok") {
            Some(s) => {
                eprintln!(
                    "primary={:?} secondary={:?} credits={:?} plan={} at={} from {}",
                    s.primary, s.secondary, s.credits, s.plan_type, s.captured_at,
                    s.source_file.display()
                );
                assert!(s.primary.is_some() || s.secondary.is_some());
            }
            None => eprintln!("no Codex snapshot on this machine (that is a valid result)"),
        }
    }

    #[test]
    fn read_latest_finds_newest_snapshot_across_day_dirs() {
        let tmp = std::env::temp_dir().join(format!("codex-usage-test-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let day_a = tmp.join("sessions/2026/09/04");
        let day_b = tmp.join("sessions/2026/09/05");
        std::fs::create_dir_all(&day_a).unwrap();
        std::fs::create_dir_all(&day_b).unwrap();
        let older = LINE
            .replace("52.0", "11.0")
            .replace("2026-09-05T15:12:34.748Z", "2026-09-04T10:00:00.000Z");
        let newer = LINE
            .replace("52.0", "77.0")
            .replace("2026-09-05T15:12:34.748Z", "2026-09-05T20:00:00.000Z");
        std::fs::write(day_a.join("rollout-a.jsonl"), older).unwrap();
        std::fs::write(day_b.join("rollout-b.jsonl"), newer).unwrap();
        let got = read_latest(&tmp).unwrap().expect("snapshot");
        assert_eq!(got.primary.unwrap().used_percent, 77.0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The bug this fixes: a resumed old session gets a fresh file
    /// mtime but its last `rate_limits` line is hours stale. Picking
    /// "newest file by mtime" then reading only that one returned the
    /// stale reading. `read_latest` must pick by the snapshot's own
    /// timestamp instead.
    #[test]
    fn read_latest_ignores_a_recently_touched_but_stale_rollout() {
        let tmp =
            std::env::temp_dir().join(format!("codex-usage-test-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let day = tmp.join("sessions/2026/09/06");
        std::fs::create_dir_all(&day).unwrap();

        // Fresh reading, written first (older mtime).
        let fresh = LINE
            .replace("52.0", "24.0")
            .replace("2026-09-05T15:12:34.748Z", "2026-09-06T16:17:39.000Z");
        std::fs::write(day.join("rollout-fresh.jsonl"), fresh).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));

        // Stale reading, written second so it has the NEWER mtime —
        // simulating a resumed day-old session.
        let stale = LINE
            .replace("52.0", "4.0")
            .replace("2026-09-05T15:12:34.748Z", "2026-09-05T09:00:00.000Z");
        std::fs::write(day.join("rollout-resumed-old.jsonl"), stale).unwrap();

        let got = read_latest(&tmp).unwrap().expect("snapshot");
        assert_eq!(
            got.primary.unwrap().used_percent,
            24.0,
            "should pick the fresher snapshot, not the newer-mtime file"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
