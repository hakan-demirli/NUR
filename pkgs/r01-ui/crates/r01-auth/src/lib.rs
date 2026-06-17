use std::fmt;
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum User {
    Admin,
    Guest,
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Admin => "admin",
            Self::Guest => "guest",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("argon2 hashing failed: {0}")]
    Hash(String),
    #[error("stored hash is not parseable: {0}")]
    BadStoredHash(String),
}

#[derive(Clone, Default)]
pub struct Argon2Hasher;

impl fmt::Debug for Argon2Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Argon2Hasher")
    }
}

impl Argon2Hasher {
    pub fn hash(&self, pin: &str) -> Result<String, Error> {
        let salt = SaltString::generate(&mut OsRng);
        let argon = Argon2::default();
        argon
            .hash_password(pin.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| Error::Hash(e.to_string()))
    }

    pub fn verify(&self, pin: &str, stored: &str) -> Result<bool, Error> {
        let parsed = PasswordHash::new(stored).map_err(|e| Error::BadStoredHash(e.to_string()))?;
        Ok(Argon2::default()
            .verify_password(pin.as_bytes(), &parsed)
            .is_ok())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BackoffPolicy {
    pub steps: [(u32, Duration); 3],
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            steps: [
                (3, Duration::from_secs(1)),
                (5, Duration::from_secs(5)),
                (10, Duration::from_secs(30)),
            ],
        }
    }
}

impl BackoffPolicy {
    pub fn lockout_for(&self, attempts: u32) -> Option<Duration> {
        self.steps
            .iter()
            .rev()
            .find_map(|(thresh, dur)| (attempts >= *thresh).then_some(*dur))
    }
}

#[derive(Debug, Clone, Copy)]
struct UserState {
    failed_attempts: u32,
    locked_until: Option<Instant>,
}

impl UserState {
    const fn fresh() -> Self {
        Self {
            failed_attempts: 0,
            locked_until: None,
        }
    }
}

#[derive(Debug)]
pub struct Authenticator {
    policy: BackoffPolicy,
    admin: UserState,
    guest: UserState,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Wrong {
        attempts: u32,
        lockout: Option<Duration>,
    },
    LockedOut {
        remaining: Duration,
    },
}

impl Authenticator {
    pub const fn new(policy: BackoffPolicy) -> Self {
        Self {
            policy,
            admin: UserState::fresh(),
            guest: UserState::fresh(),
        }
    }

    fn state_mut(&mut self, user: User) -> &mut UserState {
        match user {
            User::Admin => &mut self.admin,
            User::Guest => &mut self.guest,
        }
    }

    const fn state(&self, user: User) -> UserState {
        match user {
            User::Admin => self.admin,
            User::Guest => self.guest,
        }
    }

    fn evict_expired_lockout(state: &mut UserState, now: Instant) {
        if let Some(until) = state.locked_until {
            if now >= until {
                state.locked_until = None;
            }
        }
    }

    pub const fn locked_until(&self, user: User) -> Option<Instant> {
        self.state(user).locked_until
    }

    pub fn try_pin<H>(
        &mut self,
        user: User,
        pin: &str,
        stored_hash: &str,
        hasher: &H,
        now: Instant,
    ) -> Verdict
    where
        H: PinHasher,
    {
        {
            let state = self.state_mut(user);
            Self::evict_expired_lockout(state, now);
            if let Some(until) = state.locked_until {
                return Verdict::LockedOut {
                    remaining: until.saturating_duration_since(now),
                };
            }
        }

        let verified = hasher.verify(pin, stored_hash);

        if matches!(verified, Ok(true)) {
            *self.state_mut(user) = UserState::fresh();
            Verdict::Ok
        } else {
            let attempts = {
                let s = self.state_mut(user);
                s.failed_attempts = s.failed_attempts.saturating_add(1);
                s.failed_attempts
            };
            let lockout = self.policy.lockout_for(attempts);
            if let Some(dur) = lockout {
                self.state_mut(user).locked_until = Some(now + dur);
            }
            Verdict::Wrong { attempts, lockout }
        }
    }
}

pub trait PinHasher {
    fn verify(&self, pin: &str, stored: &str) -> Result<bool, Error>;
}

impl PinHasher for Argon2Hasher {
    fn verify(&self, pin: &str, stored: &str) -> Result<bool, Error> {
        Self::verify(self, pin, stored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct PlainHasher;
    impl PinHasher for PlainHasher {
        fn verify(&self, pin: &str, stored: &str) -> Result<bool, Error> {
            Ok(pin == stored)
        }
    }

    fn t(seconds: u64) -> Instant {
        static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *BASE.get_or_init(Instant::now) + Duration::from_secs(seconds)
    }

    #[test]
    fn correct_pin_passes() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        let r = a.try_pin(User::Admin, "1234", "1234", &PlainHasher, t(0));
        assert_eq!(r, Verdict::Ok);
    }

    #[test]
    fn wrong_pin_increments_counter() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        let r1 = a.try_pin(User::Admin, "0000", "1234", &PlainHasher, t(0));
        let r2 = a.try_pin(User::Admin, "0001", "1234", &PlainHasher, t(0));
        assert_eq!(
            r1,
            Verdict::Wrong {
                attempts: 1,
                lockout: None
            }
        );
        assert_eq!(
            r2,
            Verdict::Wrong {
                attempts: 2,
                lockout: None
            }
        );
    }

    #[test]
    fn third_wrong_triggers_first_lockout() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        for _ in 0..2 {
            let _ = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        }
        let r = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        assert_eq!(
            r,
            Verdict::Wrong {
                attempts: 3,
                lockout: Some(Duration::from_secs(1))
            }
        );
    }

    #[test]
    fn locked_out_during_window() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        for _ in 0..3 {
            let _ = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        }
        let r = a.try_pin(User::Admin, "y", "y", &PlainHasher, t(0));
        assert!(matches!(r, Verdict::LockedOut { .. }));
    }

    #[test]
    fn lockout_expires_after_window() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        for _ in 0..3 {
            let _ = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        }
        let r = a.try_pin(User::Admin, "y", "y", &PlainHasher, t(2));
        assert_eq!(r, Verdict::Ok);
    }

    #[test]
    fn success_resets_counter() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        for _ in 0..2 {
            let _ = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        }
        let _ = a.try_pin(User::Admin, "y", "y", &PlainHasher, t(0));
        let r = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        assert_eq!(
            r,
            Verdict::Wrong {
                attempts: 1,
                lockout: None
            }
        );
    }

    #[test]
    fn admin_and_guest_are_independent() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        for _ in 0..3 {
            let _ = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        }
        assert!(matches!(
            a.try_pin(User::Admin, "y", "y", &PlainHasher, t(0)),
            Verdict::LockedOut { .. }
        ));
        assert_eq!(
            a.try_pin(User::Guest, "y", "y", &PlainHasher, t(0)),
            Verdict::Ok
        );
    }

    #[test]
    fn backoff_steps_advance() {
        let mut a = Authenticator::new(BackoffPolicy::default());
        for _ in 0..3 {
            let _ = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(0));
        }
        let r = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(2));
        assert_eq!(
            r,
            Verdict::Wrong {
                attempts: 4,
                lockout: Some(Duration::from_secs(1))
            }
        );
        let r = a.try_pin(User::Admin, "x", "y", &PlainHasher, t(10));
        assert_eq!(
            r,
            Verdict::Wrong {
                attempts: 5,
                lockout: Some(Duration::from_secs(5))
            }
        );
    }

    #[test]
    fn argon2_roundtrip() {
        let h = Argon2Hasher;
        let stored = h.hash("4242").unwrap();
        assert!(h.verify("4242", &stored).unwrap());
        assert!(!h.verify("4243", &stored).unwrap());
    }

    #[test]
    fn argon2_rejects_malformed_stored_hash() {
        let h = Argon2Hasher;
        let err = h.verify("1234", "not-a-phc-string").unwrap_err();
        assert!(matches!(err, Error::BadStoredHash(_)));
    }
}
