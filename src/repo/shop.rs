use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::RepoError;
use crate::models::ShopProfile;

/// The column list every `ShopProfile` query selects (or `RETURNING`s).
/// Kept in one place because three separate queries hydrate the same
/// struct, and adding a field used to mean editing all of them.
///
/// A macro rather than a `const` so the queries below stay `concat!`ed
/// string *literals*: sqlx refuses a runtime-built query string unless
/// it's explicitly waved through as injection-audited, and there's no
/// reason to give up that check just to avoid retyping a column list.
macro_rules! profile_columns {
    () => {
        "shop_name, owner_name, phone, address, pin_hash, \
         (logo IS NOT NULL) AS has_logo, gst_rate_bp, gstin, pin_failed_attempts, pin_locked_until"
    };
}

/// `None` means the app has never been registered on this machine yet —
/// the caller should show the registration screen instead of login/home.
pub async fn get_shop_profile(pool: &SqlitePool) -> Result<Option<ShopProfile>, RepoError> {
    let profile = sqlx::query_as::<_, ShopProfile>(concat!(
        "SELECT ", profile_columns!(), " FROM shop_profile WHERE id = 1"
    ))
    .fetch_optional(pool)
    .await?;
    Ok(profile)
}

/// Re-hashes a PIN left over from before `migrations/0009` and blanks the
/// old plaintext column. Safe (and cheap) to call on every launch: after
/// the first run there's no plaintext left to find, so this is one
/// `SELECT` that returns nothing.
///
/// Hashing is deliberately slow, so it runs on a blocking thread — but it
/// happens at most once per install, on a single row.
pub async fn upgrade_legacy_pin(pool: &SqlitePool) -> Result<(), RepoError> {
    let legacy: Option<String> =
        sqlx::query_scalar("SELECT pin FROM shop_profile WHERE id = 1 AND pin IS NOT NULL AND pin <> ''")
            .fetch_optional(pool)
            .await?
            .flatten();

    let Some(pin) = legacy else {
        return Ok(());
    };

    // A failure here must not brick the app on startup — leave the row
    // alone and let the shopkeeper log in with the old plaintext path on
    // the next attempt rather than losing their PIN entirely.
    let Ok(hash) = tokio::task::spawn_blocking(move || crate::pin::hash(&pin)).await else {
        return Ok(());
    };
    let Ok(hash) = hash else {
        return Ok(());
    };

    sqlx::query("UPDATE shop_profile SET pin_hash = ?, pin = NULL WHERE id = 1")
        .bind(&hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Registers (or re-registers) this machine's single shop. `id` is pinned
/// to 1 by the schema, so this is always an upsert of that one row.
/// `pin_hash` is already an Argon2 PHC string — hashing happens in the
/// caller (see `ui::register`), never here.
pub async fn register_shop(
    pool: &SqlitePool,
    shop_name: &str,
    owner_name: &str,
    phone: &str,
    address: &str,
    pin_hash: Option<&str>,
) -> Result<ShopProfile, RepoError> {
    let profile = sqlx::query_as::<_, ShopProfile>(concat!(
        "INSERT INTO shop_profile (id, shop_name, owner_name, phone, address, pin_hash, created_at) \
         VALUES (1, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET \
             shop_name = excluded.shop_name, \
             owner_name = excluded.owner_name, \
             phone = excluded.phone, \
             address = excluded.address, \
             pin_hash = excluded.pin_hash, \
             pin = NULL, \
             pin_failed_attempts = 0, \
             pin_locked_until = NULL \
         RETURNING ", profile_columns!()
    ))
    .bind(shop_name)
    .bind(owner_name)
    .bind(phone)
    .bind(address)
    .bind(pin_hash)
    .bind(Utc::now())
    .fetch_one(pool)
    .await?;

    Ok(profile)
}

/// Edits the shop's name/owner/phone/address/GSTIN/default GST rate. Does
/// not touch the PIN or logo — those have their own dedicated update
/// functions since they're edited from a different part of Settings.
pub async fn update_profile(
    pool: &SqlitePool,
    shop_name: &str,
    owner_name: &str,
    phone: &str,
    address: &str,
    gstin: Option<&str>,
    gst_rate_bp: i64,
) -> Result<ShopProfile, RepoError> {
    let profile = sqlx::query_as::<_, ShopProfile>(concat!(
        "UPDATE shop_profile \
         SET shop_name = ?, owner_name = ?, phone = ?, address = ?, gstin = ?, gst_rate_bp = ? \
         WHERE id = 1 \
         RETURNING ", profile_columns!()
    ))
    .bind(shop_name)
    .bind(owner_name)
    .bind(phone)
    .bind(address)
    .bind(gstin)
    .bind(gst_rate_bp)
    .fetch_one(pool)
    .await?;
    Ok(profile)
}

/// Sets, changes, or removes (`None`) the screen-lock PIN. Any lockout is
/// cleared at the same time — whoever just proved they could change the
/// PIN shouldn't then be kept out by an older run of wrong guesses.
pub async fn update_pin(pool: &SqlitePool, pin_hash: Option<&str>) -> Result<(), RepoError> {
    sqlx::query(
        "UPDATE shop_profile \
         SET pin_hash = ?, pin = NULL, pin_failed_attempts = 0, pin_locked_until = NULL \
         WHERE id = 1",
    )
    .bind(pin_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// The stored PIN hash, read fresh from the database rather than from
/// whatever the UI is holding — verification should never depend on a
/// possibly-stale in-memory copy.
pub async fn get_pin_hash(pool: &SqlitePool) -> Result<Option<String>, RepoError> {
    let hash: Option<String> =
        sqlx::query_scalar("SELECT pin_hash FROM shop_profile WHERE id = 1").fetch_optional(pool).await?.flatten();
    Ok(hash)
}

/// Records one wrong PIN and returns the resulting attempt count and
/// lockout deadline. The count is incremented in SQL rather than read,
/// bumped and written back, so two rapid attempts can't both read the
/// same starting value and lose one of the increments.
pub async fn record_failed_pin(pool: &SqlitePool) -> Result<(i64, Option<DateTime<Utc>>), RepoError> {
    let attempts: i64 = sqlx::query_scalar(
        "UPDATE shop_profile SET pin_failed_attempts = pin_failed_attempts + 1 \
         WHERE id = 1 RETURNING pin_failed_attempts",
    )
    .fetch_one(pool)
    .await?;

    let locked_until = crate::pin::lockout_for(attempts).map(|d| Utc::now() + d);
    if locked_until.is_some() {
        sqlx::query("UPDATE shop_profile SET pin_locked_until = ? WHERE id = 1")
            .bind(locked_until)
            .execute(pool)
            .await?;
    }

    Ok((attempts, locked_until))
}

/// Clears the failed-attempt counter and any lockout — called on every
/// successful unlock.
pub async fn clear_pin_failures(pool: &SqlitePool) -> Result<(), RepoError> {
    sqlx::query("UPDATE shop_profile SET pin_failed_attempts = 0, pin_locked_until = NULL WHERE id = 1")
        .execute(pool)
        .await?;
    Ok(())
}

/// The lockout deadline as currently stored. Read before verifying a PIN
/// so a lockout can't be sidestepped by relaunching the app (which would
/// otherwise start from a fresh, empty in-memory profile).
pub async fn get_pin_lock(pool: &SqlitePool) -> Result<Option<DateTime<Utc>>, RepoError> {
    let locked_until: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT pin_locked_until FROM shop_profile WHERE id = 1")
            .fetch_optional(pool)
            .await?
            .flatten();
    Ok(locked_until)
}

pub async fn update_logo(pool: &SqlitePool, logo: Option<&[u8]>) -> Result<(), RepoError> {
    sqlx::query("UPDATE shop_profile SET logo = ? WHERE id = 1").bind(logo).execute(pool).await?;
    Ok(())
}

pub async fn get_shop_logo(pool: &SqlitePool) -> Result<Option<Vec<u8>>, RepoError> {
    let logo: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT logo FROM shop_profile WHERE id = 1").fetch_optional(pool).await?.flatten();
    Ok(logo)
}
