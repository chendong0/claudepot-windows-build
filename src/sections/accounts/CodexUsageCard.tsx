import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api";
import { Glyph } from "../../components/primitives/Glyph";
import { NF } from "../../icons";
import { Button } from "../../components/primitives/Button";
import type {
  CodexAuthStatusDto,
  CodexStatsDto,
  CodexUsageDto,
} from "../../types";
import { formatResetTime } from "./format";

/**
 * Codex CLI rate-limit card, rendered on the Accounts page beneath
 * the Anthropic account grid. Mirrors the account card's "限额窗口"
 * block: a 5-hour window row, a weekly window row, and an overage
 * line — but the data comes from the `codex` CLI's own rollout logs
 * (`~/.codex/sessions/**`), not from Anthropic.
 *
 * Read-only. Reuses the `accounts` i18n namespace's window labels so
 * the two cards speak the same language without new catalog keys.
 */
export function CodexUsageCard() {
  const { t } = useTranslation("accounts");
  const [data, setData] = useState<CodexUsageDto | null | undefined>(undefined);
  const [failed, setFailed] = useState(false);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    void api
      .codexUsageRead()
      .then((d) => {
        if (!cancelled) {
          setData(d);
          setFailed(false);
        }
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [tick]);

  useEffect(() => {
    const id = window.setInterval(() => setTick((n) => n + 1), 60_000);
    return () => window.clearInterval(id);
  }, []);

  // Lifetime rollup — refreshed on the same 60s tick as the windows.
  const [stats, setStats] = useState<CodexStatsDto | null>(null);
  useEffect(() => {
    let cancelled = false;
    void api
      .codexStatsRead()
      .then((s) => {
        if (!cancelled) setStats(s);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [tick]);

  // Sign-in state from ~/.codex/auth.json — same 60s tick, plus an
  // immediate re-read after a login/logout finishes.
  const [auth, setAuth] = useState<CodexAuthStatusDto | null>(null);
  const [busy, setBusy] = useState<"login" | "logout" | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void api
      .codexAuthStatus()
      .then((a) => {
        if (!cancelled) setAuth(a);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [tick]);

  const runAuth = (verb: "login" | "logout") => {
    setBusy(verb);
    setAuthError(null);
    const call = verb === "login" ? api.codexLogin() : api.codexLogout();
    void call
      .then((a) => setAuth(a))
      .catch((e: unknown) => setAuthError(String(e)))
      .finally(() => {
        setBusy(null);
        setTick((n) => n + 1);
      });
  };

  // Hide the card only when Codex isn't installed AND has no data —
  // an installed-but-signed-out Codex still needs the sign-in button.
  // A read failure keeps `data` at `undefined`, so this only fires on
  // a successful "no snapshot" result.
  if (data === null && !auth?.installed && !auth?.signed_in) return null;

  return (
    <article
      style={{
        border: "var(--bw-hair) solid var(--line)",
        borderRadius: "var(--r-3)",
        background: "var(--bg)",
        padding: "var(--sp-14) var(--sp-18) var(--sp-12)",
        marginTop: "var(--sp-12)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: "var(--sp-10)",
        }}
      >
        <span
          style={{
            fontWeight: 600,
            fontSize: "var(--fs-sm)",
            color: "var(--fg)",
          }}
        >
          Codex
          {auth?.signed_in && auth.email ? (
            <span
              className="selectable"
              style={{
                marginLeft: "var(--sp-8)",
                fontWeight: 400,
                fontSize: "var(--fs-xs)",
                color: "var(--fg-muted)",
              }}
            >
              {auth.email}
            </span>
          ) : null}
        </span>
        <span
          style={{ display: "inline-flex", alignItems: "center", gap: "var(--sp-8)" }}
        >
          {auth?.installed ? (
            <Button
              variant="ghost"
              size="sm"
              disabled={busy !== null}
              onClick={() => runAuth("login")}
            >
              {busy === "login"
                ? t("codex.signingIn")
                : auth.signed_in
                  ? t("codex.reSignIn")
                  : t("codex.signIn")}
            </Button>
          ) : null}
        {(auth?.plan_type || data?.plan_type) ? (
          <span
            className="mono-cap"
            style={{
              fontSize: "var(--fs-2xs)",
              color: "var(--fg-faint)",
              border: "var(--bw-hair) solid var(--line)",
              borderRadius: "var(--r-1)",
              padding: "var(--sp-2) var(--sp-6)",
              letterSpacing: "0.04em",
              textTransform: "uppercase",
            }}
          >
            {auth?.plan_type || data?.plan_type}
          </span>
        ) : null}
        </span>
      </div>

      {authError ? (
        <StatusLine>{t("codex.loginFailed", { detail: authError })}</StatusLine>
      ) : auth && !auth.installed && !auth.signed_in ? (
        <StatusLine>{t("codex.notInstalled")}</StatusLine>
      ) : auth && !auth.signed_in ? (
        <StatusLine>{t("codex.signedOut")}</StatusLine>
      ) : null}

      {failed ? (
        <StatusLine>{t("usage.fetchFailed", { detail: "codex" })}</StatusLine>
      ) : data === null ? (
        <StatusLine>{t("codex.noSnapshot")}</StatusLine>
      ) : data === undefined ? (
        <StatusLine>{t("usage.refreshing")}</StatusLine>
      ) : (
        <>
          <div
            style={{
              fontSize: "var(--fs-2xs)",
              color: "var(--fg-faint)",
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              marginBottom: "var(--sp-8)",
            }}
          >
            {t("usage.header")}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--sp-8)" }}>
            <Row
              label={t("usage.rows.fiveHour")}
              window={data.primary}
              emph
            />
            <Row label={t("usage.rows.sevenDay")} window={data.secondary} />
          </div>
          <div
            style={{
              marginTop: "var(--sp-10)",
              fontSize: "var(--fs-xs)",
              color: "var(--fg-muted)",
            }}
          >
            {t("usage.extra.label")}
            {": "}
            {data.credits.unlimited
              ? "∞"
              : data.credits.has_credits ||
                  (data.credits.balance && data.credits.balance !== "0")
                ? data.credits.balance
                : t("usage.extra.off")}
          </div>
          {stats && stats.session_count > 0 && (
            <div
              style={{
                marginTop: "var(--sp-8)",
                fontSize: "var(--fs-2xs)",
                color: "var(--fg-faint)",
              }}
            >
              {fmtTok(stats.total_tokens)} tok · {stats.active_days} days ·{" "}
              streak {stats.current_streak_days}d · peak{" "}
              {fmtTok(stats.peak_day_tokens)}/d
              <span style={{ opacity: 0.7 }}>{" (local est.)"}</span>
            </div>
          )}
          {data.captured_at && (
            <div
              style={{
                marginTop: "var(--sp-6)",
                fontSize: "var(--fs-2xs)",
                color: "var(--fg-faint)",
              }}
            >
              {formatAgo(data.captured_at)}
            </div>
          )}
        </>
      )}
    </article>
  );
}

function fmtTok(n: number): string {
  if (n >= 1e8) return `${(n / 1e8).toFixed(2)}亿`;
  if (n >= 1e4) return `${(n / 1e4).toFixed(1)}万`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return String(n);
}

function StatusLine({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ fontSize: "var(--fs-xs)", color: "var(--fg-muted)" }}>
      {children}
    </div>
  );
}

function Row({
  label,
  window: w,
  emph = false,
}: {
  label: string;
  window: CodexUsageDto["primary"];
  emph?: boolean;
}) {
  const { t } = useTranslation("accounts");
  if (!w) return null;
  const pct = Math.round(Math.max(0, Math.min(100, w.used_percent)));
  // Codex only records a snapshot when the CLI itself runs a turn — see
  // `codex_usage.rs`'s module doc. So a window whose `resets_at` has
  // already passed has definitely rolled over in reality (confirmed by
  // e.g. the ChatGPT app's own live usage panel), but this card is still
  // showing the LAST reading from before that rollover. Painting that
  // number in the same red/amber bands as a live reading — 99% used, in
  // red — reads as "you are nearly capped" when the true current value
  // is unknown and could be anywhere from 0%. Mute it and say so instead
  // of asserting a number we know is stale.
  const stale = w.resets_at > 0 && w.resets_at * 1000 <= Date.now();
  const band = stale
    ? "var(--fg-faint)"
    : pct >= 90
      ? "var(--danger)"
      : pct >= 60
        ? "var(--warn, var(--accent))"
        : "var(--accent)";
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "var(--sp-80) 1fr var(--sp-40) var(--sp-96)",
        gap: "var(--sp-10)",
        alignItems: "center",
        fontSize: "var(--fs-xs)",
      }}
    >
      <span
        style={{
          color: emph ? "var(--fg)" : "var(--fg-muted)",
          fontWeight: emph ? 600 : 500,
        }}
      >
        {label}
      </span>
      <SegBar pct={pct} band={band} muted={stale} />
      <span
        title={stale ? t("usage.staleWindow") : undefined}
        style={{
          fontVariantNumeric: "tabular-nums",
          textAlign: "right",
          fontWeight: 600,
          color: stale
            ? "var(--fg-faint)"
            : pct >= 60
              ? band
              : emph
                ? "var(--fg)"
                : "var(--fg-muted)",
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "flex-end",
          gap: "var(--sp-2)",
        }}
      >
        {stale && (
          <Glyph
            g={NF.clock}
            style={{ fontSize: "var(--fs-3xs)" }}
          />
        )}
        {stale ? "~" : ""}
        {pct}%
      </span>
      <span
        title={stale ? t("usage.staleWindow") : undefined}
        style={{
          textAlign: "right",
          color: "var(--fg-faint)",
          fontVariantNumeric: "tabular-nums",
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {w.resets_at > 0
          ? formatResetTime(new Date(w.resets_at * 1000).toISOString())
          : "—"}
      </span>
    </div>
  );
}

function SegBar({
  pct,
  band,
  muted = false,
}: {
  pct: number;
  band: string;
  muted?: boolean;
}) {
  const segs = 20;
  const raw = Math.round((pct / 100) * segs);
  const filled = pct > 0 ? Math.max(1, raw) : 0;
  return (
    <div
      style={{ display: "flex", gap: "var(--sp-2)", height: "var(--sp-8)" }}
      aria-hidden
    >
      {Array.from({ length: segs }).map((_, i) => (
        <div
          key={i}
          style={{
            flex: 1,
            background: i < filled ? band : "var(--bg-active)",
            borderRadius: "var(--sp-px)",
            // A stale reading keeps its filled-segment count (the shape
            // of the last known reading) but drops to the same faint
            // opacity as the unfilled tail, so the bar reads as a ghost
            // of a number rather than a confident live one.
            opacity: i < filled ? (muted ? "var(--opacity-segbar)" : 1) : "var(--opacity-segbar)",
          }}
        />
      ))}
    </div>
  );
}

function formatAgo(iso: string): string {
  const ts = Date.parse(iso);
  if (Number.isNaN(ts)) return iso;
  const mins = Math.round((Date.now() - ts) / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const h = Math.floor(mins / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}
