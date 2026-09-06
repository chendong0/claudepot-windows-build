import { useEffect, useState } from "react";
import { api } from "../../api";
import type { CodexStatsDto } from "../../types";

/**
 * Codex usage + rough cost, on the Activities → Cost tab. Aggregated
 * from every local `~/.codex/sessions/**` rollout log.
 *
 * Two honesty notes are baked into the footer, not optional:
 *  - **Local only** — misses web / mobile Codex use, so the totals sit
 *    below the ChatGPT app's profile figures.
 *  - **The dollar figure is a crude estimate** — Codex Plus is a
 *    subscription (no per-token billing), and the models it uses have
 *    no price table we trust, so every model is scored at one flat
 *    GPT-4-class rate.
 */
export function CodexCostPanel() {
  const [stats, setStats] = useState<CodexStatsDto | null | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;
    void api
      .codexStatsRead()
      .then((s) => {
        if (!cancelled) setStats(s);
      })
      .catch(() => {
        if (!cancelled) setStats(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (stats === undefined || stats === null || stats.session_count === 0) {
    return null;
  }

  const all = stats.windows.find((w) => w.window === "all");

  return (
    <section
      style={{
        border: "var(--bw-hair) solid var(--line)",
        borderRadius: "var(--r-3)",
        background: "var(--bg)",
        padding: "var(--sp-14) var(--sp-16)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--sp-10)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--sp-8)",
        }}
      >
        <span style={{ fontWeight: 600, fontSize: "var(--fs-sm)", color: "var(--fg)" }}>
          Codex
        </span>
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
          local · ≈ estimate
        </span>
      </div>

      {/* lifetime line */}
      <div style={{ fontSize: "var(--fs-xs)", color: "var(--fg-muted)" }}>
        <Stat label="lifetime" value={fmtTokens(stats.total_tokens)} />
        <Sep />
        <Stat label="sessions" value={String(stats.session_count)} />
        <Sep />
        <Stat label="active days" value={String(stats.active_days)} />
        <Sep />
        <Stat
          label="streak"
          value={`${stats.current_streak_days}d (max ${stats.longest_streak_days}d)`}
        />
        <Sep />
        <Stat label="peak/day" value={fmtTokens(stats.peak_day_tokens)} />
        {all && (
          <>
            <Sep />
            <Stat label="≈ API cost" value={fmtUsd(all.approx_usd)} />
          </>
        )}
      </div>

      {/* per-model table */}
      <div style={{ overflowX: "auto" }}>
        <table
          style={{
            borderCollapse: "collapse",
            width: "100%",
            fontSize: "var(--fs-xs)",
            fontVariantNumeric: "tabular-nums",
          }}
        >
          <thead>
            <tr style={{ color: "var(--fg-faint)", textAlign: "right" }}>
              <th style={{ textAlign: "left", padding: "var(--sp-4) var(--sp-8)" }}>model</th>
              <th style={cellR}>sessions</th>
              <th style={cellR}>input</th>
              <th style={cellR}>cached</th>
              <th style={cellR}>output</th>
              <th style={cellR}>reasoning</th>
              <th style={cellR}>total</th>
              <th style={cellR}>≈ $</th>
            </tr>
          </thead>
          <tbody>
            {stats.by_model.map((m) => (
              <tr key={m.model} style={{ borderTop: "var(--bw-hair) solid var(--line)" }}>
                <td style={{ textAlign: "left", padding: "var(--sp-4) var(--sp-8)", color: "var(--fg)" }}>
                  {m.model}
                </td>
                <td style={cellR}>{m.sessions}</td>
                <td style={cellR}>{fmtTokens(m.tokens.input)}</td>
                <td style={cellR}>{fmtTokens(m.tokens.cached_input)}</td>
                <td style={cellR}>{fmtTokens(m.tokens.output)}</td>
                <td style={cellR}>{fmtTokens(m.tokens.reasoning_output)}</td>
                <td style={{ ...cellR, color: "var(--fg)", fontWeight: 600 }}>
                  {fmtTokens(m.tokens.total)}
                </td>
                <td style={cellR}>{fmtUsd(m.approx_usd)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p style={{ fontSize: "var(--fs-2xs)", color: "var(--fg-faint)", margin: 0, lineHeight: 1.5 }}>
        Rough API-equivalent estimate: every model priced at one flat GPT-4-class
        rate ($2.50 / $0.31 / $10.00 per 1M input / cached / output). Codex Plus
        is a subscription — you are not billed per token. Counts only sessions
        that wrote a rollout log on this machine, so totals run below the ChatGPT
        profile page.
      </p>
    </section>
  );
}

const cellR: React.CSSProperties = {
  textAlign: "right",
  padding: "var(--sp-4) var(--sp-8)",
};

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <span>
      <span style={{ color: "var(--fg-faint)" }}>{label} </span>
      <span style={{ color: "var(--fg)", fontWeight: 600 }}>{value}</span>
    </span>
  );
}

function Sep() {
  return <span style={{ color: "var(--fg-ghost)", margin: "0 var(--sp-8)" }}>·</span>;
}

function fmtTokens(n: number): string {
  if (n >= 1e8) return `${(n / 1e8).toFixed(2)}亿`;
  if (n >= 1e4) return `${(n / 1e4).toFixed(1)}万`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return String(n);
}

function fmtUsd(n: number): string {
  if (n >= 100) return `$${n.toFixed(0)}`;
  if (n >= 1) return `$${n.toFixed(2)}`;
  return `$${n.toFixed(3)}`;
}
