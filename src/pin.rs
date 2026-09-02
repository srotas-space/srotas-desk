//! The counter screen-lock PIN: hashing, verification, and the
//! failed-attempt lockout policy.
//!
//! The PIN used to be stored as plain text (see `migrations/0002` — it was
//! framed as a soft screen lock rather than a security boundary). It's now
//! stored as an Argon2id PHC string instead: the shop database is a plain
//! file that gets copied onto pendrives and synced folders by the backup
//! feature, so anything readable in it travels a lot further than the
//! counter it was typed at. Existing plaintext PINs are re-hashed in place
//! on the next launch — see `repo::shop::upgrade_legacy_pin`.
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{DateTime, Duration, Utc};

/// How many consecutive wrong PINs are tolerated before the screen locks
/// for a while. Generous enough that ordinary fat-fingering at a busy
/// counter never trips it.
pub const MAX_ATTEMPTS: i64 = 5;

/// How long the first lockout lasts. Each further wrong PIN doubles it,
/// up to `MAX_LOCKOUT`.
const BASE_LOCKOUT_SECS: i64 = 30;

/// Ceiling on the doubling above — long enough to make guessing a 4-digit
/// PIN hopeless, short enough that a shopkeeper who genuinely forgot isn't
/// stranded for the rest of the day (and there's always "Forgot PIN?").
const MAX_LOCKOUT_SECS: i64 = 15 * 60;

/// Hashes a PIN for storage. Argon2's default params are deliberately
/// slow (tens of milliseconds), so call this off the UI thread.
pub fn hash(pin: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("could not secure that PIN: {e}"))
}

/// Checks a typed PIN against a stored hash. A malformed or corrupt stored
/// hash is treated as "doesn't match" rather than an error — there's no
/// useful recovery at the login screen beyond the reset flow, and failing
/// closed is the safe direction.
pub fn verify(pin: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default().verify_password(pin.as_bytes(), &parsed).is_ok()
}

/// Validates a PIN the user is trying to *set*. `Ok(None)` means "no PIN"
/// (the field was left blank), which is a legitimate choice — it removes
/// the screen lock.
pub fn validate_new(pin: &str, confirm: &str) -> Result<Option<String>, String> {
    let pin = pin.trim();
    if pin.is_empty() {
        return Ok(None);
    }
    if !(4..=6).contains(&pin.chars().count()) || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err("PIN must be 4 to 6 digits".into());
    }
    if pin != confirm.trim() {
        return Err("PIN and confirmation don't match".into());
    }
    Ok(Some(pin.to_string()))
}

/// How long to lock the screen after `attempts` consecutive failures.
/// `None` for anything below the threshold — those are just retries.
pub fn lockout_for(attempts: i64) -> Option<Duration> {
    if attempts < MAX_ATTEMPTS {
        return None;
    }
    let over = (attempts - MAX_ATTEMPTS).min(16) as u32;
    let secs = BASE_LOCKOUT_SECS.saturating_mul(2i64.saturating_pow(over)).min(MAX_LOCKOUT_SECS);
    Some(Duration::seconds(secs))
}

/// Seconds still to wait before `locked_until` expires, or `None` once it
/// has (or was never set). Everything that gates on the lockout goes
/// through this so a stale timestamp in the database can't lock anyone out
/// permanently.
pub fn remaining_lock_secs(locked_until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<i64> {
    let until = locked_until?;
    let secs = (until - now).num_seconds();
    if secs > 0 { Some(secs) } else { None }
}

/// Renders a remaining lockout as something a shopkeeper can read at a
/// glance ("1:30" rather than "90 seconds").
pub fn format_remaining(secs: i64) -> String {
    if secs >= 60 {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_verifies_against_its_own_pin_and_nothing_else() {
        let stored = hash("1234").unwrap();
        assert!(verify("1234", &stored));
        assert!(!verify("1235", &stored));
        assert!(!verify("", &stored));
    }

    #[test]
    fn the_same_pin_hashes_differently_every_time() {
        // Distinct salts — two shops with the same PIN must not share a hash.
        assert_ne!(hash("1234").unwrap(), hash("1234").unwrap());
    }

    #[test]
    fn a_corrupt_stored_hash_never_verifies() {
        assert!(!verify("1234", "not-a-phc-string"));
        assert!(!verify("1234", ""));
    }

    #[test]
    fn validates_a_new_pin() {
        assert_eq!(validate_new("1234", "1234").unwrap(), Some("1234".to_string()));
        assert_eq!(validate_new("  ", "").unwrap(), None);
        assert!(validate_new("123", "123").is_err());
        assert!(validate_new("1234567", "1234567").is_err());
        assert!(validate_new("12ab", "12ab").is_err());
        assert!(validate_new("1234", "4321").is_err());
    }

    #[test]
    fn lockout_starts_only_at_the_threshold_and_then_doubles() {
        assert!(lockout_for(MAX_ATTEMPTS - 1).is_none());
        assert_eq!(lockout_for(MAX_ATTEMPTS).unwrap().num_seconds(), BASE_LOCKOUT_SECS);
        assert_eq!(lockout_for(MAX_ATTEMPTS + 1).unwrap().num_seconds(), BASE_LOCKOUT_SECS * 2);
        assert_eq!(lockout_for(MAX_ATTEMPTS + 2).unwrap().num_seconds(), BASE_LOCKOUT_SECS * 4);
    }

    #[test]
    fn lockout_is_capped() {
        assert_eq!(lockout_for(MAX_ATTEMPTS + 500).unwrap().num_seconds(), MAX_LOCKOUT_SECS);
    }

    #[test]
    fn an_expired_lock_reports_no_remaining_time() {
        let now = Utc::now();
        assert_eq!(remaining_lock_secs(None, now), None);
        assert_eq!(remaining_lock_secs(Some(now - Duration::seconds(5)), now), None);
        assert_eq!(remaining_lock_secs(Some(now + Duration::seconds(30)), now), Some(30));
    }

    #[test]
    fn formats_remaining_time_for_humans() {
        assert_eq!(format_remaining(45), "45s");
        assert_eq!(format_remaining(90), "1:30");
        assert_eq!(format_remaining(600), "10:00");
    }
}
