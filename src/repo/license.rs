use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::RepoError;

pub struct LicenseRow {
    pub device_id: String,
    pub key_text: Option<String>,
}

/// Returns this machine's license row, creating it (with a freshly
/// generated, permanent device id) on the very first call. Safe to call
/// on every launch — after the first run this is just a read.
pub async fn get_or_create(pool: &SqlitePool) -> Result<LicenseRow, RepoError> {
    if let Some(row) = sqlx::query_as::<_, (String, Option<String>)>("SELECT device_id, key_text FROM license WHERE id = 1")
        .fetch_optional(pool)
        .await?
    {
        return Ok(LicenseRow { device_id: row.0, key_text: row.1 });
    }

    let device_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO license (id, device_id) VALUES (1, ?)").bind(&device_id).execute(pool).await?;
    Ok(LicenseRow { device_id, key_text: None })
}

/// Stores a newly-activated (and already-verified by the caller) license
/// key against this machine's device id.
pub async fn activate(pool: &SqlitePool, key_text: &str) -> Result<(), RepoError> {
    sqlx::query("UPDATE license SET key_text = ?, activated_at = ? WHERE id = 1")
        .bind(key_text)
        .bind(Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}
