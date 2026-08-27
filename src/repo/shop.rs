use chrono::Utc;
use sqlx::SqlitePool;

use super::RepoError;
use crate::models::ShopProfile;

/// `None` means the app has never been registered on this machine yet —
/// the caller should show the registration screen instead of login/home.
pub async fn get_shop_profile(pool: &SqlitePool) -> Result<Option<ShopProfile>, RepoError> {
    let profile = sqlx::query_as::<_, ShopProfile>(
        "SELECT shop_name, owner_name, phone, address, pin, created_at, (logo IS NOT NULL) AS has_logo, gst_rate_bp, gstin \
         FROM shop_profile WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(profile)
}

/// Registers (or re-registers) this machine's single shop. `id` is pinned
/// to 1 by the schema, so this is always an upsert of that one row.
pub async fn register_shop(
    pool: &SqlitePool,
    shop_name: &str,
    owner_name: &str,
    phone: &str,
    address: &str,
    pin: Option<&str>,
) -> Result<ShopProfile, RepoError> {
    let profile = sqlx::query_as::<_, ShopProfile>(
        "INSERT INTO shop_profile (id, shop_name, owner_name, phone, address, pin, created_at) \
         VALUES (1, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET \
             shop_name = excluded.shop_name, \
             owner_name = excluded.owner_name, \
             phone = excluded.phone, \
             address = excluded.address, \
             pin = excluded.pin \
         RETURNING shop_name, owner_name, phone, address, pin, created_at, (logo IS NOT NULL) AS has_logo, gst_rate_bp, gstin",
    )
    .bind(shop_name)
    .bind(owner_name)
    .bind(phone)
    .bind(address)
    .bind(pin)
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
    let profile = sqlx::query_as::<_, ShopProfile>(
        "UPDATE shop_profile \
         SET shop_name = ?, owner_name = ?, phone = ?, address = ?, gstin = ?, gst_rate_bp = ? \
         WHERE id = 1 \
         RETURNING shop_name, owner_name, phone, address, pin, created_at, (logo IS NOT NULL) AS has_logo, gst_rate_bp, gstin",
    )
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

/// Sets, changes, or removes (`None`) the screen-lock PIN.
pub async fn update_pin(pool: &SqlitePool, pin: Option<&str>) -> Result<(), RepoError> {
    sqlx::query("UPDATE shop_profile SET pin = ? WHERE id = 1").bind(pin).execute(pool).await?;
    Ok(())
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
