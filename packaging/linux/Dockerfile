# Build environment for cross-building the Linux binary from a non-Linux
# host (see package-docker.sh). Ubuntu 22.04, not the newest LTS, is
# deliberate: linking against an older glibc means the binary also runs on
# newer distros, not just this exact version — the reverse isn't true.
# Package list mirrors the ubuntu-latest job in
# .github/workflows/release.yml so a local Docker build and CI produce
# equivalent binaries.
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential pkg-config \
    libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
    libxcursor-dev libgl1-mesa-dev libfontconfig1-dev libgtk-3-dev \
    libxkbcommon-x11-0 \
    && rm -rf /var/lib/apt/lists/*
# libxkbcommon-x11-0 (note: no -dev, it's not needed to build — winit's
# xkbcommon-dl crate dlopen()s libxkbcommon-x11.so at runtime instead of
# linking it) is here so this image can also *run* the built binary for
# local X11-forwarded testing (see the docker run example in INSTALL.md),
# not because compiling needs it. A real end user's Ubuntu Desktop
# normally already has this as part of its X11/Xkb stack; a bare/minimal
# install might not.

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add x86_64-unknown-linux-gnu

WORKDIR /work
