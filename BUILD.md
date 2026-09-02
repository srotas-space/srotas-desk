# Building Srotas Desk

How to build the app for Ubuntu/Linux, macOS and Windows — both a plain
binary for development and the packaged artefact each OS expects.

If you only want to *use* the app, don't build it: download a release and
follow `INSTALL.md`.

---

## Contents

- [Before anything](#before-anything) — toolchain and per-OS dependencies
- [Running from source](#running-from-source) — `cargo run`
- [Building for Ubuntu / Linux](#building-for-ubuntu--linux)
- [Building for macOS](#building-for-macos)
- [Building for Windows](#building-for-windows)
- [Which host can build what](#which-host-can-build-what)
- [Testing a build you can't run natively](#testing-a-build-you-cant-run-natively)
- [Cutting a release](#cutting-a-release) — the tag-and-ship runbook
- [Known gaps](#known-gaps)

---

## Before anything

Every platform needs the [Rust](https://www.rust-lang.org/tools/install)
stable toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # macOS / Linux
```

On Windows, install [rustup-init.exe](https://rustup.rs) and pick the
default (MSVC) toolchain when it asks.

**Linux also needs GUI development headers.** macOS and Windows need
nothing beyond Rust itself.

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config \
  libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
  libxcursor-dev libgl1-mesa-dev libfontconfig1-dev libgtk-3-dev
```

These are the same packages the `ubuntu-latest` job in
`.github/workflows/release.yml` installs, so a local build and CI build
the same thing. On Fedora/RHEL the equivalents are `libxkbcommon-devel`,
`wayland-devel`, `libX11-devel`, `libXrandr-devel`, `libXi-devel`,
`libXcursor-devel`, `mesa-libGL-devel`, `fontconfig-devel`, `gtk3-devel`.

Check it worked:

```bash
cargo build          # should finish with no warnings from this crate
cargo test           # 65 tests, all green
```

`cargo build` prints one warning that isn't from this project — see
[Known gaps](#known-gaps).

---

## Running from source

```bash
cargo run
```

Good enough for day-to-day work; no packaging needed. The app creates its
database on first launch and then asks to be activated — see
`INSTALL.md`'s activation section.

Two helper scripts make a development install easier to work with:

```bash
./demo.sh            # fill the catalogue with 100,000 demo items
./reset.sh           # wipe the install back to a blank slate
```

Both take `--help`.

---

## Building for Ubuntu / Linux

Produces `dist/linux/srotas-desk-linux.tar.gz` — the binary, a `.desktop`
entry, the icon, and an `install.sh` that puts them where a desktop
environment will find them.

**On a Linux machine:**

```bash
./packaging/linux/package.sh
```

**From macOS (or any non-Linux host), via Docker:**

```bash
./packaging/linux/package-docker.sh
```

This builds inside an Ubuntu 22.04 container and then hands off to
`package.sh` for the same assembly. Takes 5–10 minutes on Apple Silicon,
because the container runs under x86-64 emulation.

> **Don't run `packaging/linux/package.sh` directly on macOS.** It calls
> `cargo build --release`, which targets whatever OS it runs on — so you
> get a macOS binary inside a Linux-shaped tarball. It looks fine right up
> until somebody on Ubuntu tries to run it. Use `package-docker.sh`.

Ubuntu 22.04 is deliberate rather than the newest LTS: linking against an
older glibc means the binary also runs on newer distributions. The
reverse isn't true.

**Test the tarball:**

```bash
tar xzf dist/linux/srotas-desk-linux.tar.gz -C /tmp
/tmp/srotas-desk/srotas-desk
```

---

## Building for macOS

Produces `dist/macos/Srotas Desk.app` — a real bundle, so macOS shows the
app's own icon in the Dock and Finder rather than the generic "exec" icon
a bare Unix binary gets.

**Must run on a Mac.** Cross-compiling to macOS from Linux or Windows
needs Apple's SDK and isn't set up here.

```bash
./packaging/macos/package.sh
open "dist/macos/Srotas Desk.app"
```

The script ad-hoc signs the bundle (`codesign -s -`). That is free and
local — not real code-signing — but it matters: without *any* signature,
Gatekeeper on Apple Silicon refuses to launch a downloaded bundle and
says it "is damaged and can't be opened", which reads like file
corruption rather than the missing Developer ID it actually is. Ad-hoc
signing downgrades that to the ordinary "unidentified developer" warning.

To hand the `.app` to someone else, zip it:

```bash
(cd dist/macos && zip -r -X srotas-desk-macos.zip "Srotas Desk.app")
```

---

## Building for Windows

Produces `dist/windows/srotas-desk-windows.zip` containing
`srotas-desk.exe`. There's no installer — the `.exe` runs from wherever
it sits. The icon is compiled into the executable by `build.rs` from
`assets/icon.ico`.

**On a Windows machine** (PowerShell, Rust installed):

```powershell
.\packaging\windows\package.ps1
```

**From macOS or Linux**, cross-compiled with mingw-w64:

```bash
brew install mingw-w64            # macOS
sudo apt-get install mingw-w64    # Debian/Ubuntu

./packaging/windows/package-cross.sh
```

The script adds the `x86_64-pc-windows-gnu` target itself and points
cargo at the mingw linker. Note this produces a **GNU**-toolchain
executable, whereas building on Windows produces an **MSVC** one; both
run fine, and CI ships the MSVC build.

The output is x86-64. It runs on ARM Windows through that OS's built-in
x64 emulation.

---

## Which host can build what

| Build for →<br>On ↓ | Ubuntu / Linux | macOS | Windows |
| --- | --- | --- | --- |
| **macOS** | ✅ via Docker (`package-docker.sh`) | ✅ native | ✅ via mingw-w64 (`package-cross.sh`) |
| **Ubuntu / Linux** | ✅ native | ❌ needs Apple's SDK | ✅ via mingw-w64 (`package-cross.sh`) |
| **Windows** | ✅ via WSL2 or Docker | ❌ needs Apple's SDK | ✅ native (`package.ps1`) |

CI sidesteps the whole table: `.github/workflows/release.yml` builds each
platform **natively** on its own GitHub runner. Cross-compiling is only
for building and testing locally.

---

## Testing a build you can't run natively

A green `cargo build` proves the binary compiles. It says nothing about
whether the window opens.

### macOS

Nothing special — `cargo run`, or test the real bundle:

```bash
./packaging/macos/package.sh
open "dist/macos/Srotas Desk.app"
```

To exercise a first-run install without touching your own shop data,
point the app at a throwaway home directory (it keeps everything under
`$HOME/Library/Application Support/srotas-desk`):

```bash
HOME=/tmp/desk-test "dist/macos/Srotas Desk.app/Contents/MacOS/srotas-desk"
```

That gets you the activation screen on a clean database. `./reset.sh`
does the same for your real install.

### Ubuntu / Linux, from a Mac — Docker + XQuartz

This runs the actual Linux binary and forwards its window to your Mac.
One-time setup:

```bash
brew install --cask xquartz
open -a XQuartz
# XQuartz → Preferences → Security → "Allow connections from network
# clients" — on some XQuartz versions that checkbox writes to the WRONG
# preference domain, so set it directly instead, then restart:
defaults write org.xquartz.X11 nolisten_tcp -bool false
osascript -e 'tell application "XQuartz" to quit'; open -a XQuartz
```

Build a Linux binary **for your Mac's own architecture** — no `--platform`
flag, so no QEMU emulation. Minutes rather than tens of minutes, and it
exercises the same Linux code paths:

```bash
docker build -t srotas-desk-linux-arm64 -f packaging/linux/Dockerfile .
docker run --rm -v "$PWD":/work -w /work srotas-desk-linux-arm64 \
  cargo build --release --target aarch64-unknown-linux-gnu
```

(On an Intel Mac, use `x86_64-unknown-linux-gnu` instead.)

Then run it with the window forwarded:

```bash
xhost + 127.0.0.1
docker run --rm \
  -e DISPLAY=host.docker.internal:0 \
  -e ICED_BACKEND=tiny-skia \
  -e XDG_RUNTIME_DIR=/tmp/runtime \
  -v "$PWD/target/aarch64-unknown-linux-gnu/release/srotas-desk":/app/srotas-desk:ro \
  srotas-desk-linux-arm64 \
  sh -c 'mkdir -p /tmp/runtime && chmod 700 /tmp/runtime && exec /app/srotas-desk'
```

> **`ICED_BACKEND=tiny-skia` is not optional here.** By default iced
> renders through wgpu, which needs OpenGL, and XQuartz's GLX can't supply
> a usable framebuffer config to a container — you get
> `libGL error: No matching fbConfigs or visuals found` followed by a
> process that runs forever without ever showing a window, which looks
> exactly like a hang. `tiny-skia` is iced's software renderer, it's in
> the default feature set so it's already compiled into the binary, and it
> draws straight to X11 with no GL at all. Slower to redraw; irrelevant
> for checking that screens render and buttons work.

Two things this does **not** test: GPU rendering (you're on the software
path), and the exact x86-64 artifact CI ships. For those, use a VM.

Nothing here depends on what fonts the host has: the app bundles Inter
(`assets/fonts/`) and sets it as the default, and the Home tiles use
embedded SVGs (`assets/store.svg`, `assets/inventory.svg`) rather than
emoji. So a bare container renders the same as a full desktop.

### Ubuntu, from a Mac — a UTM virtual machine

Higher fidelity and worth the setup if you're testing the packaged
release rather than iterating on code: a real desktop, a real app menu, a
real GTK file picker for the backup folder, and the `install.sh` path end
to end.

1. Install [UTM](https://mac.getutm.app) (free) and download the
   **Ubuntu Desktop ARM64** ISO from `ubuntu.com/download/desktop/arm`.
2. New VM → Virtualize → Linux → the ISO. 4 CPUs, 8 GB RAM and 25 GB disk
   is comfortable on a 16 GB+ Mac.
3. Inside the VM, build the arm64 tarball with
   `./packaging/linux/package.sh`, or copy one in — then
   `tar xzf … && ./install.sh` and launch it from the app menu.

Budget ~25 GB of disk and about half an hour for the install.

### Windows, from a Mac — no lightweight option works

All of these were tried and ruled out:

- `wine-stable`, Whisky, `wine-crossover` — deprecated or removed from
  Homebrew as of 2026 (unsigned/unmaintained).
- Wine *inside* the Linux Docker container above, which sidesteps macOS
  Gatekeeper entirely since it would be a Linux binary — installs fine,
  then dies on launch with `anon_mmap_fixed: Assertion failed` and
  `qemu: uncaught target signal 6`. That's a real, documented
  incompatibility between Wine's virtual-memory handling and QEMU's
  x86-on-ARM user-mode emulation, not a misconfiguration. Don't re-attempt
  it without a different emulation layer.

The only reliable option is a real Windows VM — UTM plus a Windows-on-ARM
image (Microsoft's Insider Preview VHDX; their official free dev VMs are
x64-only formats). The `.exe` is x86-64 and runs under that OS's built-in
x64 emulation.

## Cutting a release

### How it fits together

1. `srotas-space/srotas-desk` on GitHub **must stay public** — the
   downloads page links straight at
   `github.com/srotas-space/srotas-desk/releases/latest/download/<asset>`,
   which 404s for anonymous visitors on a private repo. Check this first
   if downloads suddenly stop working; someone may have flipped
   visibility back.
2. Pushing a tag matching `v*` triggers `.github/workflows/release.yml`,
   which builds all three platforms natively and publishes them as
   assets on a GitHub Release.
3. `/releases/latest/download/<asset>` always resolves to the newest
   release, so the downloads page
   (`business/fe/open-source/src/lib/downloads.ts`) never changes when a
   new version ships — only this repo needs a tag.
4. The three asset filenames are load-bearing: `downloads.ts` hardcodes
   `srotas-desk-macos.zip`, `srotas-desk-linux.tar.gz` and
   `srotas-desk-windows.zip`. Renaming them in the workflow means
   updating `downloads.ts` in the same change, or the buttons silently
   404.

### Checklist

1. **Bump the version** in `Cargo.toml`. Commit it before tagging.

2. **Check it builds and passes:**

   ```bash
   cargo build --release
   cargo test
   ```

3. **(Optional) Smoke-test packaging locally.** Not required — CI builds
   for real on each OS — but it catches a broken packaging script before
   you've published a release with a broken asset:

   ```bash
   ./packaging/macos/package.sh
   ./packaging/linux/package-docker.sh
   ./packaging/windows/package-cross.sh
   ```

4. **Tag and push:**

   ```bash
   git tag v0.x.y
   git push origin v0.x.y
   ```

   Use the version actually in `Cargo.toml` — don't guess. If a tag is
   wrong *before* its release publishes, delete and recreate it; once a
   release has published with assets, ship forward instead of rewriting.
   This is the point of no return for that version number.

5. **Watch the run** at `github.com/srotas-space/srotas-desk/actions` —
   three build jobs in parallel, then a `release` job that attaches all
   three artifacts. A few minutes end to end.

6. **Verify the assets exist:**

   ```bash
   curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-macos.zip
   curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-linux.tar.gz
   curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-windows.zip
   ```

   All three should answer `302` (GitHub redirects release downloads to
   its CDN), not `404`. For a real install-and-launch check rather than
   just "the file exists", follow `INSTALL.md`.

7. **Verify the downloads page** — `open-source.srotas.space/products/desk/downloads`.
   Nothing to redeploy on the website; it links at `/releases/latest/...`,
   which now resolves.

---

## Known gaps

- **One warning isn't ours.** `cargo build` prints:

  ```
  warning: the following packages contain code that will be rejected by a
  future version of Rust: block v0.1.6
  ```

  `block` arrives four levels down (`metal` → `wgpu-hal` → `wgpu` →
  `iced`) and is pinned by iced 0.14's wgpu version, so there's nothing
  to fix here — it clears when iced updates its graphics stack. Run
  `cargo report future-incompatibilities --id 1` for the detail. If the
  line genuinely gets in your way, a project-local
  `.cargo/config.toml` with `[future-incompat-report]` and
  `frequency = "never"` silences it — at the cost of also hiding the same
  report for this crate's own code later, which is why it isn't checked
  in. This project's own code compiles clean.

- **Not code-signed.** macOS shows a Gatekeeper "unidentified developer"
  warning (right-click → Open bypasses it); Windows shows a SmartScreen
  "unknown publisher" warning (More info → Run anyway). Fixing this
  properly needs a paid Apple Developer ID ($99/yr, for notarization) and
  a Windows code-signing certificate — infrastructure decisions, not
  something to add quietly.

- **No auto-update.** Each version is a fresh manual download; the app
  never checks for or fetches updates.
