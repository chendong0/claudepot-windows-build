import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api";
import type { CodexUsageDto } from "../../types";
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

  // Hide the card entirely when Codex isn't installed / has no data —
  // an empty "no data" card on the Accounts page is just noise. A
  // read failure keeps `data` at `undefined`, so this only fires on a
  // successful "no snapshot" result.
  if (data === null) return null;

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
        </span>
        {data?.plan_type ? (
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
            {data.plan_type}
          </span>
        ) : null}
      </div>

      {failed ? (
        <StatusLine>{t("usage.fetchFailed", { detail: "codex" })}</StatusLine>
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
  if (!w) return null;
  const pct = Math.round(Math.max(0, Math.min(100, w.used_percent)));
  const band =
    pct >= 90
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
      <SegBar pct={pct} band={band} />
      <span
        style={{
          fontVariantNumeric: "tabular-nums",
          textAlign: "right",
          fontWeight: 600,
          color: pct >= 60 ? band : emph ? "var(--fg)" : "var(--fg-muted)",
        }}
      >
        {pct}%
      </span>
      <span
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

function SegBar({ pct, band }: { pct: number; band: string }) {
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
            opacity: i < filled ? 1 : "var(--opacity-segbar)",
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
