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

/// Marker dropped when the app dies inside the graphics stack. Holds the
/// version that crashed, so a later build gets a fresh attempt at the GPU
/// rather than being condemned to software rendering forever by one bad
/// afternoon with an old driver.
fn renderer_marker() -> Option<PathBuf> {
    crate::db::data_dir().map(|dir| dir.join("use-software-renderer"))
}

/// Was this panic the graphics stack giving up?
///
/// Matched on the message rather than the type because the panic arrives
/// as a formatted string from deep inside wgpu — there is no error type to
/// downcast to by the time it reaches a panic hook.
///
/// The real case this catches, reported from a Windows install:
///
/// ```text
/// Error in Surface::present: Validation Error
/// Caused by:
///   Parent device is lost
/// ```
///
/// A lost device means the driver reset underneath us — a GPU timeout, a
/// remote-desktop session taking the adapter away, a virtual machine
/// without real 3D. None of it is recoverable in place, and all of it is
/// survivable by drawing in software instead.
fn is_graphics_failure(message: &str) -> bool {
    const SIGNS: [&str; 6] = [
        "device is lost",
        "Surface::present",
        "wgpu",
        "no suitable adapter",
        "NoAvailableAdapter",
        "SurfaceError",
    ];
    SIGNS.iter().any(|sign| message.contains(sign))
}

/// Whether this launch should draw in software rather than on the GPU.
///
/// True when a previous run of *this same version* died in the graphics
/// stack. A different version clears the marker and tries the GPU again:
/// the shopkeeper may well have updated their drivers in between, and
/// software rendering is a fallback, not a destination.
pub fn should_use_software_renderer() -> bool {
    let Some(marker) = renderer_marker() else {
        return false;
    };
    let Ok(recorded) = std::fs::read_to_string(&marker) else {
        return false;
    };

    if recorded.trim() == env!("CARGO_PKG_VERSION") {
        true
    } else {
        let _ = std::fs::remove_file(&marker);
        false
    }
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

    // A graphics death is the one kind the app can do something about, so
    // leave a note for the next launch to pick up.
    if is_graphics_failure(&entry) {
        if let Some(marker) = renderer_marker() {
            let _ = std::fs::write(marker, env!("CARGO_PKG_VERSION"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_windows_device_loss_that_prompted_this() {
        let real = "message   : Error in Surface::present: Validation Error\n\n\
                    Caused by:\n  Parent device is lost\n";
        assert!(is_graphics_failure(real));
    }

    #[test]
    fn recognises_an_adapter_that_never_arrived() {
        assert!(is_graphics_failure("called `Option::unwrap()` on a `None` value: no suitable adapter found"));
        assert!(is_graphics_failure("NoAvailableAdapter"));
    }

    #[test]
    fn leaves_ordinary_panics_alone() {
        // An unrelated crash must not quietly downgrade everyone to
        // software rendering for the rest of the version's life.
        assert!(!is_graphics_failure("attempt to divide by zero"));
        assert!(!is_graphics_failure("could not create /Users/x/Library/...: permission denied"));
        assert!(!is_graphics_failure("index out of bounds: the len is 3 but the index is 7"));
    }
}
