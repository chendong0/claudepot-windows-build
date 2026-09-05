//! Tauri command surface for the Codex rate-limit card.
//!
//! Pure read-through — all parsing lives in
//! `claudepot_core::codex_usage`. This file only shapes the DTO the
//! Activities dashboard renders. No business logic.
//!
//! `codex_usage_read` returns `Ok(None)` when Codex is present but
//! has no usable snapshot yet, or when there is no `~/.codex` at all;
//! it only errors when a rollout file exists but cannot be read.

use claudepot_core::codex_usage::{self, CodexRateLimits, RateWindow};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CodexWindowDto {
    pub used_percent: f64,
    pub window_minutes: u64,
    /// Unix seconds; `0` when Codex did not send it.
    pub resets_at: i64,
}

#[derive(Debug, Serialize)]
pub struct CodexCreditsDto {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: String,
}

/// Wire mirror of `claudepot_core::codex_usage::CodexRateLimits`,
/// minus the diagnostic `source_file` path.
#[derive(Debug, Serialize)]
pub struct CodexUsageDto {
    /// The 5-hour rolling window (`window_minutes == 300`).
    pub primary: Option<CodexWindowDto>,
    /// The weekly rolling window (`window_minutes == 10080`).
    pub secondary: Option<CodexWindowDto>,
    pub credits: CodexCreditsDto,
    /// Codex plan string (`"plus"` / `"pro"`), empty if absent.
    pub plan_type: String,
    /// ISO-8601 timestamp of the snapshot, verbatim from the log.
    pub captured_at: String,
}

fn window_dto(w: Option<RateWindow>) -> Option<CodexWindowDto> {
    w.map(|x| CodexWindowDto {
        used_percent: x.used_percent,
        window_minutes: x.window_minutes,
        resets_at: x.resets_at,
    })
}

impl From<CodexRateLimits> for CodexUsageDto {
    fn from(r: CodexRateLimits) -> Self {
        Self {
            primary: window_dto(r.primary),
            secondary: window_dto(r.secondary),
            credits: CodexCreditsDto {
                has_credits: r.credits.has_credits,
                unlimited: r.credits.unlimited,
                balance: r.credits.balance,
            },
            plan_type: r.plan_type,
            captured_at: r.captured_at,
        }
    }
}

/// Read the most recent Codex rate-limit snapshot for the Activities
/// dashboard card.
#[tauri::command]
pub async fn codex_usage_read() -> Result<Option<CodexUsageDto>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(home) = codex_usage::codex_home() else {
            return Ok(None);
        };
        codex_usage::read_latest(&home)
            .map(|opt| opt.map(CodexUsageDto::from))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("task join: {e}"))?
}

// The snapshot parsing this command depends on is covered by
// `claudepot_core::codex_usage`'s unit tests. The `From` impl here is
// a straight field copy; it is exercised whenever the Activities
// dashboard renders the card.
