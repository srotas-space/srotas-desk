// Ignored on non-Windows targets. On Windows, without this the app would
// launch with a console window flashing up behind the GUI window, since
// Rust binaries default to the "console" subsystem.
#![windows_subsystem = "windows"]

mod crashlog;
mod db;
mod license;
mod models;
mod money;
mod pdf;
mod pin;
mod repo;
mod settings;
mod ui;

fn main() -> iced::Result {
    // First thing, before anything can fail: on Windows this process has
    // no console, so without the log a panic is a window that flashes and
    // disappears with nothing written down.
    crashlog::install();
    ui::run()
}
