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
