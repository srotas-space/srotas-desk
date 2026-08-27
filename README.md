# Srotas Desk

A desktop inventory and billing app for a single hardware shop counter.
Runs fully offline — everything is stored in one local SQLite file, no
server or cloud involved.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- **Linux only** — a few system libraries the GUI needs to build/run:

  ```bash
  sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev \
    libxrandr-dev libxi-dev libxcursor-dev libgl1-mesa-dev \
    libfontconfig1-dev libgtk-3-dev
  ```

macOS and Windows need nothing extra beyond Rust itself.

## Running it during development

```bash
cargo run
```

This is enough for day-to-day development — it launches straight from
`target/debug`, no packaging needed.

## Deploying a real build

Each platform has a script under `packaging/` that builds a release
binary and wraps it the way that OS expects (proper app icon, app-menu
entry, etc.), so it doesn't just look like a bare executable.

### macOS

```bash
./packaging/macos/package.sh
```

Produces `dist/macos/Srotas Desk.app` — a real `.app` bundle with the
logo as its icon. Double-click it, or:

```bash
open "dist/macos/Srotas Desk.app"
```

To hand it to someone else, zip the `.app` and share the zip. It isn't
code-signed, so first launch will need a right-click → Open (or System
Settings → Privacy & Security → "Open Anyway") to get past Gatekeeper —
expected for an app without an Apple Developer ID.

### Windows

You do **not** need a Windows machine — this can be cross-compiled from
macOS or Linux using the mingw-w64 toolchain:

```bash
# macOS
brew install mingw-w64
# Ubuntu/Debian
sudo apt-get install mingw-w64

./packaging/windows/package-cross.sh
```

Or, on an actual Windows machine with Rust installed, from PowerShell:

```powershell
./packaging/windows/package.ps1
```

Either way it produces `dist/windows/srotas-desk-windows.zip`,
containing `srotas-desk.exe` — a GUI-subsystem exe (no console window
flashing up) with the logo already embedded as its icon (shows up in
Explorer and the taskbar). Unzip and run it directly — no installer
needed. Since it isn't signed with a code-signing certificate,
SmartScreen may show an "Unknown publisher" warning the first time;
choose "More info" → "Run anyway".

### Ubuntu / Linux

`package.sh` must run on an actual Linux machine (or Linux CI) —
`cargo build --release` just builds for whatever OS it's invoked on, so
running this script directly on macOS silently produces a macOS binary
wrapped in Linux-shaped packaging. It looks fine until someone on Ubuntu
actually tries to run it.

On Linux:

```bash
./packaging/linux/package.sh
```

From macOS (or any non-Linux host), cross-build via Docker instead — no
Linux machine needed:

```bash
./packaging/linux/package-docker.sh
```

This builds inside an Ubuntu 22.04 container (Docker Desktop must be
running) with the same apt packages the CI job installs, so the output
matches what CI would produce. It's slower than a native build on Apple
Silicon (the container runs under emulation to produce a real x86_64
binary), but only needs to run occasionally for a release.

Either way it produces `dist/linux/srotas-desk-linux.tar.gz`. On the
target machine:

```bash
tar xzf srotas-desk-linux.tar.gz
cd srotas-desk
./install.sh
```

This installs the binary to `~/.local/bin`, the icon to
`~/.local/share/icons`, and an app-menu entry to
`~/.local/share/applications` — no root required. Make sure
`~/.local/bin` is on your `PATH`, then launch "Srotas Desk" from your
application menu, or run `srotas-desk` directly.

### Building all three at once (CI)

Pushing a tag like `v0.1.0` triggers `.github/workflows/release.yml`,
which builds macOS, Ubuntu, and Windows packages in parallel on GitHub's
own runners and attaches them to a GitHub Release automatically:

```bash
git tag v0.1.0
git push origin v0.1.0
```

(Requires this repo to have a GitHub remote configured.)

## Data & backups

All data lives in one SQLite file in the OS-standard app-data folder
(e.g. `~/Library/Application Support/srotas-desk/shop.db` on macOS,
`%APPDATA%\srotas-desk\shop.db` on Windows, `~/.local/share/srotas-desk/shop.db`
on Linux). It survives restarts, updates, and reinstalls of the app
itself — the only real risk is disk failure, which is what the in-app
**Backup** screen (under Inventory) is for. Back up regularly to a
pendrive or a folder that syncs to cloud storage.
