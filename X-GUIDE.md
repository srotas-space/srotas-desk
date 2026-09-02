 cd /Users/snmmaurya/ws/xss/gui/srotas-desk

  git add src/ui/items.rs src/ui/mod.rs

git commit -m "$(cat <<'EOF'
  Add Stock/Buy/Sell stat tiles to item detail; fix Tab focus navigation
  
  Item detail screen now uses the same stat-tile treatment as the Sell
  screen's Stock/Buy/Sell panel instead of plain label:value rows.

  Tab/Shift+Tab now move focus between form fields app-wide via
  iced::widget::operation::focus_next/previous — text_input doesn't do
  this on its own, unlike a browser's native tab order.
  EOF
)"

  git push origin main

  If you want this in an actual downloadable release afterward:

  # bump version = "0.1.3" in Cargo.toml first, then:
  cargo check          # regenerates Cargo.lock for the version bump
  git add Cargo.toml Cargo.lock
  git commit -m "Bump to 0.1.3"
  git push origin main

  git tag -a v0.1.3 -m "v0.1.3"
  git push origin v0.1.3




## Ubuntu — two options

### Option A: Docker + XQuartz (lightest — no VM, reuses what's already set up)

You already have packaging/linux/package-docker.sh's Ubuntu 22.04 image with GTK/X11 libs installed. Point its display back at macOS:

# One-time: allow local X11 connections
open -a XQuartz
xhost + 127.0.0.1

# Run the actual app with its window forwarded to XQuartz
docker run --rm --platform linux/amd64 \
-e DISPLAY=host.docker.internal:0 \
-v "$PWD/dist/linux/srotas-desk":/app \
srotas-desk-linux-builder \
/app/srotas-desk

Caveat: this shares the kernel with macOS and has no real GPU access — iced falls back to CPU rendering (tiny-skia), so it'll be slower and isn't a perfect stand-in for a real user's machine. Good for "does
it launch, does the layout look right, can I click through it" — not for performance testing.

Option B: UTM VM (real Ubuntu, most representative)

brew install --cask utm
Then in UTM: Create a New VM → Virtualize → Linux → download Ubuntu Server ARM64 (not Desktop — install just a minimal desktop on top with sudo apt install ubuntu-desktop-minimal for something light) → 4GB
RAM / 20GB disk is plenty. Since it's ARM64, build a native ARM64 Linux binary to test in it (drop --platform linux/amd64 from the Dockerfile build, or just cargo build --release directly if you ever get a
native Linux ARM box) — testing UI/UX doesn't need it to be the same arch as what you ship.

Windows — two options

Option A: UTM + free Windows 11 ARM dev VM (real fidelity)

Microsoft publishes a free, pre-activated Windows 11 ARM64 dev VM (time-limited, periodically refreshed) at aka.ms/windev_VM — for UTM specifically choose the UTM (Apple Silicon) format. Your .exe is x86_64
(built via mingw-w64), and Windows-on-ARM has built-in x86_64 emulation, so it should just run.

Option B: Wine (lightest, but genuinely risky for this app)

brew install --cask wine-stable
wine "srotas-desk.exe"
Fast to try, but iced's GPU rendering under Wine's translated graphics stack is a coin flip — it may not render at all. Worth a 2-minute try, not worth trusting as a real signal either way.

My honest recommendation: UTM for both (one tool, free, real OS, actually reliable) — Docker+XQuartz and Wine are worth trying first since they're nearly free to attempt, but treat them as "quick smoke
test," not a substitute for the real thing.