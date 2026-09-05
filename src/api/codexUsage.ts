// Codex CLI usage snapshot. Sharded from src/api.ts;
// src/api/index.ts merges every domain slice into `api`.

import { invoke } from "@tauri-apps/api/core";
import type { CodexUsageDto } from "../types";

export const codexUsageApi = {
  /**
   * Most recent Codex rate-limit snapshot (5h / 7d windows + overage
   * credits), read from the newest `~/.codex/sessions/**` rollout
   * log. Resolves to `null` when Codex has produced no snapshot yet
   * or is not installed. Rejects only when a rollout file exists but
   * cannot be read.
   */
  codexUsageRead(): Promise<CodexUsageDto | null> {
    return invoke("codex_usage_read");
  },
};
