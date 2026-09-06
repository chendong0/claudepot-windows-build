//! Aggregate stats over *all* Codex CLI rollout logs — lifetime token
//! totals, activity streaks, and a per-model usage breakdown with a
//! **rough** API-equivalent dollar estimate.
//!
//! Read-only. Scans every `~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-*.jsonl`,
//! takes each session's final `total_token_usage` snapshot plus its
//! first/last timestamp, and rolls the lot up.
//!
//! # Caveats the caller must surface
//!
//! - **Local only.** These numbers cover sessions that wrote a rollout
//!   file on this machine. The ChatGPT app's profile page counts web
//!   and mobile use too, so its "cumulative tokens" is higher.
//! - **The dollar figure is a crude estimate.** Codex Plus is a
//!   subscription — you do not pay per token — so "cost" here means
//!   "what pay-per-call would have cost". The models Codex uses
//!   (`gpt-5.6-*`, `gpt-6-*`, …) have no price table we can trust, so
//!   every model is scored at one flat GPT-4-class rate
//!   ([`RATE`]). Treat it as an order-of-magnitude hint, not a bill.
//! - **"Longest session" is first-to-last span**, so a session left
//!   idle for days and resumed reads as very long.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::codex_usage::CodexUsageError;

/// Flat GPT-4-class rate used for every model, in USD per 1M tokens.
/// Not model-specific — see the module caveats.
pub struct Rate {
    pub input_per_mtok: f64,
    pub cached_input_per_mtok: f64,
    pub output_per_mtok: f64,
}
pub const RATE: Rate = Rate {
    input_per_mtok: 2.50,
    cached_input_per_mtok: 0.31,
    output_per_mtok: 10.00,
};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TokenBreakdown {
    pub input: u64,
    /// Subset of `input` served from cache (cheaper).
    pub cached_input: u64,
    pub cache_write: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total: u64,
}

impl TokenBreakdown {
    fn add(&mut self, o: &TokenBreakdown) {
        self.input += o.input;
        self.cached_input += o.cached_input;
        self.cache_write += o.cache_write;
        self.output += o.output;
        self.reasoning_output += o.reasoning_output;
        self.total += o.total;
    }

    /// Rough API-equivalent cost at the flat [`RATE`]. Uncached input
    /// billed at the input rate, cached input at the cached rate,
    /// output + reasoning at the output rate.
    fn approx_usd(&self) -> f64 {
        let uncached_input = self.input.saturating_sub(self.cached_input) as f64;
        let cached = self.cached_input as f64;
        let out = (self.output + self.reasoning_output) as f64;
        (uncached_input * RATE.input_per_mtok
            + cached * RATE.cached_input_per_mtok
            + out * RATE.output_per_mtok)
            / 1_000_000.0
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub sessions: usize,
    pub tokens: TokenBreakdown,
    pub approx_usd: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WindowUsage {
    /// `"5h"`, `"7d"`, `"30d"`, `"all"`.
    pub window: String,
    pub sessions: usize,
    pub tokens: TokenBreakdown,
    pub approx_usd: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexStats {
    // ── lifetime ──────────────────────────────────────────────
    pub total_tokens: u64,
    pub session_count: usize,
    pub active_days: usize,
    pub current_streak_days: usize,
    pub longest_streak_days: usize,
    pub peak_day_tokens: u64,
    /// ISO date (UTC) of the earliest / latest session seen. Empty
    /// when there is no data.
    pub first_seen: String,
    pub last_seen: String,
    /// First-to-last span of the single longest session, in seconds.
    pub longest_session_secs: i64,
    // ── breakdowns ───────────────────────────────────────────
    pub by_model: Vec<ModelUsage>,
    pub windows: Vec<WindowUsage>,
    /// True when the dollar figures used the flat fallback rate for at
    /// least one model (always true today — kept for when a real
    /// per-model table lands).
    pub cost_is_estimate: bool,
}

/// One session's contribution, extracted from its rollout file.
struct SessionRow {
    model: String,
    tokens: TokenBreakdown,
    /// Unix seconds of the first line's timestamp.
    started_at: i64,
    ended_at: i64,
    /// UTC `YYYY-MM-DD` of `started_at`.
    day: String,
}

fn parse_rfc3339_secs(s: &str) -> Option<i64> {
    // `2026-09-06T08:17:39.819Z` — lean parser, no chrono needed here.
    // Fields: YYYY-MM-DDThh:mm:ss(.fff)?Z
    let b = s.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let num = |a: usize, z: usize| s.get(a..z)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, se) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    // days-from-civil (Howard Hinnant's algorithm)
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

fn extract_session(path: &Path) -> Option<SessionRow> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut model: Option<String> = None;
    let mut last_tokens: Option<TokenBreakdown> = None;
    let mut first_ts: Option<i64> = None;
    let mut first_iso: Option<String> = None;
    let mut last_ts: Option<i64> = None;

    for line in text.lines() {
        if model.is_none() {
            if let Some(m) = find_json_str(line, "\"model\":") {
                model = Some(m);
            }
        }
        let has_ts = line.contains("\"timestamp\"");
        let has_tok = line.contains("\"total_token_usage\"");
        if !has_ts && !has_tok {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
            if let Some(secs) = parse_rfc3339_secs(ts) {
                if first_ts.is_none() {
                    first_ts = Some(secs);
                    first_iso = Some(ts.to_string());
                }
                last_ts = Some(secs);
            }
        }
        if let Some(tu) = v
            .get("payload")
            .and_then(|p| p.get("info"))
            .and_then(|i| i.get("total_token_usage"))
        {
            let g = |k: &str| tu.get(k).and_then(|n| n.as_u64()).unwrap_or(0);
            last_tokens = Some(TokenBreakdown {
                input: g("input_tokens"),
                cached_input: g("cached_input_tokens"),
                cache_write: g("cache_write_input_tokens"),
                output: g("output_tokens"),
                reasoning_output: g("reasoning_output_tokens"),
                total: g("total_tokens"),
            });
        }
    }

    let tokens = last_tokens?;
    let started_at = first_ts?;
    let day = first_iso?.get(0..10)?.to_string();
    Some(SessionRow {
        model: model.unwrap_or_else(|| "unknown".to_string()),
        tokens,
        started_at,
        ended_at: last_ts.unwrap_or(started_at),
        day,
    })
}

/// Find `"<key>": "value"` and return `value`. `key` includes the
/// trailing colon, e.g. `"\"model\":"`.
fn find_json_str(line: &str, key: &str) -> Option<String> {
    let i = line.find(key)? + key.len();
    let rest = line[i..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Walk `sessions/<yyyy>/<mm>/<dd>/` three levels, returning every
/// rollout path.
fn all_rollouts(home: &Path) -> Result<Vec<std::path::PathBuf>, CodexUsageError> {
    let root = home.join("sessions");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let dirs = |d: &Path| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    };
    for y in dirs(&root) {
        for m in dirs(&y) {
            for day in dirs(&m) {
                let rd = std::fs::read_dir(&day).map_err(|source| CodexUsageError::Io {
                    path: day.clone(),
                    source,
                })?;
                for ent in rd.flatten() {
                    let p = ent.path();
                    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if n.starts_with("rollout-") && n.ends_with(".jsonl") {
                        out.push(p);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Consecutive-day run ending at the most recent active day, and the
/// longest run anywhere. Days are UTC `YYYY-MM-DD` strings.
fn streaks(days: &BTreeSet<String>) -> (usize, usize) {
    if days.is_empty() {
        return (0, 0);
    }
    let as_num = |s: &str| parse_rfc3339_secs(&format!("{s}T00:00:00Z")).unwrap_or(0) / 86400;
    let nums: Vec<i64> = days.iter().map(|d| as_num(d)).collect();
    let mut longest = 1;
    let mut run = 1;
    for w in nums.windows(2) {
        if w[1] == w[0] + 1 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
    }
    // current streak: walk back from the last active day
    let mut current = 1;
    for w in nums.windows(2).rev() {
        if w[1] == w[0] + 1 {
            current += 1;
        } else {
            break;
        }
    }
    (current, longest)
}

/// Scan every rollout under `home` and roll up lifetime + windowed
/// stats. `now_secs` is the reference "now" for the rolling windows
/// (unix seconds), injected so tests are deterministic.
pub fn read_all(home: &Path, now_secs: i64) -> Result<CodexStats, CodexUsageError> {
    let mut rows: Vec<SessionRow> = Vec::new();
    for p in all_rollouts(home)? {
        if let Some(r) = extract_session(&p) {
            rows.push(r);
        }
    }

    let mut by_model_map: BTreeMap<String, (usize, TokenBreakdown)> = BTreeMap::new();
    let mut day_tokens: BTreeMap<String, u64> = BTreeMap::new();
    let mut days: BTreeSet<String> = BTreeSet::new();
    let mut total_tokens = 0u64;
    let mut longest_session_secs = 0i64;
    let (mut first_seen, mut last_seen) = (String::new(), String::new());

    let windows = [("5h", 5 * 3600), ("7d", 7 * 86400), ("30d", 30 * 86400)];
    let mut win_acc: Vec<(usize, TokenBreakdown)> = windows
        .iter()
        .map(|_| (0, TokenBreakdown::default()))
        .collect();
    let mut all_acc = (0usize, TokenBreakdown::default());

    for r in &rows {
        total_tokens += r.tokens.total;
        days.insert(r.day.clone());
        *day_tokens.entry(r.day.clone()).or_default() += r.tokens.total;
        longest_session_secs = longest_session_secs.max(r.ended_at - r.started_at);
        if first_seen.is_empty() || r.day < first_seen {
            first_seen = r.day.clone();
        }
        if r.day > last_seen {
            last_seen = r.day.clone();
        }

        let e = by_model_map
            .entry(r.model.clone())
            .or_insert((0, TokenBreakdown::default()));
        e.0 += 1;
        e.1.add(&r.tokens);

        all_acc.0 += 1;
        all_acc.1.add(&r.tokens);
        for (i, (_, secs)) in windows.iter().enumerate() {
            if r.started_at >= now_secs - secs {
                win_acc[i].0 += 1;
                win_acc[i].1.add(&r.tokens);
            }
        }
    }

    let (current_streak_days, longest_streak_days) = streaks(&days);

    let mut by_model: Vec<ModelUsage> = by_model_map
        .into_iter()
        .map(|(model, (sessions, tokens))| {
            let approx_usd = tokens.approx_usd();
            ModelUsage {
                model,
                sessions,
                tokens,
                approx_usd,
            }
        })
        .collect();
    by_model.sort_by(|a, b| b.tokens.total.cmp(&a.tokens.total));

    let mut win_list: Vec<WindowUsage> = windows
        .iter()
        .zip(win_acc)
        .map(|((name, _), (sessions, tokens))| {
            let approx_usd = tokens.approx_usd();
            WindowUsage {
                window: name.to_string(),
                sessions,
                tokens,
                approx_usd,
            }
        })
        .collect();
    win_list.push(WindowUsage {
        window: "all".to_string(),
        sessions: all_acc.0,
        approx_usd: all_acc.1.approx_usd(),
        tokens: all_acc.1,
    });

    Ok(CodexStats {
        total_tokens,
        session_count: rows.len(),
        active_days: days.len(),
        current_streak_days,
        longest_streak_days,
        peak_day_tokens: day_tokens.values().copied().max().unwrap_or(0),
        first_seen,
        last_seen,
        longest_session_secs,
        by_model,
        windows: win_list,
        cost_is_estimate: true,
    })
}

impl Default for TokenBreakdown {
    fn default() -> Self {
        Self {
            input: 0,
            cached_input: 0,
            cache_write: 0,
            output: 0,
            reasoning_output: 0,
            total: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: &str, model: &str, total: u64, inp: u64, cached: u64, out: u64) -> String {
        format!(
            r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{inp},"cached_input_tokens":{cached},"cache_write_input_tokens":0,"output_tokens":{out},"reasoning_output_tokens":0,"total_tokens":{total}}}}}}},"model":"{model}"}}"#
        )
    }

    fn write_session(dir: &Path, name: &str, lines: &[String]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), lines.join("\n")).unwrap();
    }

    #[test]
    fn parse_rfc3339_secs_matches_known_epoch() {
        // 2026-09-06T08:17:39Z
        assert_eq!(parse_rfc3339_secs("2026-09-06T08:17:39.819Z"), Some(1788682659));
        assert_eq!(parse_rfc3339_secs("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn rolls_up_lifetime_and_per_model() {
        let tmp = std::env::temp_dir().join(format!("codex-stats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let s = tmp.join("sessions");
        write_session(
            &s.join("2026/09/05"),
            "rollout-a.jsonl",
            &[
                line("2026-09-05T01:00:00Z", "gpt-x", 100, 60, 20, 40),
                line("2026-09-05T01:30:00Z", "gpt-x", 300, 200, 50, 100),
            ],
        );
        write_session(
            &s.join("2026/09/06"),
            "rollout-b.jsonl",
            &[line("2026-09-06T02:00:00Z", "gpt-y", 500, 400, 0, 100)],
        );
        // now = 2026-09-06T03:00:00Z
        let now = parse_rfc3339_secs("2026-09-06T03:00:00Z").unwrap();
        let st = read_all(&tmp, now).unwrap();

        assert_eq!(st.session_count, 2);
        assert_eq!(st.total_tokens, 800); // 300 (final of a) + 500 (b)
        assert_eq!(st.active_days, 2);
        assert_eq!(st.current_streak_days, 2);
        assert_eq!(st.longest_streak_days, 2);
        assert_eq!(st.peak_day_tokens, 500); // 09-06
        assert_eq!(st.first_seen, "2026-09-05");
        assert_eq!(st.last_seen, "2026-09-06");
        assert_eq!(st.by_model.len(), 2);
        assert_eq!(st.by_model[0].model, "gpt-y"); // 500 > 300
        assert_eq!(st.by_model[0].tokens.total, 500);
        assert_eq!(st.by_model[1].tokens.total, 300);

        // windows: 7d/30d/all catch both; 5h catches only b (a started 26h before now)
        let w = |name: &str| st.windows.iter().find(|w| w.window == name).unwrap();
        assert_eq!(w("all").sessions, 2);
        assert_eq!(w("7d").sessions, 2);
        assert_eq!(w("5h").sessions, 1);
        assert_eq!(w("5h").tokens.total, 500);
        assert!(w("all").approx_usd > 0.0);
        assert!(st.cost_is_estimate);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn empty_home_is_all_zero() {
        let tmp = std::env::temp_dir().join(format!("codex-stats-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let st = read_all(&tmp, 0).unwrap();
        assert_eq!(st.session_count, 0);
        assert_eq!(st.total_tokens, 0);
        assert_eq!(st.active_days, 0);
        assert_eq!(st.current_streak_days, 0);
        assert!(st.by_model.is_empty());
        assert_eq!(st.windows.len(), 4);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn streaks_finds_current_and_longest() {
        let days: BTreeSet<String> = ["2026-08-01", "2026-08-02", "2026-08-03", "2026-08-10",
            "2026-08-11"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // longest run = Aug 1-3 (3); current run = Aug 10-11 (2)
        assert_eq!(streaks(&days), (2, 3));
    }

    #[test]
    fn approx_usd_prices_cached_cheaper_than_fresh() {
        let all_fresh = TokenBreakdown {
            input: 1_000_000,
            cached_input: 0,
            cache_write: 0,
            output: 0,
            reasoning_output: 0,
            total: 1_000_000,
        };
        let all_cached = TokenBreakdown {
            cached_input: 1_000_000,
            ..all_fresh.clone()
        };
        assert!(all_cached.approx_usd() < all_fresh.approx_usd());
        assert!((all_fresh.approx_usd() - RATE.input_per_mtok).abs() < 1e-9);
    }
}
