#!/usr/bin/env bash
# Cross-builds the Linux binary from macOS (or any non-Linux host) via
# Docker — no Linux machine needed. Builds inside the Ubuntu 22.04
# container defined by Dockerfile (same apt packages the ubuntu-latest CI
# job in .github/workflows/release.yml installs), then hands off to
# package.sh for the same desktop-file/icon/tarball assembly CI uses —
# see package.sh's own comment for why building directly on macOS instead
# of through here silently produces a broken (macOS) "Linux" package.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
  echo "Docker isn't installed/running. Start Docker Desktop and try again."
  exit 1
fi

TARGET=x86_64-unknown-linux-gnu
IMAGE=srotas-desk-linux-builder

# --platform linux/amd64 is required on Apple Silicon: Docker Desktop
# otherwise pulls/builds the arm64 variant of the base image, whose
# rustup default target is aarch64-unknown-linux-gnu — cross-compiling
# from there to x86_64 needs its own gcc, which isn't installed. Running
# an amd64 container directly (via QEMU emulation under the hood) sidesteps
# that; it's slower than a native arm64 build, but this only needs to run
# occasionally for a release, not on every change.
docker build --platform linux/amd64 -t "$IMAGE" -f packaging/linux/Dockerfile .

# Explicit --target (even though it's the container's native arch here)
# puts the build under target/x86_64-unknown-linux-gnu/release/ instead of
# target/release/ — keeping it out of the way of a macOS host build in
# the same target/ directory, which would otherwise fight over that path
# every time you switch between building here and building natively.
docker run --rm --platform linux/amd64 -v "$PWD":/work -w /work "$IMAGE" \
  cargo build --release --target "$TARGET"

SROTAS_DESK_BIN="target/$TARGET/release/srotas-desk" ./packaging/linux/package.sh
