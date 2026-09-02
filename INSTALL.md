# Installing Srotas Desk

For anyone downloading a release from
`open-source.srotas.space/products/desk/downloads` (or directly from
[GitHub Releases](https://github.com/srotas-space/srotas-desk/releases/latest)).
Building it yourself instead? See `BUILD.md`.

Pick your operating system:

- [Ubuntu / Linux](#ubuntu--linux)
- [macOS](#macos)
- [Windows](#windows)

Then work through [First run](#first-run) — activation, shop setup and the
counter PIN — and read [Where your data lives](#where-your-data-lives)
before you rely on it for real.

**None of the three builds is code-signed**, so macOS and Windows each
show a one-time warning on first launch. That's expected, not a sign that
anything is wrong — see `BUILD.md`'s "Known gaps" for why.

## What you need

- **Ubuntu / Linux**: a 64-bit x86 desktop (Ubuntu 22.04 or newer, or
  any distribution of similar vintage). No root needed.
- **macOS**: macOS 11 Big Sur or newer, Intel or Apple Silicon.
- **Windows**: Windows 10 or 11, 64-bit. Works on ARM devices through
  Windows' built-in x64 emulation.

About 60 MB of disk for the app, plus your shop's data — a catalogue of a
few thousand items is only a few megabytes.

---

## Ubuntu / Linux

**Install** — the tarball ships an installer that needs no root:

```bash
tar xzf srotas-desk-linux.tar.gz
cd srotas-desk
./install.sh
```

That copies the binary to `~/.local/bin`, the icon to
`~/.local/share/icons`, and an app-menu entry to
`~/.local/share/applications`. Make sure `~/.local/bin` is on your `PATH`
(most distributions add it already), then launch **Srotas Desk** from
your application menu, or run `srotas-desk`. No Gatekeeper- or
SmartScreen-style warning on Linux.

If it won't start, you're most likely missing an X/Wayland runtime
library that a minimal or server install skips:

```bash
sudo apt-get install -y libxkbcommon-x11-0 libgtk-3-0 libgl1
```

The app bundles its own typeface and its own icons, so it looks the same
here as on macOS and Windows — nothing to install for fonts or emoji.

**Uninstall:**

```bash
pkill -f srotas-desk                              # quit it first

rm -f ~/.local/bin/srotas-desk
rm -f ~/.local/share/icons/srotas-desk.png
rm -f ~/.local/share/applications/srotas-desk.desktop

# Your shop's data. Back it up first if you might want it again.
rm -rf ~/.local/share/srotas-desk
```

---

## macOS

**Install:**

1. Download `srotas-desk-macos.zip` and unzip it (double-click, or
   `unzip srotas-desk-macos.zip`) — you get `Srotas Desk.app`.
2. Drag `Srotas Desk.app` into `/Applications`.
3. **First launch only:** right-click the app → **Open** → **Open** again
   in the dialog. A plain double-click gets blocked, because the app
   isn't signed with an Apple Developer ID. After that once, it opens
   normally.

If macOS says the app "is damaged and can't be opened", the download was
truncated or the quarantine flag is confusing it — re-download, and if it
persists:

```bash
xattr -dr com.apple.quarantine "/Applications/Srotas Desk.app"
```

**Uninstall:**

```bash
pkill -f "Srotas Desk"                            # quit it first (or Cmd+Q)

rm -rf "/Applications/Srotas Desk.app"

# Your shop's data. Back it up first if you might want it again.
rm -rf ~/Library/Application\ Support/srotas-desk

# Window/UI preferences — no shop data in here, safe to delete.
rm -f ~/Library/Preferences/srotas-desk.plist
```

---

## Windows

**Install:**

1. Download `srotas-desk-windows.zip` and unzip it — you get
   `srotas-desk.exe`.
2. Move it wherever you want to keep it (`C:\Program Files\Srotas Desk\`,
   or just the Desktop). There is no installer; it runs from wherever the
   `.exe` sits.
3. Double-click `srotas-desk.exe`. **SmartScreen** will most likely say
   "Windows protected your PC" the first time, because the app has no
   code-signing certificate — click **More info**, then **Run anyway**.
   Once per machine.

To get it into the Start menu, right-click the `.exe` → **Show more
options** → **Pin to Start**.

**Uninstall** — there's no installer, so nothing appears in "Add or
remove programs". Delete two things:

```powershell
# Close the app first, then delete wherever you put it:
Remove-Item -Recurse -Force "C:\Program Files\Srotas Desk"

# Your shop's data. Back it up first if you might want it again.
Remove-Item -Recurse -Force "$env:APPDATA\srotas-desk"
```

---

## First run

### 1. Activation

The first screen is **Activate Srotas Desk**, with two fields that are
*not* the same thing:

- **Device ID** (top, read-only) — identifies this particular computer.
  Leave it alone unless support asks you for it; there's a **Copy**
  button if they do.
- **License key** (bottom, the one you paste into) — get it from
  `open-source.srotas.space/products/desk/license`. A single key
  published there activates on any machine, so copy it straight in.

Tick **I agree to the Terms & Conditions** — the **Activate** button
stays greyed out until you do — then click **Activate**.

Keep that licence key somewhere you can find it again. It is also what
resets a forgotten counter PIN (below).

### 2. Set up your shop

Next comes a one-time form: shop name (required), owner, phone, address,
and optionally a screen-lock PIN. Everything here goes onto your printed
bills, so it's worth getting right — you can change it later under
**Settings → Profile**, along with your GSTIN, default GST rate and shop
logo.

### 3. The counter PIN (optional)

A 4–6 digit PIN locks the screen when you step away from the counter.
Set one during setup, or later under **Settings → Security**.

- The **Lock** button in the top-right locks the screen on demand.
- The PIN is stored as a one-way hash, so it stays unreadable even in a
  backup copy of your database.
- After 5 wrong attempts the lock screen pauses for 30 seconds, doubling
  with each further wrong attempt, up to 15 minutes. Quitting and
  reopening the app does not clear that.
- **Forgot it?** The lock screen has a **Forgot PIN?** link: paste your
  licence key to prove the shop is yours, then set a new PIN (or leave
  both boxes blank to remove the lock entirely).

---

## Where your data lives

Everything — items, stock, bills, your shop profile — is in **one SQLite
file** on this computer. Nothing is sent anywhere.

| OS | Database |
| --- | --- |
| Ubuntu / Linux | `~/.local/share/srotas-desk/shop.db` |
| macOS | `~/Library/Application Support/srotas-desk/shop.db` |
| Windows | `%APPDATA%\srotas-desk\shop.db` |

**Set up backups before you start using it seriously.** Under
**Inventory → Backup**, choose a folder on a pendrive or in a synced
folder (Google Drive, Dropbox). Once a folder is chosen, the app also
backs up by itself the first time you open it each day.

A backup is a plain copy of `shop.db`. To restore one, quit the app and
copy the backup file over the path in the table above.

There is no cloud copy and no account recovery. If that disk dies without
a backup, the data is gone.

---

## Upgrading

Download the new release and install it over the old one — the same steps
as above. Your data isn't touched: the app keeps it in the folder listed
above, separate from the program itself, and brings the database up to
date on first launch.

The app doesn't check for updates on its own, so check the downloads page
now and then.

Take a backup before upgrading anyway. It costs a moment.

---

© 2026 [Srotas](https://srotas.space). All rights reserved. Use of this
app is governed by the licence key issued with it and the
[Terms & Conditions](https://open-source.srotas.space/products/desk/tnc)
accepted on activation.
