//! Login throttling keyed by client IP and by username (5 attempts/minute each).

use std::num::NonZeroU32;

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use mbl_core::Error;

const ATTEMPTS_PER_MINUTE: NonZeroU32 = NonZeroU32::MIN.saturating_add(4);
/// Drop idle keys once the maps grow beyond this many entries.
const MAX_TRACKED_KEYS: usize = 10_000;

pub struct LoginLimiter {
    by_ip: DefaultKeyedRateLimiter<String>,
    by_user: DefaultKeyedRateLimiter<String>,
}

impl std::fmt::Debug for LoginLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginLimiter").finish_non_exhaustive()
    }
}

impl Default for LoginLimiter {
    fn default() -> Self {
        let quota = Quota::per_minute(ATTEMPTS_PER_MINUTE);
        Self {
            by_ip: RateLimiter::keyed(quota),
            by_user: RateLimiter::keyed(quota),
        }
    }
}

impl LoginLimiter {
    /// Record an attempt; `RateLimited` if either the IP or the username is over quota.
    pub fn check(&self, ip: &str, username: &str) -> mbl_core::Result<()> {
        if self.by_ip.len() > MAX_TRACKED_KEYS {
            self.by_ip.retain_recent();
        }
        if self.by_user.len() > MAX_TRACKED_KEYS {
            self.by_user.retain_recent();
        }
        let user_key = username.trim().to_lowercase();
        for (limiter, key) in [(&self.by_ip, ip.to_owned()), (&self.by_user, user_key)] {
            if let Err(not_until) = limiter.check_key(&key) {
                let wait = not_until.wait_time_from(DefaultClock::default().now());
                return Err(Error::RateLimited {
                    retry_after_secs: wait.as_secs().max(1),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixth_attempt_is_limited_per_username() {
        let limiter = LoginLimiter::default();
        for i in 0..5 {
            limiter.check(&format!("10.0.0.{i}"), "Marcus").unwrap();
        }
        let err = limiter.check("10.0.0.99", "marcus").unwrap_err();
        assert!(matches!(err, Error::RateLimited { retry_after_secs } if retry_after_secs >= 1));
        assert!(limiter.check("10.0.0.99", "someone-else").is_ok());
    }

    #[test]
    fn limited_per_ip() {
        let limiter = LoginLimiter::default();
        for i in 0..5 {
            limiter.check("1.2.3.4", &format!("user{i}")).unwrap();
        }
        assert!(limiter.check("1.2.3.4", "user9").is_err());
    }
}
