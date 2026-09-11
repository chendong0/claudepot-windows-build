import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import type { CodexUsageDto } from "../../types";

// Narrow API mock — the card only probes `codexUsageRead` / `codexStatsRead`
// on mount and on its 60s tick.
const usageSpy = vi.fn();
const statsSpy = vi.fn();
vi.mock("../../api", () => ({
  api: {
    codexUsageRead: (...args: unknown[]) => usageSpy(...args),
    codexStatsRead: (...args: unknown[]) => statsSpy(...args),
  },
}));

import { CodexUsageCard } from "./CodexUsageCard";

function mkUsage(overrides: Partial<CodexUsageDto> = {}): CodexUsageDto {
  return {
    primary: { used_percent: 52, window_minutes: 300, resets_at: 0 },
    secondary: { used_percent: 29, window_minutes: 10080, resets_at: 0 },
    credits: { has_credits: false, unlimited: false, balance: "0" },
    plan_type: "plus",
    captured_at: new Date().toISOString(),
    ...overrides,
  };
}

describe("CodexUsageCard — stale window handling", () => {
  it("renders a live reading in its normal danger/accent band when the window has not reset", async () => {
    usageSpy.mockResolvedValue(
      mkUsage({
        primary: {
          used_percent: 99,
          window_minutes: 300,
          // Resets an hour from now — still the current window.
          resets_at: Math.floor(Date.now() / 1000) + 3600,
        },
      }),
    );
    statsSpy.mockResolvedValue(null);

    render(<CodexUsageCard />);

    await waitFor(() => expect(screen.getByText("99%")).toBeInTheDocument());
    // No stale marker on a live reading.
    expect(screen.queryByText("~99%")).not.toBeInTheDocument();
  });

  it("marks a window's reading stale once its resets_at has passed", async () => {
    usageSpy.mockResolvedValue(
      mkUsage({
        primary: {
          used_percent: 99,
          window_minutes: 300,
          // Reset already happened — Codex hasn't recorded a fresh
          // snapshot since (the CLI hasn't run a turn), so 99% is a
          // reading of a window that no longer exists.
          resets_at: Math.floor(Date.now() / 1000) - 3600,
        },
      }),
    );
    statsSpy.mockResolvedValue(null);

    render(<CodexUsageCard />);

    await waitFor(() =>
      expect(screen.getByText(/~99%/)).toBeInTheDocument(),
    );
    // The stale pct carries an explanatory tooltip rather than
    // asserting the number as current.
    expect(screen.getByText(/~99%/).closest("span")).toHaveAttribute(
      "title",
      expect.stringMatching(/already reset/i),
    );
  });

  it("does not mark a window stale when resets_at is unknown (0)", async () => {
    usageSpy.mockResolvedValue(
      mkUsage({
        primary: { used_percent: 40, window_minutes: 300, resets_at: 0 },
      }),
    );
    statsSpy.mockResolvedValue(null);

    render(<CodexUsageCard />);

    await waitFor(() => expect(screen.getByText("40%")).toBeInTheDocument());
    expect(screen.queryByText(/~40%/)).not.toBeInTheDocument();
  });
});
