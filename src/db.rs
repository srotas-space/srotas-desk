use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Where the shop's single SQLite file lives.
///
/// We use the OS-standard app-data directory (not the current working
/// directory) because the installed app will be launched from a desktop
/// shortcut, not a terminal sitting in the project folder — the data has to
/// live somewhere stable regardless of how the app is started.
pub fn db_path() -> PathBuf {
    let mut dir = dirs::data_dir().expect("could not resolve OS data directory");
    dir.push("srotas-desk");
    dir.push("shop.db");
    dir
}

/// Opens (creating if needed) the shop database and brings its schema up to
/// date. Safe to call every time the app starts — `sqlx::migrate!` only
/// applies migrations that haven't run yet.
pub async fn connect_and_migrate() -> Result<SqlitePool, sqlx::Error> {
    let path = db_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("could not create app data directory");
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
