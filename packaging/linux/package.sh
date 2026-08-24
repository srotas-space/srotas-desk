#!/usr/bin/env bash
# Builds a release binary and packages it with a .desktop entry + icon so
# a Linux desktop environment (GNOME, KDE, etc.) shows the real logo and
# an app-menu entry, instead of the binary just being a bare executable.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build --release

OUT="dist/linux/srotas-desk"
rm -rf "dist/linux"
mkdir -p "$OUT"

cp target/release/srotas-desk "$OUT/srotas-desk"
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
