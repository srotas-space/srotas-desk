# Srotas Desk

A desktop inventory and billing app for a single hardware shop counter.
Runs fully offline — everything is stored in one local SQLite file, no
server or cloud involved.

Runs on **Ubuntu/Linux, macOS and Windows**.

© 2026 [Srotas](https://srotas.space). All rights reserved.

---

## Install it (for shopkeepers)

No build tools needed — download, unzip, run. Every link below always
points at the newest release, so they never go stale.

| | Download |
| --- | --- |
| **Ubuntu / Linux** | [srotas-desk-linux.tar.gz](https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-linux.tar.gz) |
| **macOS** | [srotas-desk-macos.zip](https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-macos.zip) |
| **Windows** | [srotas-desk-windows.zip](https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-windows.zip) |

Or from the terminal:

```bash
# Ubuntu / Linux — installs for the current user, no root needed
curl -L -o srotas-desk-linux.tar.gz \
  https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-linux.tar.gz
tar xzf srotas-desk-linux.tar.gz
cd srotas-desk && ./install.sh
```

```bash
# macOS — first launch needs right-click → Open (the app isn't signed)
curl -L -o srotas-desk-macos.zip \
  https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-macos.zip
unzip srotas-desk-macos.zip
mv "Srotas Desk.app" /Applications/
```

```powershell
# Windows — SmartScreen shows "More info" → "Run anyway" the first time
curl.exe -L -o srotas-desk-windows.zip `
  https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-windows.zip
Expand-Archive srotas-desk-windows.zip -DestinationPath "$env:LOCALAPPDATA\Srotas Desk"
```

First launch asks for a licence key — get one from
[open-source.srotas.space/products/desk/license](https://open-source.srotas.space/products/desk/license).

**See [`INSTALL.md`](INSTALL.md)** for the full walkthrough: the
first-launch warning each OS shows and how to get past it, activation,
shop setup, the counter PIN, where your data lives, and uninstalling.

---

## Build it from source

### Requirements

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- **Linux only** — GUI development headers:

  ```bash
  sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev \
    libxrandr-dev libxi-dev libxcursor-dev libgl1-mesa-dev \
    libfontconfig1-dev libgtk-3-dev
  ```

macOS and Windows need nothing beyond Rust itself.

### Running during development

```bash
cargo run
```

Launches straight from `target/debug` — no packaging needed. Two helper
scripts make a development install easier to work with:

```bash
./demo.sh     # fill the catalogue with 100,000 demo items
./reset.sh    # wipe the install back to a blank slate
```

Both take `--help`.

### Building a release locally

Each platform has a script under `packaging/` that wraps the binary the
way that OS expects — a real `.app` bundle on macOS, a `.desktop` entry
and icon on Linux, an `.exe` with its icon embedded on Windows.

```bash
./packaging/macos/package.sh          # macOS (must run on a Mac)
./packaging/linux/package-docker.sh   # Linux, cross-built via Docker
./packaging/windows/package-cross.sh  # Windows, cross-built via mingw-w64
```

**See [`BUILD.md`](BUILD.md)** for per-platform detail: which host can
build what, the traps (running the Linux script directly on macOS
silently produces a macOS binary), and how to test a build you can't run
natively.

---

## Publishing a release

Pushing a tag matching `v*` is the whole release process:
`.github/workflows/release.yml` builds all three platforms **natively**
on GitHub's own runners and attaches the artifacts to a GitHub Release.
Nothing is uploaded by hand.

### 1. Commit your work

```bash
git status                 # check what's going out
git add -A
git commit -m "Describe what changed"
git push origin main
```

### 2. Bump the version

Edit `version` in `Cargo.toml`. The tag you push must match it — that
version is what tells a shopkeeper which build they're running.

```bash
git add .
git commit -m "logo changes"
git push origin main
```

### 3. Check it builds and passes before tagging

```bash
cargo build --release
cargo test
```

### 4. Tag and push

```bash
git tag v0.1.9
git push origin v0.1.9
```

**This is the point of no return for that version number.** CI publishes
a public Release from here. If a tag turns out wrong *before* its release
finishes publishing, delete and recreate it:

```bash
git tag -d v0.1.4 && git push origin :refs/tags/v0.1.4
```

Once a release has published with assets, ship a new version forward
rather than rewriting it.

### 5. Watch the build

[github.com/srotas-space/srotas-desk/actions](https://github.com/srotas-space/srotas-desk/actions)
— three build jobs run in parallel (macos-latest, ubuntu-latest,
windows-latest), then a `release` job attaches all three artifacts. A few
minutes end to end.

### 6. Verify the download links

```bash
curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-macos.zip
curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-linux.tar.gz
curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-windows.zip
```

All three should answer `302` (GitHub redirects release downloads to its
CDN), not `404`.

### Two things that silently break downloads

- **The repo must stay public.** The `/releases/latest/download/` links
  404 for anonymous visitors on a private repo. Check this first if
  downloads suddenly stop working.
- **The three asset filenames are load-bearing.** The downloads page
  (`business/fe/open-source/src/lib/downloads.ts`) hardcodes
  `srotas-desk-macos.zip`, `srotas-desk-linux.tar.gz` and
  `srotas-desk-windows.zip`. Renaming them in the workflow means updating
  `downloads.ts` in the same change, or the buttons silently 404.

The release checklist in [`BUILD.md`](BUILD.md) covers the same ground
with more context.

---

## Data & backups

All data lives in one SQLite file in the OS-standard app-data folder:

| OS | Database |
| --- | --- |
| Ubuntu / Linux | `~/.local/share/srotas-desk/shop.db` |
| macOS | `~/Library/Application Support/srotas-desk/shop.db` |
| Windows | `%APPDATA%\srotas-desk\shop.db` |

It survives restarts, updates and reinstalls of the app itself — the only
real risk is disk failure, which is what the in-app **Backup** screen
(under Inventory) is for. Point it at a pendrive or a folder that syncs
to cloud storage, and it will also back up by itself the first time the
app opens each day.

There is no cloud copy and no account recovery. If that disk dies without
a backup, the data is gone.

---

## Licence

Copyright © 2026 **[Srotas](https://srotas.space)**. All rights reserved.

This source is published so builds can be distributed and the code
inspected — **not** as open source. No licence to copy, modify,
redistribute or create derivative works is granted. See
[`LICENSE`](LICENSE) for the full terms, and
[srotas.space](https://srotas.space) for permissions and licensing
enquiries.

Using the app itself is covered by the licence key issued with it and the
[Terms & Conditions](https://open-source.srotas.space/products/desk/tnc)
accepted on activation.

Third-party components keep their own licences — notably the bundled
Inter typeface (SIL Open Font License 1.1) and the Rust crate
dependencies. [`LICENSE`](LICENSE) lists them.

---

## Further reading

- **[`INSTALL.md`](INSTALL.md)** — installing, first-run setup and
  uninstalling a downloaded release, for end users.
- **[`BUILD.md`](BUILD.md)** — building for Linux, macOS and Windows,
  testing a build you can't run natively, and the release checklist.
- **`demo.sh` / `reset.sh`** — fill a development install with 100,000
  demo items, or wipe it back to a blank slate. Both take `--help`.
