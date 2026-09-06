// Codex CLI rate-limit snapshot — mirror of
// `src-tauri/src/commands/codex_usage.rs` DTOs.

export interface CodexWindowDto {
  used_percent: number;
  window_minutes: number;
  /** Unix seconds at which the window rolls over; 0 if unknown. */
  resets_at: number;
}

export interface CodexCreditsDto {
  has_credits: boolean;
  unlimited: boolean;
  /** Codex sends the balance as a string. */
  balance: string;
}

export interface CodexUsageDto {
  /** The 5-hour rolling window (window_minutes === 300). */
  primary: CodexWindowDto | null;
  /** The weekly rolling window (window_minutes === 10080). */
  secondary: CodexWindowDto | null;
  credits: CodexCreditsDto;
  /** Codex plan string ("plus" / "pro"); empty if absent. */
  plan_type: string;
  /** ISO-8601 timestamp of the snapshot line. */
  captured_at: string;
}

// ── aggregate stats over all Codex rollout logs (codex_stats_read) ──

export interface CodexTokenBreakdown {
  input: number;
  cached_input: number;
  cache_write: number;
  output: number;
  reasoning_output: number;
  total: number;
}

export interface CodexModelUsage {
  model: string;
  sessions: number;
  tokens: CodexTokenBreakdown;
  approx_usd: number;
}

export interface CodexWindowUsage {
  /** "5h" | "7d" | "30d" | "all" */
  window: string;
  sessions: number;
  tokens: CodexTokenBreakdown;
  approx_usd: number;
}

export interface CodexStatsDto {
  total_tokens: number;
  session_count: number;
  active_days: number;
  current_streak_days: number;
  longest_streak_days: number;
  peak_day_tokens: number;
  first_seen: string;
  last_seen: string;
  longest_session_secs: number;
  by_model: CodexModelUsage[];
  windows: CodexWindowUsage[];
  /** Always true today — the $ figure uses one flat GPT-4-class rate. */
  cost_is_estimate: boolean;
}
