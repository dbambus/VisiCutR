#!/bin/sh
# Trägt VisiCutR für den angemeldeten Benutzer ins Anwendungsmenü ein und
# ordnet VisiCutR-Projekte (.vcr), PLF und LaserScript (.ls) zu; SVG, PDF,
# DXF und EPS erscheinen unter „Öffnen mit“. Schreibt nur nach
# ~/.local/share (bzw. $XDG_DATA_HOME), braucht also kein root.
#   ./install-desktop.sh              eintragen (nach dem Verschieben erneut)
#   ./install-desktop.sh --uninstall  wieder entfernen
set -eu

here=$(cd "$(dirname "$0")" && pwd)
data=${XDG_DATA_HOME:-$HOME/.local/share}
desktop="$data/applications/visicutr.desktop"
mime="$data/mime/packages/visicutr-mime.xml"
icon_png="$data/icons/hicolor/256x256/apps/visicutr.png"
icon_svg="$data/icons/hicolor/scalable/apps/visicutr.svg"

refresh() {
  if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$data/mime" || true
  fi
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data/applications" || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$data/icons/hicolor" || true
  fi
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$desktop" "$mime" "$icon_png" "$icon_svg"
  refresh
  echo "VisiCutR ist aus dem Anwendungsmenü entfernt."
  exit 0
fi

exe="$here/visicutr"
if [ ! -x "$exe" ]; then
  echo "visicutr nicht gefunden: $exe" >&2
  exit 1
fi
# Diese Zeichen bräuchten in Exec= eine eigene Maskierung.
case "$exe" in
  *[\"\`\$\\\|\&]*)
    echo "Bitte VisiCutR in einen Ordner ohne \" \` \$ \\ | & verschieben." >&2
    exit 1
    ;;
esac

mkdir -p "$(dirname "$desktop")" "$(dirname "$mime")" \
  "$(dirname "$icon_png")" "$(dirname "$icon_svg")"
sed "s|@EXEC@|\"$exe\"|" "$here/desktop/visicutr.desktop" > "$desktop"
cp "$here/desktop/visicutr-mime.xml" "$mime"
cp "$here/desktop/visicutr.png" "$icon_png"
cp "$here/desktop/visicutr.svg" "$icon_svg"
refresh
if command -v xdg-mime >/dev/null 2>&1; then
  xdg-mime default visicutr.desktop \
    application/x-visicutr-project application/x-plf application/x-ls || true
fi
echo "VisiCutR ist im Anwendungsmenü eingetragen: $exe"
