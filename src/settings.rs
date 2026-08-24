use chrono::NaiveDate;
use std::path::PathBuf;

/// Persisted app settings — currently just backup configuration. Stored as
/// plain `key=value` lines rather than JSON so we don't need to pull in
/// serde for two fields.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub backup_folder: Option<PathBuf>,
    pub last_backup_date: Option<NaiveDate>,
}

fn settings_path() -> PathBuf {
    let mut dir = crate::db::db_path();
    dir.pop(); // drop "shop.db", keep the app data directory
    dir.push("settings.txt");
    dir
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };

    let mut settings = Settings::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "backup_folder" if !value.is_empty() => settings.backup_folder = Some(PathBuf::from(value)),
            "last_backup_date" => settings.last_backup_date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
            _ => {}
        }
    }
    settings
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }

    let folder = settings
        .backup_folder
        .as_ref()
        .and_then(|p: &PathBuf| p.to_str())
        .unwrap_or("");
    let date = settings
        .last_backup_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();

    std::fs::write(path, format!("backup_folder={folder}\nlast_backup_date={date}\n"))
}
