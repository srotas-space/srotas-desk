# Installing / uninstalling Srotas Desk

For end users downloading a release from
`open-source.srotas.space/products/desk/downloads` (or directly from
[GitHub Releases](https://github.com/srotas-space/srotas-desk/releases/latest)).
If you're building from source instead, see `README.md`.

None of these three builds are code-signed (see `DEPLOY.md`'s "Known
gaps"), so each OS shows a first-run warning — that's expected, not a
sign anything's wrong.

## Activation

First launch shows an "Activate Srotas Desk" screen with two fields —
they are **not** the same thing:

- **Device ID** (top, read-only): identifies this specific computer.
  Nothing to do with this field except leave it alone, unless support
  asks for it.
- **License key** (bottom, where you paste something): get this from
  `open-source.srotas.space/products/desk/license` — a single key
  published there works on any machine, so just copy it in and click
  **Activate**. It's not tied to your Device ID.

## macOS

**Install:**

1. Download `srotas-desk-macos.zip` and unzip it (double-click, or
   `unzip srotas-desk-macos.zip`) — this gives you `Srotas Desk.app`.
2. Drag `Srotas Desk.app` into `/Applications`.
3. First launch: right-click the app → **Open** → **Open** again in the
   dialog (a plain double-click gets blocked by Gatekeeper since the app
   isn't signed with an Apple Developer ID). After this first time, it
   opens normally.

**Uninstall:**

```bash
# Quit it first if it's running (Cmd+Q, or):
pkill -f "Srotas Desk"

# Remove the app itself:
rm -rf "/Applications/Srotas Desk.app"

# Remove its data — shop.db lives here. Back it up first if you might
# want it again; this is your actual inventory/billing data, not cache:
rm -rf ~/Library/Application\ Support/srotas-desk

# Remove saved window/UI preferences (safe to delete, no shop data in it):
rm -f ~/Library/Preferences/srotas-desk.plist
```

## Windows

**Install:**

1. Download `srotas-desk-windows.zip` and unzip it — this gives you
   `srotas-desk.exe`.
2. Move the folder wherever you want to keep it (e.g.
   `C:\Program Files\Srotas Desk\`, or just leave it on the Desktop —
   there's no installer, it runs from wherever the `.exe` sits).
3. Double-click `srotas-desk.exe` to run it. **SmartScreen** will likely
   show "Windows protected your PC" the first time (unsigned app, no
   code-signing certificate) — click **More info**, then **Run anyway**.
   Only needed once per machine.

**Uninstall** (there's no installer, so there's nothing in "Add or
Remove Programs" — just delete two things):

```powershell
# Close the app first if it's running, then delete wherever you put it, e.g.:
Remove-Item -Recurse -Force "C:\Program Files\Srotas Desk"

# Remove its data (shop.db) — back it up first if you might want it again:
Remove-Item -Recurse -Force "$env:APPDATA\srotas-desk"
```

## Ubuntu / Linux

**Install:**

1. Download `srotas-desk-linux.tar.gz`, extract it, and run the bundled
   installer (no root needed):

   ```bash
   tar xzf srotas-desk-linux.tar.gz
   cd srotas-desk
   ./install.sh
   ```

2. This copies the binary to `~/.local/bin`, the icon to
   `~/.local/share/icons`, and adds an app-menu entry under
   `~/.local/share/applications`. Make sure `~/.local/bin` is on your
   `PATH` (most distros already add it for you), then launch
   **Srotas Desk** from your application menu, or run `srotas-desk`
   directly. No SmartScreen/Gatekeeper-style warning on Linux.

**Uninstall:**

```bash
# Quit it first if it's running:
pkill -f srotas-desk

# Remove the binary, icon, and app-menu entry install.sh created:
rm -f ~/.local/bin/srotas-desk
rm -f ~/.local/share/icons/srotas-desk.png
rm -f ~/.local/share/applications/srotas-desk.desktop

# Remove its data (shop.db) — back it up first if you might want it again:
rm -rf ~/.local/share/srotas-desk
```
