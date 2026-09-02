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

    // If the last run of this version died in the graphics stack, draw in
    // software this time. A shop counter running an old driver, a virtual
    // machine, or a remote-desktop session can all lose the GPU device
    // mid-frame — wgpu panics, and an app that only renders one way simply
    // stops opening. Slower to redraw; still an app.
    //
    // Set before iced starts and never overriding a value the operator
    // chose by hand. Safe despite `set_var` being unsafe in edition 2024:
    // this is the first statement of `main`, single-threaded, before any
    // reader of the environment exists.
    if crashlog::should_use_software_renderer() && std::env::var_os("ICED_BACKEND").is_none() {
        unsafe { std::env::set_var("ICED_BACKEND", "tiny-skia") };
    }

    ui::run()
}
