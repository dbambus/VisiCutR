# VisiCutR – Dokumentation

VisiCutR bereitet Laseraufträge für den LTT iLaser 4000 des FAU FabLabs vor:
egui-Oberfläche, SVG-Verarbeitung und LTT-Treiber in Rust, auf Windows,
Linux und macOS gleich. Es ist eine Neuimplementierung von
[VisiCut](https://github.com/t-oster/VisiCut) und erzeugt für dieselben
Eingaben dieselben Auftragsdateien (siehe [Java-Vergleich](Java-Vergleich.md)).

## Für Anwender

- [Installation](Installation.md) – Downloads, Voraussetzungen, Dateien per Doppelklick
- [Bedienung](Bedienung.md) – Import, Zuordnung, Material, Vorschau, Senden, Sprache und Updates
- [Text, Formen, Schilder, Kopien und Nesting](Text-Formen-Schilder.md)
- [Inkscape und Befehlszeile](Inkscape-und-Befehlszeile.md)
- [Illustrator und CorelDRAW](Illustrator-und-CorelDRAW.md)
- [Lasercutter, Rotationseinheit und Kamera](Lasercutter-Rotation-Kamera.md)

## Technik

- [LTT-Protokoll](LTT-Protokoll.md) – Portierungsstand, bewusste Einschränkungen, Firmware
- [Java-Vergleich](Java-Vergleich.md) – Byte-Vergleich mit Java-VisiCut, Stand und Grenzen
- [Entwicklung](Entwicklung.md) – Bauen, Tests, CI, Releases, Signierung, Übersetzungen

## Prüfberichte

- [Java-Vergleich 2026-10-21](audits/Java-Vergleich-2026-10-21.md)
- [Java-Vergleich 2026-10-22](audits/Java-Vergleich-2026-10-22.md) (mit den Java-Fix-Branches)
- [Java-Vergleich 2026-10-28](audits/Java-Vergleich-2026-10-28.md) (VisiCutR 0.23.0 und Nachträge)
- [Audit 1.0.0-rc.2](audits/AUDIT-1.0.0-rc.2.md) (Sicherheit, Robustheit, Bedienung)

Änderungen je Version: [CHANGELOG](../CHANGELOG.md).
