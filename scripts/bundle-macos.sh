#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
if [[ "$(uname -s)" != Darwin ]]; then
  echo "Dieses Script benötigt macOS." >&2
  exit 1
fi
# MACOS_SIGN_IDENTITY (z. B. "Developer ID Application: Name (TEAMID)") signiert
# mit Hardened Runtime und Zeitstempel für die Notarisierung; ohne die Variable
# wird ad hoc signiert.
identity="${MACOS_SIGN_IDENTITY:-}"
sign() {
  if [[ -n "$identity" ]]; then
    codesign --force --options runtime --timestamp --sign "$identity" "$@"
  else
    codesign --force --sign - "$@"
  fi
}
# Das Bundle enthält die egui-Oberfläche (dieselbe wie unter Windows/Linux).
cargo build --release --bins --locked
app="$project_dir/dist/VisiCutR.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/visicutr "$app/Contents/MacOS/visicutr"
sign "$app/Contents/MacOS/visicutr"
cp target/release/visicutr-cameraserver "$app/Contents/MacOS/visicutr-cameraserver"
sign "$app/Contents/MacOS/visicutr-cameraserver"
# visicutr-lenscal ist abgeschaltet: Die Objektivkalibrierung mit Schachbrett
# erhöhte auf der FAU-Laserkamera nur den Fehler (siehe Cargo.toml).
# cp target/release/visicutr-lenscal "$app/Contents/MacOS/visicutr-lenscal"
# sign "$app/Contents/MacOS/visicutr-lenscal"
cp resources/icon/VisiCutR.icns "$app/Contents/Resources/VisiCutR.icns"
cp COPYING "$app/Contents/Resources/COPYING"
cp COPYING.LESSER "$app/Contents/Resources/COPYING.LESSER"
cp README.md "$app/Contents/Resources/README.md"
# Die Anleitung (Wiki-Seiten ohne die Prüfberichte).
mkdir -p "$app/Contents/Resources/docs"
cp docs/*.md "$app/Contents/Resources/docs/"
cp -R docs/images "$app/Contents/Resources/docs/images"
cp resources/materials.json "$app/Contents/Resources/materials.json"
# Inkscape-Erweiterung zum Installieren von Hand (VisiCutR installiert sie auch selbst).
cp -R resources/inkscape "$app/Contents/Resources/inkscape-extension"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>VisiCutR</string>
  <key>CFBundleDisplayName</key><string>VisiCutR</string>
  <key>CFBundleExecutable</key><string>visicutr</string>
  <key>CFBundleIdentifier</key><string>io.github.dbambus.visicutr</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>VisiCutR.icns</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSLocalNetworkUsageDescription</key><string>VisiCutR sendet Aufträge über das lokale Netzwerk an den Lasercutter und lädt das Kamerabild.</string>
  <key>CFBundleDocumentTypes</key><array>
    <dict><key>CFBundleTypeName</key><string>VisiCutR Projekt</string>
    <key>CFBundleTypeRole</key><string>Editor</string>
    <key>LSHandlerRank</key><string>Owner</string>
    <key>LSItemContentTypes</key><array><string>org.visicut.project</string></array></dict>
    <dict><key>CFBundleTypeName</key><string>VisiCut Portable Laser File</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSHandlerRank</key><string>Default</string>
    <key>LSItemContentTypes</key><array><string>de.thomas-oster.visicut.plf</string></array></dict>
    <dict><key>CFBundleTypeName</key><string>VisiCut LaserScript</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSHandlerRank</key><string>Default</string>
    <key>LSItemContentTypes</key><array><string>de.thomas-oster.visicut.laserscript</string></array></dict>
    <dict><key>CFBundleTypeName</key><string>Grafik</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSHandlerRank</key><string>Alternate</string>
    <key>LSItemContentTypes</key><array>
      <string>public.svg-image</string>
      <string>com.adobe.pdf</string>
      <string>com.adobe.encapsulated-postscript</string>
      <string>com.autodesk.dxf</string>
    </array></dict>
  </array>
  <key>UTExportedTypeDeclarations</key><array><dict>
    <key>UTTypeIdentifier</key><string>org.visicut.project</string>
    <key>UTTypeDescription</key><string>VisiCutR Projekt</string>
    <key>UTTypeConformsTo</key><array><string>public.json</string></array>
    <key>UTTypeTagSpecification</key><dict><key>public.filename-extension</key><array><string>vcr</string></array></dict>
  </dict></array>
  <key>UTImportedTypeDeclarations</key><array>
    <dict><key>UTTypeIdentifier</key><string>de.thomas-oster.visicut.plf</string>
    <key>UTTypeDescription</key><string>VisiCut Portable Laser File</string>
    <key>UTTypeConformsTo</key><array><string>public.zip-archive</string></array>
    <key>UTTypeTagSpecification</key><dict><key>public.filename-extension</key><array><string>plf</string></array></dict></dict>
    <dict><key>UTTypeIdentifier</key><string>de.thomas-oster.visicut.laserscript</string>
    <key>UTTypeDescription</key><string>VisiCut LaserScript</string>
    <key>UTTypeConformsTo</key><array><string>public.plain-text</string></array>
    <key>UTTypeTagSpecification</key><dict><key>public.filename-extension</key><array><string>ls</string></array></dict></dict>
    <dict><key>UTTypeIdentifier</key><string>com.autodesk.dxf</string>
    <key>UTTypeDescription</key><string>DXF</string>
    <key>UTTypeConformsTo</key><array><string>public.data</string></array>
    <key>UTTypeTagSpecification</key><dict><key>public.filename-extension</key><array><string>dxf</string></array></dict></dict>
  </array>
</dict></plist>
PLIST
python3 - "$app/Contents/Info.plist" <<'PY'
import json
import plistlib
import subprocess
import sys
from pathlib import Path

metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
]))
version = metadata["packages"][0]["version"].split("-")[0]
path = Path(sys.argv[1])
info = plistlib.loads(path.read_bytes())
info["CFBundleVersion"] = version
info["CFBundleShortVersionString"] = version
path.write_bytes(plistlib.dumps(info))
PY
plutil -lint "$app/Contents/Info.plist"
sign "$app"
codesign --verify --strict --deep "$app"
codesign --display --verbose=2 "$app" 2>&1 | grep -E '^(Authority|TeamIdentifier|Signature)=' || true
echo "$app"
