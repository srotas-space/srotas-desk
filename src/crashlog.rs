//! Records why the app died, for the one platform that can't tell you.
//!
//! Windows binaries here are built for the "windows" subsystem (see
//! `main.rs`), which means the process has no console. A panic on that
//! target writes its message to a stderr nobody is reading: the window
//! appears, vanishes, and the shopkeeper is left with an app that "opens
//! and closes straight away" and no way to say more than that.
//!
//! So every panic is also appended to `crash.log` beside the shop
//! database, where it can be found and sent on. The file is only ever
//! written *by* a panic, so an install that has never crashed doesn't have
//! one.

use std::backtrace::Backtrace;
use std::io::Write;
use std::path::PathBuf;

/// Where the log lives — beside `shop.db`, because that is the folder
/// `INSTALL.md` already teaches people to find.
pub fn path() -> Option<PathBuf> {
    crate::db::data_dir().map(|dir| dir.join("crash.log"))
}

/// Installs the panic hook. Call once, first thing in `main`.
///
/// The default hook still runs afterwards, so a terminal launch keeps
/// printing what it always did; this only adds the file.
pub fn install() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        // Capture before anything else can panic in turn.
        let backtrace = Backtrace::force_capture();
        write_entry(info, &backtrace);
        default_hook(info);
    }));
}

fn write_entry(info: &std::panic::PanicHookInfo<'_>, backtrace: &Backtrace) {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        // Best-effort: if the directory can't be made, there is nowhere to
        // report that fact to either.
        let _ = std::fs::create_dir_all(dir);
    }

    let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_else(|| "unknown".into());

    let entry = format!(
        "\n=== Srotas Desk {} crashed ===\n\
         when      : {}\n\
         platform  : {} {}\n\
         at        : {}\n\
         message   : {}\n\
         backtrace :\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        location,
        info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info
            .payload()
            .downcast_ref::<String>()
            .cloned())
            .unwrap_or_else(|| "(no message)".into()),
    );

    // Appended, not truncated: a crash that only happens on the third
    // launch is worth as much as the first, and the file stays small
    // because it only grows on a crash.
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(entry.as_bytes());
    }
}
