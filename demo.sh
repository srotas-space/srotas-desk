#!/usr/bin/env bash
#
# Fills the shop database with a large demo catalogue — a north-Indian
# hardware shop's range: tiles, plumbing, nuts and bolts, darwaja (doors),
# almirahs, paints, sanitaryware, electricals and tools.
#
# The point is volume. 100,000 items is far more than any real counter
# stocks; it's here so pagination, search, the item picker and the reports
# can be exercised against a catalogue big enough to hurt.
#
# Items are generated as a cross product of category x specification x
# variant x brand, which is roughly how hardware SKUs really multiply
# (an M8 bolt exists in eight lengths and four finishes). Every row is
# derived from a fixed hash of its position, so two runs of this script
# produce byte-for-byte the same catalogue.
#
#   ./demo.sh                    # 100,000 items, refuses if items exist
#   ./demo.sh --fresh            # replace the catalogue and stock history
#   ./demo.sh --append           # add more, continuing where it left off
#   ./demo.sh --count 5000       # a smaller set, still spread over every category
#   ./demo.sh --db /tmp/shop.db  # a database somewhere else
#   ./demo.sh -y                 # skip the confirmation
#
set -euo pipefail

COUNT=100000
DB=""
MODE="" # fresh | append
ASSUME_YES=0

die() {
	printf 'demo.sh: %s\n' "$1" >&2
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
	--count)
		COUNT="${2:-}"
		shift 2
		;;
	--db)
		DB="${2:-}"
		shift 2
		;;
	--fresh)
		MODE=fresh
		shift
		;;
	--append)
		MODE=append
		shift
		;;
	-y | --yes)
		ASSUME_YES=1
		shift
		;;
	-h | --help) usage ;;
	*) die "unknown option '$1' (try --help)" ;;
	esac
done

case "$COUNT" in
'' | *[!0-9]*) die "--count needs a whole number, got '$COUNT'" ;;
esac
[ "$COUNT" -gt 0 ] || die "--count must be at least 1"

command -v sqlite3 >/dev/null 2>&1 || die "sqlite3 is not installed"

# Mirrors `dirs::data_dir()` in src/db.rs — keep the two in step.
if [ -z "$DB" ]; then
	case "$(uname -s)" in
	Darwin) DATA_DIR="$HOME/Library/Application Support/srotas-desk" ;;
	Linux) DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/srotas-desk" ;;
	MINGW* | MSYS* | CYGWIN*) DATA_DIR="${APPDATA:-$HOME/AppData/Roaming}/srotas-desk" ;;
	*) die "unsupported platform '$(uname -s)' — pass --db explicitly" ;;
	esac
	DB="$DATA_DIR/shop.db"
fi

# The schema is created by sqlx migrations inside the app, not here, so an
# absent database means the app has simply never been opened on this
# machine yet.
[ -f "$DB" ] || die "no database at $DB — launch Srotas Desk once (cargo run) so it can create one, then re-run"
sqlite3 "$DB" "SELECT 1 FROM items LIMIT 1;" >/dev/null 2>&1 ||
	die "$DB has no items table — is that really a Srotas Desk database?"

if pgrep -x srotas-desk >/dev/null 2>&1; then
	die "Srotas Desk is running — quit it first, or it will overwrite what this script writes"
fi

EXISTING=$(sqlite3 "$DB" "SELECT COUNT(*) FROM items;")
if [ "$EXISTING" -gt 0 ] && [ -z "$MODE" ]; then
	die "$DB already holds $EXISTING items — pass --fresh to replace them, or --append to add alongside"
fi

# The generator is deterministic, so appending has to skip the rows that
# are already there — regenerating from the top would collide with every
# existing name.
SKIP=0
if [ "$MODE" = append ]; then
	SKIP="$EXISTING"
fi

printf 'Database : %s\n' "$DB"
printf 'Items    : %s (%s existing)\n' "$COUNT" "$EXISTING"
printf 'Mode     : %s\n' "${MODE:-seed}"

if [ "$ASSUME_YES" -eq 0 ]; then
	printf 'Continue? [y/N] '
	read -r reply
	case "$reply" in
	y | Y | yes | YES) ;;
	*) die "cancelled" ;;
	esac
fi

SQL=$(mktemp -t srotas-demo)
trap 'rm -f "$SQL"' EXIT

{
	echo 'PRAGMA foreign_keys = ON;'
	echo 'BEGIN IMMEDIATE;'

	if [ "$MODE" = fresh ]; then
		# Stock history references items, so it has to go first. The shop
		# profile and the licence are deliberately left alone — this
		# replaces the catalogue, it isn't a factory reset (see reset.sh).
		cat <<-'SQL'
			DELETE FROM bill_items;
			DELETE FROM bills;
			DELETE FROM transactions;
			DELETE FROM items;
			DELETE FROM sqlite_sequence WHERE name IN ('items','transactions','bills','bill_items');
		SQL
	fi

	cat <<-'SQL'
		-- A shop has to exist for the app to get past registration. Only
		-- filled in when there isn't one already, so this never overwrites
		-- a real shop's details.
		INSERT INTO shop_profile (id, shop_name, owner_name, phone, address, created_at, gst_rate_bp, gstin)
		SELECT 1, 'Sharma Hardware Store', 'Ramesh Sharma', '98765 43210',
		       '14 Birhana Road, Kanpur 208001', '2026-01-01T09:00:00Z', 1800, '09AAACS1234F1ZV'
		WHERE NOT EXISTS (SELECT 1 FROM shop_profile WHERE id = 1);

		-- ------------------------------------------------------------------
		-- Dimensions. Item names are (brand, category, spec, variant) tuples,
		-- so distinct tuples give distinct names — which matters, because
		-- items.name carries a case-insensitive UNIQUE index.
		-- ------------------------------------------------------------------

		CREATE TEMP TABLE d_brand(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
		INSERT INTO d_brand(name) VALUES
		  ('Ajanta'),('Bharat'),('Cera'),('Deepak'),('Everest'),
		  ('Gupta'),('Hindustan'),('Jindal'),('Kisan'),('Supreme');

		-- Variants are grouped so a bolt gets a metal finish and a tile gets
		-- a colour, rather than every item sharing one implausible list.
		CREATE TEMP TABLE d_variant(id INTEGER PRIMARY KEY, vkind TEXT NOT NULL, name TEXT NOT NULL);
		INSERT INTO d_variant(vkind, name) VALUES
		  ('colour','Ivory'),('colour','Beige'),('colour','Pearl White'),('colour','Ash Grey'),('colour','Charcoal'),
		  ('colour','Walnut'),('colour','Teak'),('colour','Terracotta'),('colour','Sea Green'),('colour','Royale Blue'),
		  ('finish','Zinc Plated'),('finish','SS 304'),('finish','SS 202'),('finish','MS Black'),('finish','Chrome'),
		  ('finish','Antique Brass'),('finish','Satin Nickel'),('finish','Powder Coated'),('finish','Galvanised'),('finish','Brass Polished'),
		  ('grade','ISI Marked'),('grade','Heavy Duty'),('grade','Standard'),('grade','Premium'),('grade','Agri Grade'),
		  ('grade','Plumbing Grade'),('grade','Industrial'),('grade','Economy'),('grade','Contractor'),('grade','Pro Series');

		-- ------------------------------------------------------------------
		-- Specifications, one family per `kind`. Size grids are generated
		-- arithmetically (an M4-to-M20 bolt in eight lengths is 72 rows that
		-- nobody should type out); the rest are short explicit lists.
		-- ------------------------------------------------------------------

		-- `mult` scales the category's base price for this size: a 20 L pail
		-- of emulsion should not cost the same as a 500 ml tin, and an M20
		-- bolt should not cost the same as an M4. Without it the catalogue
		-- reads as obviously fake the moment you sort by price.
		CREATE TEMP TABLE d_spec(id INTEGER PRIMARY KEY, kind TEXT NOT NULL, label TEXT NOT NULL, mult REAL NOT NULL);

		-- Tiles: five formats in six surface finishes, priced by area.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'tile', s.column1 || ' ' || f.column1, s.column2
		FROM (VALUES ('300x300mm',0.5),('300x600mm',0.8),('600x600mm',1.0),('800x800mm',1.6),('600x1200mm',2.4)) s
		CROSS JOIN (VALUES ('Matt'),('Glossy'),('Rustic'),('Satin'),('Polished'),('Carving')) f;

		-- Pipes: six bores in five pressure classes, priced by bore.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'pipe', s.column1 || ' ' || c.column1, s.column2
		FROM (VALUES ('1/2 inch',0.5),('3/4 inch',0.7),('1 inch',1.0),('1.25 inch',1.4),('1.5 inch',1.8),('2 inch',2.6)) s
		CROSS JOIN (VALUES ('Class A'),('Class B'),('Class C'),('SCH 40'),('SCH 80')) c;

		-- Fittings: six bores in four patterns.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'fitting', s.column1 || ' ' || p.column1, s.column2
		FROM (VALUES ('1/2 inch',0.5),('3/4 inch',0.7),('1 inch',1.0),('1.25 inch',1.4),('1.5 inch',1.8),('2 inch',2.6)) s
		CROSS JOIN (VALUES ('Plain'),('Threaded'),('Reducer'),('Long Body')) p;

		-- Fasteners: M4..M20 in even steps, eight lengths each. Price rises
		-- with both, which is why the multiplier is computed rather than listed.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'fastener',
		       'M' || (4 + 2 * (n % 9)) || ' x ' || (10 + 5 * (n / 9)) || 'mm',
		       ((4 + 2 * (n % 9)) / 8.0) * ((10 + 5 * (n / 9)) / 25.0)
		FROM (WITH RECURSIVE s(n) AS (SELECT 0 UNION ALL SELECT n + 1 FROM s WHERE n < 71) SELECT n FROM s);

		-- Doors (darwaja): five leaf sizes in five constructions.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'door', s.column1 || ' ' || c.column1, s.column2
		FROM (VALUES ('30x78 inch',0.85),('32x78 inch',0.92),('32x80 inch',1.0),('36x80 inch',1.12),('36x84 inch',1.25)) s
		CROSS JOIN (VALUES ('Solid Core'),('Hollow Core'),('Membrane'),('Laminated'),('Moulded')) c;

		-- Almirah: four sizes in four internal configurations.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'almirah', s.column1 || ' ' || c.column1, s.column2
		FROM (VALUES ('4x2 ft',0.6),('5x3 ft',0.85),('6x3 ft',1.0),('7x4 ft',1.45)) s
		CROSS JOIN (VALUES ('2 Door'),('3 Door'),('2 Door with Locker'),('3 Door with Mirror')) c;

		-- Paints: five pack sizes in six sheens. The base band is a 4 L pack.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'paint', p.column1 || ' ' || sh.column1, p.column2
		FROM (VALUES ('500 ml',0.16),('1 L',0.30),('4 L',1.0),('10 L',2.3),('20 L',4.2)) p
		CROSS JOIN (VALUES ('Matt'),('Sheen'),('Satin'),('Gloss'),('Velvet'),('Textured')) sh;

		-- Builders hardware: six sizes in five duty ratings.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'hardware', s.column1 || ' ' || d.column1, s.column2
		FROM (VALUES ('2 inch',0.6),('3 inch',0.8),('4 inch',1.0),('5 inch',1.25),('6 inch',1.5),('8 inch',2.0)) s
		CROSS JOIN (VALUES ('Light Duty'),('Medium Duty'),('Heavy Duty'),('Auto Close'),('Ball Bearing')) d;

		-- Electricals: six conductor sizes in five patterns.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'electrical', r.column1 || ' ' || p.column1, r.column2
		FROM (VALUES ('0.75 sqmm',0.4),('1.0 sqmm',0.55),('1.5 sqmm',0.8),('2.5 sqmm',1.0),('4.0 sqmm',1.5),('6.0 sqmm',2.2)) r
		CROSS JOIN (VALUES ('Single Core'),('Multi Strand'),('Flexible'),('FR'),('FRLS')) p;

		-- Switchgear is rated in amps, so it gets its own family — a switch
		-- described as "2.5 sqmm" would be nonsense.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'switchgear', r.column1 || ' ' || p.column1, r.column2
		FROM (VALUES ('6A',0.6),('10A',0.8),('16A',1.0),('20A',1.3),('32A',1.8),('63A',2.8)) r
		CROSS JOIN (VALUES ('Single Pole'),('Double Pole'),('Triple Pole'),('C Curve'),('B Curve')) p;

		-- Tools: six sizes in four builds.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'tool', s.column1 || ' ' || b.column1, s.column2
		FROM (VALUES ('4 inch',0.6),('6 inch',0.8),('8 inch',1.0),('10 inch',1.2),('12 inch',1.5),('18 inch',2.0)) s
		CROSS JOIN (VALUES ('Drop Forged'),('Chrome Vanadium'),('Insulated'),('Rubber Grip')) b;

		-- Cement: four grades in three pack sizes. Sold by the kilo, so the
		-- pack size names the bag without changing the per-kg price.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'cement', g.column1 || ' ' || p.column1, 1.0
		FROM (VALUES ('OPC 43 Grade'),('OPC 53 Grade'),('PPC'),('Rapid Hardening')) g
		CROSS JOIN (VALUES ('1 kg Pack'),('20 kg Bag'),('50 kg Bag')) p;

		-- Adhesives and putties: also sold by the kilo, but graded differently
		-- from cement — "Tile Adhesive OPC 43 Grade" would be nonsense.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'compound', g.column1 || ' ' || p.column1, 1.0
		FROM (VALUES ('Grey'),('White'),('Waterproof'),('Fast Set')) g
		CROSS JOIN (VALUES ('1 kg Pack'),('20 kg Bag'),('40 kg Bag')) p;

		-- Sanitaryware: five models in four mountings.
		INSERT INTO d_spec(kind, label, mult)
		SELECT 'sanitary', m.column1 || ' ' || f.column1, m.column2
		FROM (VALUES ('Standard',1.0),('Compact',0.85),('Deluxe',1.4),('Designer',1.9),('Institutional',1.2)) m
		CROSS JOIN (VALUES ('Wall Mounted'),('Floor Mounted'),('Counter Top'),('Concealed')) f;

		-- ------------------------------------------------------------------
		-- Categories. `buy_lo`/`buy_hi` are paise and bound the cost price;
		-- `gst_bp` is basis points (1800 = 18%), matching the app's own
		-- convention. `unit` is constrained by the schema to piece/kg/metre.
		-- ------------------------------------------------------------------

		CREATE TEMP TABLE d_cat(
		  id INTEGER PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, vkind TEXT NOT NULL,
		  unit TEXT NOT NULL, buy_lo INTEGER NOT NULL, buy_hi INTEGER NOT NULL, gst_bp INTEGER NOT NULL
		);
		INSERT INTO d_cat(name, kind, vkind, unit, buy_lo, buy_hi, gst_bp) VALUES
		  ('Ceramic Floor Tile','tile','colour','piece',   2800,  9500, 1800),
		  ('Vitrified Tile','tile','colour','piece',       4500, 18000, 1800),
		  ('Wall Tile','tile','colour','piece',            2200,  7800, 1800),
		  ('Elevation Tile','tile','colour','piece',       5200, 16000, 1800),
		  ('Anti-Skid Tile','tile','colour','piece',       3600, 11000, 1800),

		  ('PVC Pipe','pipe','grade','metre',              4200, 19000, 1800),
		  ('CPVC Pipe','pipe','grade','metre',             7800, 28000, 1800),
		  ('UPVC Pipe','pipe','grade','metre',             6600, 24000, 1800),
		  ('GI Pipe','pipe','grade','metre',              11000, 42000, 1800),
		  ('SWR Drainage Pipe','pipe','grade','metre',     5400, 21000, 1800),

		  ('PVC Elbow','fitting','grade','piece',          1200,  6500, 1800),
		  ('PVC Tee','fitting','grade','piece',            1400,  7200, 1800),
		  ('CPVC Coupler','fitting','grade','piece',       1600,  8400, 1800),
		  ('Brass Ball Valve','fitting','grade','piece',  18000, 92000, 1800),
		  ('Pipe Clamp','fitting','grade','piece',           600,  3200, 1800),

		  ('Hex Bolt','fastener','finish','piece',           180,  2400, 1800),
		  ('Hex Nut','fastener','finish','piece',             90,  1100, 1800),
		  ('Machine Screw','fastener','finish','piece',      110,  1400, 1800),
		  ('Washer','fastener','finish','piece',              40,   620, 1800),
		  ('Anchor Fastener','fastener','finish','piece',    320,  4200, 1800),
		  ('Self Drilling Screw','fastener','finish','piece', 130,  1600, 1800),

		  ('Flush Door (Darwaja)','door','colour','piece', 185000, 620000, 1800),
		  ('WPC Door (Darwaja)','door','colour','piece',   240000, 780000, 1800),
		  ('PVC Bathroom Door','door','colour','piece',    120000, 380000, 1800),
		  ('Panel Door (Darwaja)','door','colour','piece', 260000, 940000, 1800),

		  ('Steel Almirah','almirah','colour','piece',     620000,1850000, 1800),
		  ('Wardrobe Almirah','almirah','colour','piece',  780000,2400000, 1800),
		  ('Office Almirah','almirah','colour','piece',    540000,1620000, 1800),

		  -- Paint bands describe a 4 L pack; d_spec.mult scales the rest.
		  ('Interior Emulsion','paint','colour','piece',    28000,140000, 1800),
		  ('Exterior Emulsion','paint','colour','piece',    34000,165000, 1800),
		  ('Enamel Paint','paint','colour','piece',         30000,125000, 1800),
		  ('Wood Primer','paint','colour','piece',          22000, 95000, 1800),
		  ('Wall Putty','paint','colour','piece',           16000, 70000, 1800),
		  ('Distemper','paint','colour','piece',            14000, 58000, 1800),

		  ('Door Hinge','hardware','finish','piece',          900,  8600, 1800),
		  ('Door Handle','hardware','finish','piece',        3200, 42000, 1800),
		  ('Mortice Lock','hardware','finish','piece',      28000,185000, 1800),
		  ('Tower Bolt','hardware','finish','piece',         1100, 12000, 1800),
		  ('Drawer Channel','hardware','finish','piece',     2600, 24000, 1800),
		  ('Door Closer','hardware','finish','piece',       42000,240000, 1800),

		  -- Wire is per metre at 2.5 sqmm; switchgear is per piece at 16A.
		  ('Copper Wire','electrical','finish','metre',      2000,  7000, 1800),
		  ('Aluminium Wire','electrical','finish','metre',    900,  3200, 1800),
		  ('Modular Switch','switchgear','finish','piece',   4500, 38000, 1800),
		  ('MCB','switchgear','finish','piece',             14000, 96000, 1800),

		  ('Screwdriver','tool','finish','piece',            3400, 28000, 1800),
		  ('Plier','tool','finish','piece',                  6800, 54000, 1800),
		  ('Hammer','tool','finish','piece',                 9200, 62000, 1800),
		  ('Adjustable Spanner','tool','finish','piece',     8600, 58000, 1800),
		  ('Measuring Tape','tool','finish','piece',         4200, 26000, 1800),

		  -- Sold by the kilo, so these bands are per kg, not per bag.
		  ('Cement','cement','grade','kg',                    600,  1400, 2800),
		  ('White Cement','cement','grade','kg',             1800,  4200, 2800),
		  ('Tile Adhesive','compound','grade','kg',          1100,  3200, 1800),
		  ('Wall Care Putty','compound','grade','kg',         900,  2600, 1800),

		  ('Wash Basin','sanitary','colour','piece',        95000, 480000, 1800),
		  ('Water Closet','sanitary','colour','piece',     185000,1250000, 1800),
		  ('Shower Set','sanitary','colour','piece',        42000, 320000, 1800),
		  ('Bib Cock','sanitary','colour','piece',          28000, 165000, 1800);

		-- ------------------------------------------------------------------
		-- The catalogue itself.
		--
		-- ROW_NUMBER partitioned by category makes the LIMIT below take rows
		-- round-robin across categories, so asking for 5,000 items gives a
		-- slice of every category rather than five categories in full.
		--
		-- `h` is a cheap deterministic hash of the row's position, standing
		-- in for a random number generator: prices, stock and thresholds are
		-- all derived from it, so re-running produces an identical catalogue.
		-- ------------------------------------------------------------------

		CREATE TEMP TABLE gen AS
		SELECT
		  t.cname, t.spec, t.variant, t.brand, t.unit, t.buy_lo, t.buy_hi, t.gst_bp, t.mult,
		  (t.rn * 2654435761 + t.cid * 40503 + 12345) % 1000003 AS h
		FROM (
		  SELECT c.id AS cid, c.name AS cname, c.unit, c.buy_lo, c.buy_hi, c.gst_bp,
		         s.label AS spec, s.mult AS mult, v.name AS variant, b.name AS brand,
		         ROW_NUMBER() OVER (PARTITION BY c.id ORDER BY s.id, v.id, b.id) AS rn
		  FROM d_cat c
		  JOIN d_spec s ON s.kind = c.kind
		  JOIN d_variant v ON v.vkind = c.vkind
		  CROSS JOIN d_brand b
		) t
		ORDER BY t.rn, t.cid
	SQL

	printf 'LIMIT %s OFFSET %s;\n' "$COUNT" "$SKIP"

	cat <<-'SQL'
		INSERT OR IGNORE INTO items (name, buy_price_paise, sell_price_paise, stock_qty, unit, low_stock_threshold, description, location, gst_rate_bp, deleted)
		SELECT
		  brand || ' ' || cname || ' ' || spec || ' ' || variant,
		  -- The band is the price at this category's reference size; `mult`
		  -- scales it for the actual size, so a 20 L pail costs about four
		  -- times a 4 L can rather than a random amount.
		  MAX(1, CAST((buy_lo + (h % (buy_hi - buy_lo + 1))) * mult AS INTEGER)),
		  -- Margin between 12% and 45% of cost.
		  MAX(2, CAST((buy_lo + (h % (buy_hi - buy_lo + 1))) * mult * (112 + (h % 34)) / 100 AS INTEGER)),
		  -- One row in twenty-five is deliberately below its threshold, so
		  -- the low-stock screens have something to show.
		  CASE WHEN h % 25 = 0 THEN (h % 9) * 0.5 ELSE 8 + (h % 380) * 0.5 END,
		  unit,
		  5 + (h % 16),
		  cname || ' — ' || spec || ', ' || variant || '. Demo catalogue item.',
		  -- Where it sits in the shop. Derived from the same hash, so an
		  -- item keeps its rack across re-runs; one in twelve is left blank
		  -- to exercise the "Not recorded" case.
		  CASE
		    WHEN h % 12 = 0 THEN ''
		    WHEN h % 7 = 0 THEN 'Godown, Row ' || (1 + h % 6)
		    ELSE 'Rack ' || (1 + h % 24) || ', Shelf ' || CHAR(65 + (h / 24) % 6)
		  END,
		  gst_bp,
		  0
		FROM gen;

		DROP TABLE gen;
		COMMIT;
		ANALYZE;
	SQL
} >"$SQL"

printf 'Seeding... '
START=$(date +%s)
sqlite3 "$DB" <"$SQL"
END=$(date +%s)

TOTAL=$(sqlite3 "$DB" "SELECT COUNT(*) FROM items WHERE deleted = 0;")
if [ "$MODE" = append ]; then
	ADDED=$((TOTAL - EXISTING))
else
	ADDED="$TOTAL"
fi
LOW=$(sqlite3 "$DB" "SELECT COUNT(*) FROM items WHERE deleted = 0 AND stock_qty < low_stock_threshold;")
SIZE=$(du -h "$DB" | cut -f1)

printf 'done in %ss\n\n' "$((END - START))"
printf 'Items      : %s added, %s in the catalogue (%s low on stock)\n' "$ADDED" "$TOTAL" "$LOW"
printf 'Database   : %s (%s)\n' "$DB" "$SIZE"
printf '\nOpen the app to browse it:  cargo run\n'
