// Ignored on non-Windows targets. On Windows, without this the app would
// launch with a console window flashing up behind the GUI window, since
// Rust binaries default to the "console" subsystem.
#![windows_subsystem = "windows"]

mod db;
mod models;
mod money;
mod pdf;
mod repo;
mod settings;
mod ui;

fn main() -> iced::Result {
    ui::run()
}
