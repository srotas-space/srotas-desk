fn main() {
    // Embeds the app icon into the .exe's resources so Explorer, the
    // taskbar, and the shortcut all show the logo instead of a generic
    // icon. macOS/Linux icons are handled separately at packaging time.
    //
    // Build scripts always compile for (and run on) the *host*, even when
    // cross-compiling — so `#[cfg(target_os = "windows")]` here would
    // reflect the host, not the target, and silently never fire when
    // cross-building a Windows exe from macOS/Linux. `CARGO_CFG_TARGET_OS`
    // is the one that actually reflects the target being built.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon: {e}");
        }
    }
}
