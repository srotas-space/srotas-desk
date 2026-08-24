use chrono::Utc;
use sqlx::SqlitePool;

use super::RepoError;
use crate::models::ShopProfile;

/// `None` means the app has never been registered on this machine yet —
/// the caller should show the registration screen instead of login/home.
pub async fn get_shop_profile(pool: &SqlitePool) -> Result<Option<ShopProfile>, RepoError> {
    let profile = sqlx::query_as::<_, ShopProfile>(
        "SELECT shop_name, owner_name, phone, address, pin, created_at FROM shop_profile WHERE id = 1",
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
         RETURNING shop_name, owner_name, phone, address, pin, created_at",
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
