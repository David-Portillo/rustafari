#!/usr/bin/env bash
#
# Regenerates the fonts in crates/rustafari-app/assets/.
#
#   Inter-Medium.ttf            UI text      from Inter[opsz,wght]  @ wght=500
#   JetBrainsMono-Regular.ttf   code panes   from JetBrainsMono[wght] @ wght=400
#   lucide.ttf                  icons        from the full Lucide font
#
# The text fonts are instantiated from their variable sources at one weight and
# subset to Latin, Latin Extended, Greek and Cyrillic — the same coverage egui's
# bundled defaults had, at a fraction of the size. The icon font is subset to
# exactly the codepoints referenced in src/icons.rs, which is read directly so
# that file stays the single source of truth: add a constant there, run this,
# and the glyph appears.
#
# NotoEmoji-Regular.ttf is not regenerated here; it is vendored whole from
# egui's own default set, since emoji cannot meaningfully be subset.
#
# Requires python3. Everything else (downloads, fonttools) goes in a throwaway
# virtualenv under target/, so nothing is added to your system.
set -euo pipefail

cd "$(dirname "$0")/.."

ASSETS=crates/rustafari-app/assets
ICONS_RS=crates/rustafari-app/src/icons.rs
WORK=target/font-subset
mkdir -p "$WORK" "$ASSETS"

# Latin, Latin-1, Latin Extended-A/B, spacing modifiers, Greek, Cyrillic,
# Latin Extended Additional, general punctuation, currency, ™, arrows, and the
# few math glyphs (−, ∕, ≠, ≤, ≥) that turn up in dev output.
TEXT_RANGES="U+0020-007E,U+00A0-024F,U+02C6-02DC,U+0370-03FF,U+0400-04FF,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,U+2122,U+2190-2199,U+2212,U+2215,U+2260,U+2264,U+2265,U+FEFF,U+FFFD"

if [[ ! -x "$WORK/venv/bin/pyftsubset" ]]; then
	echo "==> Installing fonttools"
	python3 -m venv "$WORK/venv"
	"$WORK/venv/bin/pip" install -q fonttools
fi
PY="$WORK/venv/bin"

fetch() { # fetch <url> <dest>
	if [[ ! -f "$2" ]]; then
		echo "==> Downloading $(basename "$2")"
		curl -sSL --fail -o "$2" "$1"
	fi
}

GF=https://github.com/google/fonts/raw/main/ofl
fetch "$GF/inter/Inter%5Bopsz%2Cwght%5D.ttf" "$WORK/inter-var.ttf"
fetch "$GF/inter/OFL.txt" "$ASSETS/INTER-LICENSE"
fetch "$GF/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf" "$WORK/jbmono-var.ttf"
fetch "$GF/jetbrainsmono/OFL.txt" "$ASSETS/JETBRAINSMONO-LICENSE"
fetch "https://unpkg.com/lucide-static@latest/font/lucide.ttf" "$WORK/lucide-full.ttf"
fetch "https://raw.githubusercontent.com/lucide-icons/lucide/main/LICENSE" "$ASSETS/LUCIDE-LICENSE"

echo "==> Inter Medium"
"$PY/fonttools" varLib.instancer -q "$WORK/inter-var.ttf" wght=500 opsz=14 -o "$WORK/inter-500.ttf"
"$PY/pyftsubset" "$WORK/inter-500.ttf" --unicodes="$TEXT_RANGES" \
	--output-file="$ASSETS/Inter-Medium.ttf" \
	--layout-features='kern,liga,calt,ccmp,mark,mkmk' --no-hinting --desubroutinize

echo "==> JetBrains Mono Regular"
"$PY/fonttools" varLib.instancer -q "$WORK/jbmono-var.ttf" wght=400 -o "$WORK/jbmono-400.ttf"
"$PY/pyftsubset" "$WORK/jbmono-400.ttf" --unicodes="$TEXT_RANGES" \
	--output-file="$ASSETS/JetBrainsMono-Regular.ttf" \
	--layout-features='kern,liga,calt' --no-hinting --desubroutinize

echo "==> Lucide icons"
# Matches the \u{E123} escapes in the icon constants.
ICON_CODEPOINTS=$(grep -oE '\\u\{[0-9A-Fa-f]+\}' "$ICONS_RS" |
	sed -E 's/\\u\{(.*)\}/U+\1/' | sort -u | paste -sd, -)
if [[ -z "$ICON_CODEPOINTS" ]]; then
	echo "!! No icon codepoints found in $ICONS_RS" >&2
	exit 1
fi
echo "    $(tr ',' '\n' <<<"$ICON_CODEPOINTS" | wc -l | tr -d ' ') glyphs"
"$PY/pyftsubset" "$WORK/lucide-full.ttf" --unicodes="$ICON_CODEPOINTS" \
	--output-file="$ASSETS/lucide.ttf" \
	--no-hinting --desubroutinize --layout-features=''

echo "==> Done"
for f in Inter-Medium JetBrainsMono-Regular NotoEmoji-Regular lucide; do
	printf '    %-28s %7d bytes\n' "$f.ttf" "$(wc -c <"$ASSETS/$f.ttf" | tr -d ' ')"
done
echo "    New icon codepoints: https://unpkg.com/lucide-static@latest/font/info.json"
