//! Codex CLI sign-in state + login/logout — the Accounts card's
//! "Sign in to Codex" control.
//!
//! Status is read from `$CODEX_HOME/auth.json` (default
//! `~/.codex/auth.json`), the file `codex login` writes. Only the
//! **claims** of the OpenID `id_token` are decoded — email and plan —
//! and no token ever leaves this module: `CodexAuthStatus` carries no
//! secret, so it is safe to cross IPC.
//!
//! Login and logout are delegated to the `codex` binary itself
//! (`codex login` runs its own browser OAuth flow on localhost and
//! writes `auth.json`; `codex logout` deletes it). Reimplementing
//! OpenAI's OAuth exchange here would fork a protocol Codex owns.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::proc_utils::NoWindowExt;

/// How long `codex login` may wait for the user to finish the browser
/// flow before we kill it. Codex's own local callback server has no
/// timeout, so without this a closed browser tab would pin the
/// "waiting for browser" state forever.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, thiserror::Error)]
pub enum CodexAuthError {
    #[error("codex CLI not found on PATH")]
    NotInstalled,
    #[error("failed to start codex: {0}")]
    Spawn(std::io::Error),
    #[error("codex {verb} timed out after {secs}s")]
    Timeout { verb: &'static str, secs: u64 },
    #[error("codex {verb} exited with {code}: {detail}")]
    Failed {
        verb: &'static str,
        code: String,
        detail: String,
    },
}

/// Secret-free view of Codex's sign-in state.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct CodexAuthStatus {
    /// A `codex` binary was found on PATH (login is possible).
    pub installed: bool,
    /// `auth.json` exists and holds usable credentials.
    pub signed_in: bool,
    /// `"chatgpt"` or `"apikey"`; empty when signed out.
    pub auth_mode: String,
    /// Account email from the id_token claims; empty if unknown.
    pub email: String,
    /// ChatGPT plan (`"plus"`, `"pro"`, …); empty if unknown.
    pub plan_type: String,
    /// ISO-8601 time Codex last refreshed its tokens, verbatim.
    pub last_refresh: String,
}

#[derive(Deserialize)]
struct AuthFile {
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default, rename = "OPENAI_API_KEY")]
    api_key: Option<String>,
    #[serde(default)]
    tokens: Option<Tokens>,
    #[serde(default)]
    last_refresh: Option<String>,
}

#[derive(Deserialize)]
struct Tokens {
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Deserialize, Default)]
struct IdClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(default, rename = "https://api.openai.com/auth")]
    openai: Option<OpenAiClaims>,
}

#[derive(Deserialize, Default)]
struct OpenAiClaims {
    #[serde(default)]
    chatgpt_plan_type: Option<String>,
}

/// Decode a JWT's payload segment without verifying it. Display-only:
/// the claims label the card, they never authorize anything.
fn jwt_claims(jwt: &str) -> Option<IdClaims> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Parse `auth.json` contents into a status. `installed` is left
/// false; the caller fills it from [`find_codex_binary`].
pub fn parse_auth(raw: &str) -> CodexAuthStatus {
    let Ok(file) = serde_json::from_str::<AuthFile>(raw) else {
        return CodexAuthStatus::default();
    };
    let has_key = file.api_key.as_deref().is_some_and(|k| !k.is_empty());
    let tokens = file.tokens.as_ref();
    let has_tokens = tokens
        .and_then(|t| t.refresh_token.as_deref())
        .is_some_and(|t| !t.is_empty());
    let claims = tokens
        .and_then(|t| t.id_token.as_deref())
        .and_then(jwt_claims)
        .unwrap_or_default();
    let auth_mode = file.auth_mode.unwrap_or_else(|| {
        if has_tokens {
            "chatgpt".into()
        } else if has_key {
            "apikey".into()
        } else {
            String::new()
        }
    });
    CodexAuthStatus {
        installed: false,
        signed_in: has_tokens || has_key,
        auth_mode,
        email: claims.email.unwrap_or_default(),
        plan_type: claims
            .openai
            .and_then(|o| o.chatgpt_plan_type)
            .unwrap_or_default(),
        last_refresh: file.last_refresh.unwrap_or_default(),
    }
}

/// Read `<codex_home>/auth.json`. A missing or unreadable file is
/// "signed out", never an error — there is nothing the user can do
/// with a read error that signing in would not also fix.
pub fn read_status(codex_home: &Path) -> CodexAuthStatus {
    let mut status = std::fs::read_to_string(codex_home.join("auth.json"))
        .map(|raw| parse_auth(&raw))
        .unwrap_or_default();
    status.installed = find_codex_binary().is_some();
    status
}

/// Locate the `codex` launcher on PATH. On Windows npm installs a
/// `codex.cmd` shim (no `.exe`), so PATHEXT-style candidates are tried
/// in order; Rust's `Command` runs `.cmd` files with correct escaping.
pub fn find_codex_binary() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &["codex.exe", "codex.cmd", "codex.bat"]
    } else {
        &["codex"]
    };
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

async fn run_codex(
    verb: &'static str,
    args: &[&str],
    timeout: Duration,
) -> Result<(), CodexAuthError> {
    let bin = find_codex_binary().ok_or(CodexAuthError::NotInstalled)?;
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .no_window();
    let child = cmd.spawn().map_err(CodexAuthError::Spawn)?;
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| CodexAuthError::Timeout {
            verb,
            secs: timeout.as_secs(),
        })?
        .map_err(CodexAuthError::Spawn)?;
    if out.status.success() {
        return Ok(());
    }
    // Codex's stderr is user-facing prose (no tokens); keep the tail.
    let text = String::from_utf8_lossy(&out.stderr);
    let detail: String = text.trim().lines().last().unwrap_or("").chars().take(300).collect();
    Err(CodexAuthError::Failed {
        verb,
        code: out
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |c| c.to_string()),
        detail,
    })
}

/// Run `codex login`: Codex opens the browser, waits for the OAuth
/// callback on localhost, and writes `auth.json`. Resolves when that
/// process exits (or after [`LOGIN_TIMEOUT`]).
pub async fn login() -> Result<(), CodexAuthError> {
    run_codex("login", &["login"], LOGIN_TIMEOUT).await
}

/// Run `codex logout`, which removes `auth.json`.
pub async fn logout() -> Result<(), CodexAuthError> {
    run_codex("logout", &["logout"], Duration::from_secs(60)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(claims: &str) -> String {
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!("{}.{}.sig", enc.encode("{}"), enc.encode(claims))
    }

    #[test]
    fn chatgpt_auth_reports_email_and_plan() {
        let id = jwt(
            r#"{"email":"a@b.c","https://api.openai.com/auth":{"chatgpt_plan_type":"plus"}}"#,
        );
        let raw = format!(
            r#"{{"auth_mode":"chatgpt","OPENAI_API_KEY":null,
               "tokens":{{"id_token":"{id}","access_token":"x","refresh_token":"r","account_id":"1"}},
               "last_refresh":"2026-10-11T02:54:42Z"}}"#
        );
        let s = parse_auth(&raw);
        assert!(s.signed_in);
        assert_eq!(s.auth_mode, "chatgpt");
        assert_eq!(s.email, "a@b.c");
        assert_eq!(s.plan_type, "plus");
        assert_eq!(s.last_refresh, "2026-10-11T02:54:42Z");
    }

    #[test]
    fn api_key_auth_is_signed_in_without_email() {
        let s = parse_auth(r#"{"OPENAI_API_KEY":"sk-test"}"#);
        assert!(s.signed_in);
        assert_eq!(s.auth_mode, "apikey");
        assert_eq!(s.email, "");
    }

    #[test]
    fn empty_or_garbage_file_is_signed_out() {
        assert!(!parse_auth("{}").signed_in);
        assert!(!parse_auth("not json").signed_in);
        assert!(!parse_auth(r#"{"tokens":{"refresh_token":""}}"#).signed_in);
    }

    #[test]
    fn malformed_id_token_still_signed_in() {
        let s = parse_auth(r#"{"tokens":{"id_token":"nope","refresh_token":"r"}}"#);
        assert!(s.signed_in);
        assert_eq!(s.email, "");
    }

    #[test]
    fn missing_auth_file_reads_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!read_status(dir.path()).signed_in);
    }
}
