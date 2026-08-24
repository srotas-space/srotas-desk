use sqlx::SqlitePool;
use std::path::Path;

use super::RepoError;

/// Writes a fully consistent snapshot of the live database to `dest_path`.
///
/// Uses SQLite's own `VACUUM INTO` rather than copying the file with
/// `std::fs::copy` — a plain file copy can land mid-write (or miss a
/// rollback journal) if a transaction happens to be in flight at the exact
/// moment of the backup. `VACUUM INTO` asks SQLite itself for a clean,
/// complete copy, which is what a backup is for.
pub async fn backup_to(pool: &SqlitePool, dest_path: &Path) -> Result<(), RepoError> {
    let dest = dest_path
        .to_str()
        .ok_or_else(|| RepoError::Db(sqlx::Error::Configuration("backup path is not valid UTF-8".into())))?;

    sqlx::query("VACUUM INTO ?").bind(dest).execute(pool).await?;
    Ok(())
}
