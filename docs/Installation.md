# Installation

Die fertigen Pakete liegen unter
[GitHub Releases](https://github.com/dbambus/VisiCutR/releases). Versionen mit
Zusatz wie `1.0.0-rc.2` sind Vorabversionen.

| Plattform | Paket | Hinweise |
| --- | --- | --- |
| Windows x64 | `VisiCutR-<Version>-x86_64-pc-windows-msvc.zip` | portabel, entpacken und `visicutr.exe` starten |
| Linux x64 | `VisiCutR-<Version>-x86_64-unknown-linux-gnu.tar.gz` | portabel; Ubuntu 24.04 oder kompatibel |
| macOS Apple Silicon | `VisiCutR-<Version>-aarch64-apple-darwin.zip` | `VisiCutR.app`, macOS 14 oder neuer |

Java wird nicht benötigt. Intel-Macs werden nicht unterstützt. Die macOS-App
ist ad hoc signiert, solange keine Developer ID hinterlegt ist (siehe
[Entwicklung](Entwicklung.md#macos-signierung)).

Alle drei Plattformen nutzen dieselbe egui-Oberfläche (unter Windows und
Linux Kürzel mit Strg statt ⌘) mit denselben Funktionen:
Zoom und Lineale, Einzel- und Regelzuordnung mit Wertvorschlägen,
Materialbibliothek, Parametersätze, Bitmap-Vektorisierung, Schnittreihenfolge,
Text, Formen, Schilder, Kopien und Nesting,
Vorschau mit Zeitschätzung und Zeitslider, Geräte mit Maschinenwerten und
Zeilenversatz-Kalibrierung, Rotationseinheit und Kamera.

Linux benötigt eine grafische Sitzung, OpenGL/EGL, libxkbcommon und einen
Desktop-Portal-Dienst für Dateidialoge. Der CI-Build setzt Ubuntu 24.04 oder
eine kompatible glibc-Laufzeit voraus.

## Dateien per Doppelklick öffnen

VisiCutR öffnet die Datei, mit der es gestartet wird (`visicutr Datei.svg`),
und unter macOS auch Dateien aus dem Finder bei bereits laufender App. Eine
Datei ersetzt das Projekt (mit Rückfrage bei ungesicherten Änderungen), mehrere
werden wie abgelegte Dateien als Teile eingefügt.

| Plattform | Einrichtung | Doppelklick öffnet | Unter „Öffnen mit“ |
| --- | --- | --- | --- |
| macOS | automatisch mit `VisiCutR.app` | `.vcr`, `.plf`, `.ls` | SVG, PDF, DXF, EPS |
| Windows | **Einstellungen → Allgemein → Dateitypen zuordnen** | `.vcr`; `.plf`, `.ls`, wenn kein anderes Programm sie öffnet | SVG, PDF, DXF, EPS |
| Linux | `./install-desktop.sh` im entpackten Ordner | `.vcr`, `.plf`, `.ls` | SVG, PDF, DXF, EPS |

Unter Windows schreibt die Zuordnung nur in `HKCU\Software\Classes` (ohne
Administratorrechte, für das angemeldete Konto) und zeigt auf die gerade
laufende `visicutr.exe`; nach dem Verschieben erneut zuordnen.
**Zuordnung entfernen** löscht alle Einträge wieder; Standardprogramme anderer
Anwendungen, etwa Java-VisiCut für `.plf`, bleiben unangetastet. Unter Linux
legt das Skript Menüeintrag, MIME-Typen (`application/x-visicutr-project`,
`application/x-plf`, `application/x-ls` wie bei Java-VisiCut) und Symbol in
`~/.local/share` ab; `./install-desktop.sh --uninstall` entfernt sie.

[← Übersicht](README.md)
