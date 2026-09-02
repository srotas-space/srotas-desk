use chrono::NaiveDate;
use std::path::PathBuf;

/// Persisted app settings — backup configuration and how the catalogue is
/// held. Stored as plain `key=value` lines rather than JSON so we don't
/// need to pull in serde for three fields.
///
/// These live in a file beside the database rather than *in* it on
/// purpose: they describe this machine (where its pendrive is, how much
/// RAM it can spare), not the shop. Restoring a backup onto a weaker
/// computer should not drag the old machine's memory settings along.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub backup_folder: Option<PathBuf>,
    pub last_backup_date: Option<NaiveDate>,
    /// Whether to hold the whole catalogue in memory.
    ///
    /// Off by default, and deliberately so: the app then asks the database
    /// for exactly the rows a screen is about to draw, which costs the
    /// same whether the shop stocks fifty items or a hundred thousand.
    /// Turning it on trades memory for instant in-memory searching — worth
    /// it on a machine with RAM to spare and a catalogue small enough to
    /// fit. See `ui::catalogue`.
    pub preload_catalogue: bool,
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
            // Anything but a literal "true" means off, so a corrupt or
            // hand-edited value fails to the cheaper mode rather than
            // silently loading a huge catalogue.
            "preload_catalogue" => settings.preload_catalogue = value.trim() == "true",
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

    std::fs::write(
        path,
        format!(
            "backup_folder={folder}\nlast_backup_date={date}\npreload_catalogue={}\n",
            settings.preload_catalogue
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parser, exercised directly — `load`/`save` themselves touch the
    /// real app-data directory, which a test has no business writing to.
    fn parse(contents: &str) -> Settings {
        let mut settings = Settings::default();
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "backup_folder" if !value.is_empty() => settings.backup_folder = Some(PathBuf::from(value)),
                "last_backup_date" => settings.last_backup_date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
                "preload_catalogue" => settings.preload_catalogue = value.trim() == "true",
                _ => {}
            }
        }
        settings
    }

    #[test]
    fn preload_is_off_unless_explicitly_switched_on() {
        assert!(!Settings::default().preload_catalogue);
        // A settings file written before this option existed.
        assert!(!parse("backup_folder=/tmp\nlast_backup_date=2026-01-01\n").preload_catalogue);
        assert!(parse("preload_catalogue=true\n").preload_catalogue);
        assert!(!parse("preload_catalogue=false\n").preload_catalogue);
    }

    #[test]
    fn a_junk_preload_value_falls_back_to_off() {
        // Failing to the cheaper mode matters: the expensive one can make
        // a modest machine unusable on a large catalogue.
        for junk in ["yes", "1", "TRUE", "", "maybe"] {
            assert!(!parse(&format!("preload_catalogue={junk}\n")).preload_catalogue, "{junk}");
        }
    }
}
