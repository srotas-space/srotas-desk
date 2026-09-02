#!/usr/bin/env bash
#
# Wipes this machine's Srotas Desk install back to a blank slate: the shop
# database (items, stock history, bills, shop profile), the remembered
# backup settings, and the licence with it.
#
# Removing the licence means removing this machine's device id too, so the
# next launch generates a fresh one and asks to be activated again — and a
# key issued for the old device id will no longer match. Pass
# --keep-licence if you only meant to clear the shop's data.
#
# Backups you have already taken (the .db files on a pendrive or in a
# synced folder) are somewhere else entirely and are never touched.
#
#   ./reset.sh                  # wipe everything, licence included
#   ./reset.sh --keep-licence   # wipe the shop's data, stay activated
#   ./reset.sh -y               # skip the confirmation
#
set -euo pipefail

# Copyright (c) 2026 Srotas — https://srotas.space
# All rights reserved. See LICENSE.

ASSUME_YES=0
KEEP_LICENCE=0
DATA_DIR=""

die() {
	printf 'reset.sh: %s\n' "$1" >&2
	exit 1
}

usage() {
	# Everything from the shebang down to the first line of actual code,
	# with the comment markers stripped — so the help can't drift out of
	# step with the header the way a hard-coded line range does.
	sed -n '2,/^[^#]/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
	exit 0
}

while [ $# -gt 0 ]; do
	case "$1" in
	-y | --yes)
		ASSUME_YES=1
		shift
		;;
	--keep-licence | --keep-license)
		KEEP_LICENCE=1
		shift
		;;
	--data-dir)
		DATA_DIR="${2:-}"
		shift 2
		;;
	-h | --help) usage ;;
	*) die "unknown option '$1' (try --help)" ;;
	esac
done

# Mirrors `dirs::data_dir()` in src/db.rs — keep the two in step.
if [ -z "$DATA_DIR" ]; then
	case "$(uname -s)" in
	Darwin) DATA_DIR="$HOME/Library/Application Support/srotas-desk" ;;
	Linux) DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/srotas-desk" ;;
	MINGW* | MSYS* | CYGWIN*) DATA_DIR="${APPDATA:-$HOME/AppData/Roaming}/srotas-desk" ;;
	*) die "unsupported platform '$(uname -s)' — pass --data-dir explicitly" ;;
	esac
fi

DB="$DATA_DIR/shop.db"

if [ ! -d "$DATA_DIR" ]; then
	printf 'Nothing to reset — %s does not exist.\n' "$DATA_DIR"
	exit 0
fi

# A running app holds the database open and would write its in-memory state
# straight back over whatever this script does.
if pgrep -x srotas-desk >/dev/null 2>&1; then
	die "Srotas Desk is running — quit it first"
fi

if [ "$KEEP_LICENCE" -eq 1 ]; then
	command -v sqlite3 >/dev/null 2>&1 || die "sqlite3 is not installed (needed for --keep-licence)"
	[ -f "$DB" ] || die "no database at $DB — nothing to keep a licence from"
fi

printf 'Data directory : %s\n' "$DATA_DIR"
if [ -f "$DB" ]; then
	ITEMS=$(sqlite3 "$DB" "SELECT COUNT(*) FROM items;" 2>/dev/null || echo '?')
	BILLS=$(sqlite3 "$DB" "SELECT COUNT(*) FROM bills;" 2>/dev/null || echo '?')
	SHOP=$(sqlite3 "$DB" "SELECT COALESCE(shop_name,'') FROM shop_profile WHERE id = 1;" 2>/dev/null || echo '')
	printf 'Shop           : %s\n' "${SHOP:-<not registered>}"
	printf 'Will delete    : %s items, %s bills, all stock history\n' "$ITEMS" "$BILLS"
fi
if [ "$KEEP_LICENCE" -eq 1 ]; then
	printf 'Licence        : KEPT — this machine stays activated\n'
else
	printf 'Licence        : REMOVED — a new device id is generated on next launch,\n'
	printf '                 and the app will ask to be activated again\n'
fi

if [ "$ASSUME_YES" -eq 0 ]; then
	printf '\nThis cannot be undone. Continue? [y/N] '
	read -r reply
	case "$reply" in
	y | Y | yes | YES) ;;
	*) die "cancelled" ;;
	esac
fi

if [ "$KEEP_LICENCE" -eq 1 ]; then
	# The licence row is bound to a device id that only exists inside this
	# database file, so keeping the licence means keeping the file and
	# emptying everything else out of it — not deleting and recreating it.
	sqlite3 "$DB" <<-'SQL'
		BEGIN IMMEDIATE;
		DELETE FROM bill_items;
		DELETE FROM bills;
		DELETE FROM transactions;
		DELETE FROM items;
		DELETE FROM shop_profile;
		DELETE FROM sqlite_sequence WHERE name IN ('items','transactions','bills','bill_items');
		COMMIT;
		VACUUM;
	SQL
	rm -f "$DATA_DIR/settings.txt"
	printf '\nShop data cleared. The licence and device id are intact.\n'
else
	# -wal and -shm siblings only exist if the app was killed mid-write, but
	# leaving one behind would resurrect part of the old database.
	rm -f "$DB" "$DB-wal" "$DB-shm" "$DATA_DIR/settings.txt"
	rmdir "$DATA_DIR" 2>/dev/null || true
	printf '\nReset complete. The next launch starts from registration and activation.\n'
fi
