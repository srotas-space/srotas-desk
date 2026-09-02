//! Verifies Srotas Desk license keys — entirely offline, on purpose. A
//! key is a self-contained, signed blob issued by the SaaS admin panel;
//! this app never calls home to check one. See `repo::license` for where
//! the activated key and this machine's device id are persisted.
//!
//! Wire format must match the signer in the backend's `src/licensing.rs`
//! byte-for-byte — there is no shared crate between the two projects,
//! same as the GST tax-calculation logic duplicated between this app and
//! the backend.
//!
//! ```text
//! version:     u8          (= 1)
//! license_id:  [u8; 16]    (UUID bytes)
//! device_id:   u8 len + UTF-8 bytes (empty = matches any device — a
//!              "universal" key, see verify_with_key)
//! shop_name:   u16 len (big-endian) + UTF-8 bytes
//! issued_at:   i64 (big-endian, unix seconds)
//! expires_at:  i64 (big-endian, unix seconds; 0 = perpetual, no expiry)
//! -- then --
//! signature:   [u8; 64]
//! ```
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Verifier, VerifyingKey};

/// The public half of the keypair the SaaS admin panel signs license keys
/// with. Baked into the binary at compile time — rotating it means
/// shipping a new build, which is the accepted tradeoff of fully offline
/// verification (see the project's licensing design notes).
const PUBLIC_KEY_BYTES: [u8; 32] = [
    136, 115, 100, 241, 52, 113, 96, 169, 214, 26, 243, 17, 2, 180, 121, 72, 21, 123, 64, 21, 60, 137, 234, 29, 61, 67,
    153, 129, 222, 29, 137, 213,
];

#[derive(Debug, Clone)]
pub struct LicensePayload {
    // The app only acts on `expires_at`; these two are carried so the tests
    // can assert this decoder agrees with the backend's signer field for
    // field. Dropping them would quietly narrow that cross-compatibility
    // check to "the signature verified", which is the easy half.
    #[allow(dead_code)]
    pub shop_name: String,
    #[allow(dead_code)]
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("that doesn't look like a valid license key")]
    InvalidFormat,
    #[error("this license key's signature is invalid")]
    BadSignature,
    #[error("this license key was issued for a different computer")]
    DeviceMismatch,
    #[error("this license expired on {0}")]
    Expired(String),
}

fn read_u8_prefixed(bytes: &[u8], pos: &mut usize) -> Result<String, LicenseError> {
    let len = *bytes.get(*pos).ok_or(LicenseError::InvalidFormat)? as usize;
    *pos += 1;
    let end = pos.checked_add(len).ok_or(LicenseError::InvalidFormat)?;
    let slice = bytes.get(*pos..end).ok_or(LicenseError::InvalidFormat)?;
    *pos = end;
    String::from_utf8(slice.to_vec()).map_err(|_| LicenseError::InvalidFormat)
}

fn read_u16_prefixed(bytes: &[u8], pos: &mut usize) -> Result<String, LicenseError> {
    let len_bytes: [u8; 2] = bytes.get(*pos..*pos + 2).ok_or(LicenseError::InvalidFormat)?.try_into().unwrap();
    let len = u16::from_be_bytes(len_bytes) as usize;
    *pos += 2;
    let end = pos.checked_add(len).ok_or(LicenseError::InvalidFormat)?;
    let slice = bytes.get(*pos..end).ok_or(LicenseError::InvalidFormat)?;
    *pos = end;
    String::from_utf8(slice.to_vec()).map_err(|_| LicenseError::InvalidFormat)
}

fn read_i64(bytes: &[u8], pos: &mut usize) -> Result<i64, LicenseError> {
    let chunk: [u8; 8] = bytes.get(*pos..*pos + 8).ok_or(LicenseError::InvalidFormat)?.try_into().unwrap();
    *pos += 8;
    Ok(i64::from_be_bytes(chunk))
}

struct DecodedPayload {
    device_id: String,
    shop_name: String,
    issued_at: i64,
    expires_at: i64,
}

fn decode_payload(bytes: &[u8]) -> Result<DecodedPayload, LicenseError> {
    let mut pos = 0usize;
    let version = *bytes.first().ok_or(LicenseError::InvalidFormat)?;
    if version != 1 {
        return Err(LicenseError::InvalidFormat);
    }
    pos += 1;

    pos += 16; // license_id — not needed for verification, only for admin-side bookkeeping

    let device_id = read_u8_prefixed(bytes, &mut pos)?;
    let shop_name = read_u16_prefixed(bytes, &mut pos)?;
    let issued_at = read_i64(bytes, &mut pos)?;
    let expires_at = read_i64(bytes, &mut pos)?;

    Ok(DecodedPayload { device_id, shop_name, issued_at, expires_at })
}

/// Verifies `key_text` against `verifying_key`, checks it was issued for
/// `expected_device_id`, and checks it hasn't expired as of `now`.
/// Exposed separately from `verify` so tests can exercise this against a
/// throwaway keypair instead of the real embedded one.
fn verify_with_key(
    key_text: &str,
    expected_device_id: &str,
    now: DateTime<Utc>,
    verifying_key: &VerifyingKey,
) -> Result<LicensePayload, LicenseError> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(key_text.trim())
        .map_err(|_| LicenseError::InvalidFormat)?;
    if raw.len() < 64 {
        return Err(LicenseError::InvalidFormat);
    }
    let (payload_bytes, sig_bytes) = raw.split_at(raw.len() - 64);
    let signature = ed25519_dalek::Signature::from_bytes(sig_bytes.try_into().unwrap());
    verifying_key.verify(payload_bytes, &signature).map_err(|_| LicenseError::BadSignature)?;

    let decoded = decode_payload(payload_bytes)?;
    // An empty device_id is a deliberate wildcard — a single publicly
    // distributed key that activates on any machine, rather than the
    // normal one-key-per-device model. Issued with device_id "" via the
    // admin API (never by the per-device Buy License / Razorpay flow).
    if !decoded.device_id.is_empty() && decoded.device_id != expected_device_id {
        return Err(LicenseError::DeviceMismatch);
    }

    let expires_at = if decoded.expires_at == 0 {
        None
    } else {
        let expires_at = DateTime::from_timestamp(decoded.expires_at, 0).ok_or(LicenseError::InvalidFormat)?;
        if now > expires_at {
            return Err(LicenseError::Expired(expires_at.format("%d %b %Y").to_string()));
        }
        Some(expires_at)
    };

    let issued_at = DateTime::from_timestamp(decoded.issued_at, 0).ok_or(LicenseError::InvalidFormat)?;
    Ok(LicensePayload { shop_name: decoded.shop_name, issued_at, expires_at })
}

pub fn verify(key_text: &str, expected_device_id: &str, now: DateTime<Utc>) -> Result<LicensePayload, LicenseError> {
    let verifying_key = VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES).expect("embedded public key must be valid");
    verify_with_key(key_text, expected_device_id, now, &verifying_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use ed25519_dalek::{Signer, SigningKey};
    use uuid::Uuid;

    /// Mirrors the backend's `licensing::sign_license`, but standalone so
    /// this test module doesn't depend on the (separate) backend crate.
    fn sign_test_key(
        signing_key: &SigningKey,
        device_id: &str,
        shop_name: &str,
        issued_at: DateTime<Utc>,
        expires_at: Option<DateTime<Utc>>,
    ) -> String {
        let mut buf = Vec::new();
        buf.push(1u8);
        buf.extend_from_slice(Uuid::new_v4().as_bytes());
        buf.push(device_id.len() as u8);
        buf.extend_from_slice(device_id.as_bytes());
        buf.extend_from_slice(&(shop_name.len() as u16).to_be_bytes());
        buf.extend_from_slice(shop_name.as_bytes());
        buf.extend_from_slice(&issued_at.timestamp().to_be_bytes());
        buf.extend_from_slice(&expires_at.map(|d| d.timestamp()).unwrap_or(0).to_be_bytes());

        let signature = signing_key.sign(&buf);
        buf.extend_from_slice(&signature.to_bytes());
        base64::engine::general_purpose::STANDARD.encode(buf)
    }

    fn test_keypair() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    #[test]
    fn accepts_a_valid_perpetual_key_for_the_right_device() {
        let (signing_key, verifying_key) = test_keypair();
        let key = sign_test_key(&signing_key, "device-a", "Test Shop", Utc::now(), None);

        let payload = verify_with_key(&key, "device-a", Utc::now(), &verifying_key).unwrap();
        assert_eq!(payload.shop_name, "Test Shop");
        assert!(payload.expires_at.is_none());
    }

    #[test]
    fn rejects_a_key_issued_for_a_different_device() {
        let (signing_key, verifying_key) = test_keypair();
        let key = sign_test_key(&signing_key, "device-a", "Test Shop", Utc::now(), None);

        let err = verify_with_key(&key, "device-b", Utc::now(), &verifying_key).unwrap_err();
        assert!(matches!(err, LicenseError::DeviceMismatch));
    }

    #[test]
    fn a_key_with_empty_device_id_activates_on_any_device() {
        let (signing_key, verifying_key) = test_keypair();
        let key = sign_test_key(&signing_key, "", "Universal", Utc::now(), None);

        assert!(verify_with_key(&key, "device-a", Utc::now(), &verifying_key).is_ok());
        assert!(verify_with_key(&key, "some-other-completely-different-device", Utc::now(), &verifying_key).is_ok());
    }

    #[test]
    fn rejects_an_expired_key() {
        let (signing_key, verifying_key) = test_keypair();
        let issued_at = Utc::now() - Duration::days(400);
        let expires_at = Utc::now() - Duration::days(35);
        let key = sign_test_key(&signing_key, "device-a", "Test Shop", issued_at, Some(expires_at));

        let err = verify_with_key(&key, "device-a", Utc::now(), &verifying_key).unwrap_err();
        assert!(matches!(err, LicenseError::Expired(_)));
    }

    #[test]
    fn accepts_a_key_that_has_not_expired_yet() {
        let (signing_key, verifying_key) = test_keypair();
        let expires_at = Utc::now() + Duration::days(30);
        let key = sign_test_key(&signing_key, "device-a", "Test Shop", Utc::now(), Some(expires_at));

        assert!(verify_with_key(&key, "device-a", Utc::now(), &verifying_key).is_ok());
    }

    #[test]
    fn rejects_a_key_signed_by_a_different_keypair() {
        let (_signing_key, verifying_key) = test_keypair();
        let other_signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let key = sign_test_key(&other_signing_key, "device-a", "Test Shop", Utc::now(), None);

        let err = verify_with_key(&key, "device-a", Utc::now(), &verifying_key).unwrap_err();
        assert!(matches!(err, LicenseError::BadSignature));
    }

    #[test]
    fn rejects_garbage_input() {
        let (_signing_key, verifying_key) = test_keypair();
        let err = verify_with_key("not-a-real-license-key", "device-a", Utc::now(), &verifying_key).unwrap_err();
        assert!(matches!(err, LicenseError::InvalidFormat));
    }

    /// Cross-compatibility check: a key actually issued by the running
    /// backend's admin API (`POST /admin/licenses`) during manual testing,
    /// verified here against the real embedded production public key —
    /// confirms the two independently-implemented wire formats agree.
    /// Issued 2026-08-25 with a 365-day expiry — if this test starts
    /// failing after 2027-08-25, that's why; regenerate a fresh fixture
    /// via `POST /admin/licenses` rather than treat it as a real bug.
    #[test]
    fn verifies_a_key_actually_issued_by_the_backend_admin_api() {
        let key = "AVT8zU5tzU1stTP/UDLz0lwkMTExMTExMTEtMjIyMi0zMzMzLTQ0NDQtNTU1NTU1NTU1NTU1ABJUZXN0IEhhcmR3YXJlIFNob3AAAAAAao1ADQAAAABsbnONT+PPK7xkNRbGNAdotwZN3fLuZBmD5XzXvUzqbxJDJmVdO9sredw9bTwWmlhvIjGT0OHOHppRWorPijoQdRxlCQ==";
        let payload = verify(key, "11111111-2222-3333-4444-555555555555", Utc::now()).unwrap();
        assert_eq!(payload.shop_name, "Test Hardware Shop");
        assert!(payload.expires_at.is_some());
    }

    /// The actual universal (empty-device_id) key published on
    /// open-source.srotas.space/products/desk/license — issued 2026-08-27
    /// via `POST /admin/licenses` with `device_id: ""`. Confirms it
    /// activates on an arbitrary device against the real embedded public
    /// key, not just a throwaway test keypair.
    #[test]
    fn the_published_universal_key_activates_on_any_device() {
        let key = "AR9XD6WnnkgUjtaPDg2Xy3QAAAtTcm90YXMgRGVzawAAAABqj8WdAAAAAAAAAABlosBH6RDtxjB1orI7noaUIoU2i3bnJFSvVdp7Bu7PglyFkOZXzbtpVNFhSSKlVTTfUgDqCmvwab/5FQ07dA4L";
        let payload = verify(key, "any-random-device-id-whatsoever", Utc::now()).unwrap();
        assert_eq!(payload.shop_name, "Srotas Desk");
        assert!(payload.expires_at.is_none());
    }
}
