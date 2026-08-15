#!/usr/bin/env bash
#
# Regenerates crates/rustafari-app/assets/lucide.ttf.
#
# The full Lucide font is ~854 KB for 2000+ icons. We ship only the glyphs the
# app actually references, which is ~8 KB. The list of codepoints is read
# straight out of icons.rs, so that file stays the single source of truth: add
# a `pub const NAME: &str = "\u{E123}";` there, run this, and the glyph appears.
#
# Requires python3. Downloads Lucide and installs fonttools into a throwaway
# virtualenv under target/, so nothing is added to your system.
set -euo pipefail

cd "$(dirname "$0")/.."

ICONS_RS=crates/rustafari-app/src/icons.rs
OUT=crates/rustafari-app/assets/lucide.ttf
WORK=target/icon-subset

mkdir -p "$WORK" "$(dirname "$OUT")"

echo "==> Reading codepoints from $ICONS_RS"
# Matches the \u{E123} escapes in the icon constants.
CODEPOINTS=$(grep -oE '\\u\{[0-9A-Fa-f]+\}' "$ICONS_RS" |
	sed -E 's/\\u\{(.*)\}/U+\1/' | sort -u | paste -sd, -)

if [[ -z "$CODEPOINTS" ]]; then
	echo "!! No icon codepoints found in $ICONS_RS" >&2
	exit 1
fi
echo "    $(tr ',' '\n' <<<"$CODEPOINTS" | wc -l | tr -d ' ') glyphs"

if [[ ! -f "$WORK/lucide-full.ttf" ]]; then
	echo "==> Downloading Lucide"
	curl -sSL --fail -o "$WORK/lucide-full.ttf" \
		"https://unpkg.com/lucide-static@latest/font/lucide.ttf"
	curl -sSL --fail -o crates/rustafari-app/assets/LUCIDE-LICENSE \
		"https://raw.githubusercontent.com/lucide-icons/lucide/main/LICENSE"
fi

if [[ ! -x "$WORK/venv/bin/pyftsubset" ]]; then
	echo "==> Installing fonttools"
	python3 -m venv "$WORK/venv"
	"$WORK/venv/bin/pip" install -q fonttools
fi

echo "==> Subsetting"
"$WORK/venv/bin/pyftsubset" "$WORK/lucide-full.ttf" \
	--unicodes="$CODEPOINTS" \
	--output-file="$OUT" \
	--no-hinting --desubroutinize --layout-features=''

echo "==> Done: $OUT ($(wc -c <"$OUT" | tr -d ' ') bytes)"
echo "    Look up new codepoints at https://unpkg.com/lucide-static@latest/font/info.json"
