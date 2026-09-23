#!/usr/bin/env bash
# Regenerates the extra Montserrat faces that TextView.setTextSize can snap to
# (crates/pd-lvgl-sys/lvgl/fonts/pd_font_montserrat_<N>.c).
#
# Every face is an ASCII-only subset (0x20-0x7F plus the degree sign and the
# bullet: the glyph set of LVGL's stock Montserrat 14 minus its FontAwesome
# symbols) of the same Montserrat-Medium.ttf LVGL builds its own fonts from,
# 4 bits per pixel, uncompressed. ASCII-only halves the flash of a stock face
# at the same size; uncompressed because LVGL's RLE decodes every glyph at
# draw time on a core that compiles its C at -Os and already runs close to its
# UI tick budget. The output is committed, so this script runs only when a size
# is added or the tool is bumped: add the size to SIZES below, run the script,
# add the matching row to crates/pd-lvgl-sys/lvgl/pd_fonts.c, and list the
# size in the MCU/board `text_sizes` key. A board never compiles a face it does
# not list, so an unused size costs nothing.
#
# Needs node and npx (the website build already does); the tool version is
# pinned so a regeneration is byte-identical.
set -euo pipefail

SIZES="20 28 64"
TOOL="lv_font_conv@1.5.3"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TTF="$ROOT/third_party/lvgl/scripts/generators/built_in_font/Montserrat-Medium.ttf"
OUT="$ROOT/crates/pd-lvgl-sys/lvgl/fonts"

if [ ! -f "$TTF" ]; then
    echo "gen-fonts: $TTF missing (git submodule update --init third_party/lvgl)" >&2
    exit 1
fi
mkdir -p "$OUT"
for size in ${*:-$SIZES}; do
    file="$OUT/pd_font_montserrat_${size}.c"
    echo "==> $file"
    npx --yes "$TOOL" \
        --bpp 4 --size "$size" \
        --font "$TTF" -r 0x20-0x7F,0xB0,0x2022 \
        --format lvgl --no-compress --no-prefilter \
        -o "$file"
done
