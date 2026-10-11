import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

import type { CodexAuthStatusDto, CodexUsageDto } from "../../types";

// Narrow API mock — the card only probes `codexUsageRead` / `codexStatsRead`
// on mount and on its 60s tick.
const usageSpy = vi.fn();
const statsSpy = vi.fn();
const authSpy = vi.fn();
const loginSpy = vi.fn();
vi.mock("../../api", () => ({
  api: {
    codexUsageRead: (...args: unknown[]) => usageSpy(...args),
    codexStatsRead: (...args: unknown[]) => statsSpy(...args),
    codexAuthStatus: (...args: unknown[]) => authSpy(...args),
    codexLogin: (...args: unknown[]) => loginSpy(...args),
    codexLogout: vi.fn(),
  },
}));

function mkAuth(overrides: Partial<CodexAuthStatusDto> = {}): CodexAuthStatusDto {
  return {
    installed: true,
    signed_in: true,
    auth_mode: "chatgpt",
    email: "me@example.com",
    plan_type: "plus",
    last_refresh: "",
    ...overrides,
  };
}

beforeEach(() => {
  authSpy.mockReset();
  loginSpy.mockReset();
  authSpy.mockResolvedValue(mkAuth());
});

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

describe("CodexUsageCard — sign-in", () => {
  it("shows the signed-in email and a re-sign-in button", async () => {
    usageSpy.mockResolvedValue(mkUsage());
    statsSpy.mockResolvedValue(null);

    render(<CodexUsageCard />);

    await waitFor(() =>
      expect(screen.getByText("me@example.com")).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: /re-sign in/i })).toBeInTheDocument();
  });

  it("still renders with a sign-in button when Codex has no snapshot and is signed out", async () => {
    usageSpy.mockResolvedValue(null);
    statsSpy.mockResolvedValue(null);
    authSpy.mockResolvedValue(mkAuth({ signed_in: false, email: "", plan_type: "" }));

    render(<CodexUsageCard />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^sign in$/i })).toBeInTheDocument(),
    );
    expect(screen.getByText(/not signed in/i)).toBeInTheDocument();
  });

  it("runs codex login and shows the new account", async () => {
    usageSpy.mockResolvedValue(mkUsage());
    statsSpy.mockResolvedValue(null);
    authSpy.mockResolvedValue(mkAuth({ signed_in: false, email: "" }));
    let finish: (a: CodexAuthStatusDto) => void = () => {};
    loginSpy.mockReturnValue(new Promise((r) => (finish = r)));

    render(<CodexUsageCard />);
    const btn = await screen.findByRole("button", { name: /^sign in$/i });
    fireEvent.click(btn);

    expect(loginSpy).toHaveBeenCalledTimes(1);
    expect(await screen.findByText(/waiting for browser/i)).toBeInTheDocument();

    authSpy.mockResolvedValue(mkAuth({ email: "new@example.com" }));
    finish(mkAuth({ email: "new@example.com" }));
    await waitFor(() =>
      expect(screen.getByText("new@example.com")).toBeInTheDocument(),
    );
  });

  it("surfaces a login failure inline", async () => {
    usageSpy.mockResolvedValue(mkUsage());
    statsSpy.mockResolvedValue(null);
    loginSpy.mockRejectedValue("codex login timed out after 600s");

    render(<CodexUsageCard />);
    fireEvent.click(await screen.findByRole("button", { name: /re-sign in/i }));

    expect(await screen.findByText(/timed out after 600s/)).toBeInTheDocument();
  });

  it("hides the card when Codex is neither installed nor has data", async () => {
    usageSpy.mockResolvedValue(null);
    statsSpy.mockResolvedValue(null);
    authSpy.mockResolvedValue(mkAuth({ installed: false, signed_in: false }));

    const { container } = render(<CodexUsageCard />);
    await waitFor(() => expect(authSpy).toHaveBeenCalled());
    await waitFor(() => expect(container.firstChild).toBeNull());
  });
});