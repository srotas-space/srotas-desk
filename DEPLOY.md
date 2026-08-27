# Deploying a Srotas Desk release

This is the runbook for shipping a new version — the sequence to run, not
just what each script does (see `README.md`'s "Deploying a real build"
section for that, and for testing an individual platform's package
locally before you tag anything).

## How it fits together

1. `srotas-space/srotas-desk` on GitHub **must stay public** — the
   downloads page links directly at
   `github.com/srotas-space/srotas-desk/releases/latest/download/<asset>`,
   which 404s for anonymous visitors on a private repo. Check this first
   if downloads suddenly stop working; someone may have flipped
   visibility back to private.
2. Pushing a tag matching `v*` triggers `.github/workflows/release.yml`,
   which builds macOS, Ubuntu, and Windows **natively** on GitHub's own
   runners (no cross-compilation in CI — that's only for building/testing
   locally from this Mac, see README) and publishes all three as assets
   on a GitHub Release.
3. `/releases/latest/download/<asset>` always resolves to the newest
   release, so the downloads page
   (`business/fe/open-source/src/lib/downloads.ts`) never needs to change
   when a new version ships — only this repo needs a new tag.
4. The three asset filenames are load-bearing: `downloads.ts` hardcodes
   `srotas-desk-macos.zip`, `srotas-desk-linux.tar.gz`,
   `srotas-desk-windows.zip`. If you ever rename them in
   `.github/workflows/release.yml`, update `downloads.ts` in the same
   change, or the download buttons silently 404.

## Release checklist

1. **Bump the version** in `Cargo.toml` (`version = "0.x.y"`) if this
   isn't the first release. Commit that on its own or with your other
   changes — just make sure it lands before tagging.

2. **Sanity-check the build locally** before tagging anything public:

   ```bash
   cargo build --release
   cargo test
   ```

3. **(Optional) Smoke-test packaging locally** — not required (CI builds
   for real on each OS), but catches a broken packaging script before
   you've published a release with a broken asset:

   ```bash
   ./packaging/macos/package.sh          # this Mac, native
   ./packaging/linux/package-docker.sh   # via Docker, ~5-10 min under emulation
   ./packaging/windows/package-cross.sh  # via mingw-w64, needs: brew install mingw-w64
   ```

4. **Tag and push**:

   ```bash
   git tag v0.x.y
   git push origin v0.x.y
   ```

   Use the version that's actually in `Cargo.toml` — don't reuse or
   guess a number. If a tag turns out wrong before its release
   publishes, delete and recreate it; once a release has actually
   published (has assets), ship a new version forward instead of
   rewriting it.

   This is the point of no return for this version number — CI publishes
   a public GitHub Release from here. Double-check the tag matches
   `Cargo.toml`'s version before pushing it.

5. **Watch the CI run**: `github.com/srotas-space/srotas-desk/actions` —
   the `Release` workflow, matrix job for the pushed tag. Three build
   jobs run in parallel (macos-latest, ubuntu-latest, windows-latest),
   then a `release` job downloads all three artifacts and attaches them
   to a new GitHub Release. Takes a few minutes end to end.

6. **Verify the release** once CI finishes:

   ```bash
   curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-macos.zip
   curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-linux.tar.gz
   curl -sI https://github.com/srotas-space/srotas-desk/releases/latest/download/srotas-desk-windows.zip
   ```

   All three should respond `302` (GitHub redirects release-asset
   downloads to its CDN) rather than `404`. For a full install-and-launch
   check (not just that the file exists), see `INSTALL.md`.

7. **Verify the downloads page** picks it up —
   `open-source.srotas.space/products/desk/downloads` — the three buttons
   should download real files. Nothing to redeploy on the website's side;
   it links at `/releases/latest/...`, which now resolves.

## Known gaps (not yet solved)

- **Not code-signed.** macOS shows a Gatekeeper "unidentified developer"
  warning (right-click → Open bypasses it); Windows shows a SmartScreen
  "unknown publisher" warning ("More info" → "Run anyway" bypasses it).
  Fixing this for real needs a paid Apple Developer ID ($99/yr, for
  notarization) and a Windows code-signing certificate — infrastructure
  decisions, not something to add silently.
- **No auto-update.** Each new version is a fresh manual download; the
  app doesn't check for or fetch updates itself.
