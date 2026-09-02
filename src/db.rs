use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// This install's folder inside the OS-standard app-data directory.
///
/// `None` only if the OS won't name that directory at all, which on a
/// healthy machine does not happen — but it is returned rather than
/// panicked on, because a panic here kills the app before it can say why.
/// On Windows the process has no console (see `windows_subsystem` in
/// `main.rs`), so a panic there is a window that flashes and vanishes with
/// nothing written anywhere.
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|mut dir| {
        dir.push("srotas-desk");
        dir
    })
}

/// Where the shop's single SQLite file lives.
///
/// We use the OS-standard app-data directory (not the current working
/// directory) because the installed app will be launched from a desktop
/// shortcut, not a terminal sitting in the project folder — the data has to
/// live somewhere stable regardless of how the app is started.
pub fn db_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("shop.db"))
}

/// Opens (creating if needed) the shop database and brings its schema up to
/// date. Safe to call every time the app starts — `sqlx::migrate!` only
/// applies migrations that haven't run yet.
pub async fn connect_and_migrate() -> Result<SqlitePool, sqlx::Error> {
    let path = db_path().ok_or_else(|| {
        sqlx::Error::Configuration("this computer has no app-data folder for Srotas Desk to use".into())
    })?;
    if let Some(dir) = path.parent() {
        // A failure here is almost always a permissions problem — a locked
        // -down or roaming profile. Reported, not panicked on, so the
        // screen can show what went wrong.
        std::fs::create_dir_all(dir).map_err(|e| {
            sqlx::Error::Configuration(format!("could not create {}: {e}", dir.display()).into())
        })?;
    }

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
