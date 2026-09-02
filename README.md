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

## Building a release

Each platform has a script under `packaging/` that builds a release
binary and wraps it the way that OS expects — a real `.app` bundle on
macOS, a `.desktop` entry and icon on Linux, an `.exe` with its icon
embedded on Windows.

```bash
./packaging/macos/package.sh          # macOS (must run on a Mac)
./packaging/linux/package-docker.sh   # Linux, cross-built via Docker
./packaging/windows/package-cross.sh  # Windows, cross-built via mingw-w64
```

Pushing a tag like `v0.1.0` builds all three natively on GitHub's runners
and attaches them to a Release.

**See `BUILD.md`** for the per-platform detail: what each host can build,
the traps (running the Linux script directly on macOS silently produces a
macOS binary), how to test a build you can't run natively, and the
release checklist.

## Data & backups

All data lives in one SQLite file in the OS-standard app-data folder
(e.g. `~/Library/Application Support/srotas-desk/shop.db` on macOS,
`%APPDATA%\srotas-desk\shop.db` on Windows, `~/.local/share/srotas-desk/shop.db`
on Linux). It survives restarts, updates, and reinstalls of the app
itself — the only real risk is disk failure, which is what the in-app
**Backup** screen (under Inventory) is for. Back up regularly to a
pendrive or a folder that syncs to cloud storage.

## Further reading

- **`BUILD.md`** — building for Linux, macOS and Windows, and cutting a
  release.
- **`INSTALL.md`** — installing, first-run setup and uninstalling a
  downloaded release, for end users.
- **`demo.sh` / `reset.sh`** — fill a development install with 100,000
  demo items, or wipe it back to a blank slate. Both take `--help`.

