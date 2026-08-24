#!/usr/bin/env bash
# Cross-compiles the Windows .exe from macOS or Linux — no Windows machine
# needed. Uses the mingw-w64 toolchain to target x86_64-pc-windows-gnu.
# The icon is embedded into the .exe via build.rs (assets/icon.ico).
set -euo pipefail
cd "$(dirname "$0")/../.."

TARGET=x86_64-pc-windows-gnu

if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  echo "mingw-w64 not found. Install it first:"
  if command -v brew >/dev/null 2>&1; then
    echo "  brew install mingw-w64"
  else
    echo "  sudo apt-get install mingw-w64   # Debian/Ubuntu"
  fi
  exit 1
fi

rustup target add "$TARGET"

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar

cargo build --release --target "$TARGET"

mkdir -p dist/windows
cp "target/$TARGET/release/srotas-desk.exe" dist/windows/srotas-desk.exe
(cd dist/windows && zip -r -X srotas-desk-windows.zip srotas-desk.exe)

echo "Built: dist/windows/srotas-desk-windows.zip"
