#!/usr/bin/env bash
# Builds a release binary and packages it with a .desktop entry + icon so
# a Linux desktop environment (GNOME, KDE, etc.) shows the real logo and
# an app-menu entry, instead of the binary just being a bare executable.
#
# Must run on an actual Linux host (or under package-docker.sh, which runs
# it inside one) — `cargo build --release` here just builds for whatever
# platform it's invoked on. Running this directly on macOS silently
# produces a macOS Mach-O binary packaged with a Linux-shaped name around
# it; it looks fine until someone on Ubuntu tries to run it. Set
# SROTAS_DESK_BIN to skip the build and package an already-built binary
# instead (what package-docker.sh does, since the build already happened
# inside the container).
set -euo pipefail
cd "$(dirname "$0")/../.."

BIN="${SROTAS_DESK_BIN:-}"
if [ -z "$BIN" ]; then
  cargo build --release
  BIN="target/release/srotas-desk"
fi

OUT="dist/linux/srotas-desk"
rm -rf "dist/linux"
mkdir -p "$OUT"

cp "$BIN" "$OUT/srotas-desk"
cp packaging/linux/srotas-desk.desktop "$OUT/srotas-desk.desktop"
cp assets/icon.png "$OUT/srotas-desk.png"

cat > "$OUT/install.sh" <<'EOF'
#!/usr/bin/env bash
# Installs Srotas Desk for the current user (no root needed).
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons"
cp srotas-desk "$HOME/.local/bin/srotas-desk"
chmod +x "$HOME/.local/bin/srotas-desk"
cp srotas-desk.png "$HOME/.local/share/icons/srotas-desk.png"
sed "s#Icon=srotas-desk#Icon=$HOME/.local/share/icons/srotas-desk.png#" srotas-desk.desktop \
  > "$HOME/.local/share/applications/srotas-desk.desktop"
echo "Installed. Make sure \$HOME/.local/bin is on your PATH, then launch"
echo "'Srotas Desk' from your application menu (or run: srotas-desk)."
EOF
chmod +x "$OUT/install.sh"

tar -C dist/linux -czf dist/linux/srotas-desk-linux.tar.gz srotas-desk
echo "Built: dist/linux/srotas-desk-linux.tar.gz"
