//! MCP caller identity (`Caller`), scope enforcement, and per-token rate limiting.

use std::num::NonZeroU32;

use axum::http::{HeaderMap, header};
use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use mbl_auth::tokens::{Scope, ScopeSet};
use mbl_core::{Actor, Error, Result};
use mbl_service::Services;

use crate::transport::{NetworkClass, bearer_token};

pub const SESSION_COOKIE: &str = "mbl_session";

const DEFAULT_CALLS_PER_MINUTE: NonZeroU32 = NonZeroU32::MIN.saturating_add(119); // 120/min
const EXPENSIVE_CALLS_PER_MINUTE: NonZeroU32 = NonZeroU32::MIN.saturating_add(1); // 2/min
const MAX_TRACKED_KEYS: usize = 10_000;

/// Resolved MCP caller identity (Appendix A.10).
#[derive(Debug, Clone)]
pub struct Caller {
    pub actor: Actor,
    pub scopes: ScopeSet,
    pub client: String,
    pub token_id: Option<i64>,
}

impl Caller {
    /// Check whether this caller's scopes allow `scope`.
    pub fn allows(&self, scope: Scope) -> bool {
        self.scopes.allows(scope)
    }

    /// Require `scope` for a tool execution, returning `Error::Forbidden` if missing.
    pub fn require(&self, scope: Scope) -> Result<()> {
        if self.allows(scope) { Ok(()) } else { Err(Error::Forbidden) }
    }

    /// Key used for per-token (or per-user-session) rate limiting.
    pub fn rate_limit_key(&self) -> i64 {
        self.token_id.unwrap_or_else(|| -self.actor.user_id.0)
    }
}

/// Resolve an incoming HTTP request to a [`Caller`] via Bearer API token, Bearer session token, or session cookie.
pub async fn resolve_caller(
    services: &Services,
    headers: &HeaderMap,
    network: NetworkClass,
) -> Result<Caller> {
    let client_header = headers
        .get("x-mcp-client")
        .or_else(|| headers.get(header::USER_AGENT))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().chars().take(100).collect::<String>())
        .filter(|s| !s.is_empty());

    let network_str = match network {
        NetworkClass::Lan => "lan",
        NetworkClass::Tailnet => "tailnet",
        NetworkClass::Wan => "wan",
    };

    if let Some(token) = bearer_token(headers) {
        if let Some(api_tok) = services.auth.authenticate_api_token(&token, network_str).await? {
            return Ok(Caller {
                actor: api_tok.actor,
                scopes: api_tok.scopes,
                client: client_header.unwrap_or(api_tok.token_name),
                token_id: Some(api_tok.token_id),
            });
        }
        let sess = services.auth.authenticate(&token).await?;
        let scopes = ScopeSet::full_for_user(sess.actor.is_admin);
        return Ok(Caller {
            actor: sess.actor,
            scopes,
            client: client_header.unwrap_or_else(|| "session".to_owned()),
            token_id: None,
        });
    }

    if let Some(cookie_tok) = extract_cookie(headers, SESSION_COOKIE) {
        let sess = services.auth.authenticate(&cookie_tok).await?;
        let scopes = ScopeSet::full_for_user(sess.actor.is_admin);
        return Ok(Caller {
            actor: sess.actor,
            scopes,
            client: client_header.unwrap_or_else(|| "web-session".to_owned()),
            token_id: None,
        });
    }

    Err(Error::Unauthorized)
}

fn extract_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
        .filter(|v| !v.is_empty())
}

/// Per-token rate limiter: 120 calls/min overall, and 2 calls/min for expensive tools (`scan_library`, `run_hw_selftest`).
pub struct McpRateLimiter {
    general: DefaultKeyedRateLimiter<i64>,
    expensive: DefaultKeyedRateLimiter<(i64, String)>,
}

impl Default for McpRateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_CALLS_PER_MINUTE, EXPENSIVE_CALLS_PER_MINUTE)
    }
}

impl McpRateLimiter {
    pub fn new(general_per_min: NonZeroU32, expensive_per_min: NonZeroU32) -> Self {
        Self {
            general: RateLimiter::keyed(Quota::per_minute(general_per_min)),
            expensive: RateLimiter::keyed(Quota::per_minute(expensive_per_min)),
        }
    }

    pub fn check(&self, caller_key: i64, tool_name: &str) -> Result<()> {
        if self.general.len() > MAX_TRACKED_KEYS {
            self.general.retain_recent();
        }
        if self.expensive.len() > MAX_TRACKED_KEYS {
            self.expensive.retain_recent();
        }

        if is_expensive_tool(tool_name)
            && let Err(not_until) = self.expensive.check_key(&(caller_key, tool_name.to_owned()))
        {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            return Err(Error::RateLimited { retry_after_secs: wait.as_secs().max(1) });
        }

        if let Err(not_until) = self.general.check_key(&caller_key) {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            return Err(Error::RateLimited { retry_after_secs: wait.as_secs().max(1) });
        }

        Ok(())
    }
}

pub fn is_expensive_tool(tool_name: &str) -> bool {
    matches!(tool_name, "scan_library" | "run_hw_selftest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_enforces_expensive_and_general_quotas() {
        let limiter = McpRateLimiter::default();
        // scan_library is capped at 2 calls/min per token.
        assert!(limiter.check(42, "scan_library").is_ok());
        assert!(limiter.check(42, "scan_library").is_ok());
        assert!(matches!(limiter.check(42, "scan_library"), Err(Error::RateLimited { .. })));
        // Another token is unaffected.
        assert!(limiter.check(99, "scan_library").is_ok());
    }
}
