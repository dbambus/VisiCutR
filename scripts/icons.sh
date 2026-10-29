#!/bin/bash
# Erzeugt die Programmsymbole aus resources/icon/visicutr.svg (macOS: braucht
# rsvg-convert und ImageMagick, z. B. per Homebrew, sowie iconutil).
set -euo pipefail
cd "$(dirname "$0")/../resources/icon"
rsvg-convert -w 256 -h 256 visicutr.svg -o icon.png
set_dir="$(mktemp -d)/VisiCutR.iconset"
mkdir -p "$set_dir"
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size" -h "$size" visicutr.svg -o "$set_dir/icon_${size}x${size}.png"
  rsvg-convert -w $((size * 2)) -h $((size * 2)) visicutr.svg -o "$set_dir/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$set_dir" -o VisiCutR.icns
for size in 16 24 32 48 64 128 256; do
  rsvg-convert -w "$size" -h "$size" visicutr.svg -o "$(dirname "$set_dir")/ico-$size.png"
done
magick "$(dirname "$set_dir")"/ico-{16,24,32,48,64,128,256}.png VisiCutR.ico
rm -rf "$(dirname "$set_dir")"
ls -l icon.png VisiCutR.icns VisiCutR.ico
