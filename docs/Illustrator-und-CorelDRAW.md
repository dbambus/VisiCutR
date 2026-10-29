# Illustrator und CorelDRAW

Ein Skript für **Adobe Illustrator** (CS5.1 oder neuer) und ein Makro für
**CorelDRAW** (2017 oder neuer, Windows, mit installiertem VBA) schicken die
Zeichnung wie die Inkscape-Erweiterung an VisiCutR. Ist etwas ausgewählt,
wird nur die Auswahl übergeben, an ihrer Lage auf der Seite (Illustrator:
aktive Zeichenfläche); Texte werden zu Pfaden. Ist die Zeichnung schon ein
Teil gleichen Namens, ersetzt VisiCutR nur dessen Grafik.

**Speichern:** unter **Einstellungen → Software-Add-ins** mit
**Illustrator-Skript speichern …** bzw. **CorelDRAW-Makro speichern …**; im
Windows-Download liegen die Dateien auch unter `illustrator-script/` und
`coreldraw-macro/`. Beide finden VisiCutR über `visicutr-path.txt`, das
VisiCutR bei jedem Start in seinen Einstellungsordner schreibt
(`%APPDATA%\VisiCutR`, macOS `~/Library/Application Support/VisiCutR`);
danach suchen sie in `%LOCALAPPDATA%\Programs\VisiCutR`,
`%ProgramFiles%\VisiCutR` bzw. `/Applications`, `VISICUTR_BIN` gibt den
Pfad fest vor. VisiCutR also einmal gestartet haben.

**Illustrator:** `VisiCutR.jsx` mit **Datei → Skripten → Anderes Skript …**
(Strg+F12) ausführen. Im Skriptordner von Illustrator gespeichert
(`Vorgaben\de_DE\Skripten` bzw. `Presets\en_US\Scripts` im Programmordner,
Administratorrechte nötig) steht es nach einem Neustart direkt unter
**Datei → Skripten**. Das Skript fügt immer als Teil hinzu. Läuft
VisiCutR, geht die Datei direkt an das offene Fenster; sonst startet das
Skript VisiCutR und wartet bis zu 30 s darauf. Der Export nutzt die
Dokumentfarben: In CMYK-Dokumenten wird reines Rot zu einem etwas anderen
RGB-Rot. Für Zuordnungen nach Farbe deshalb im RGB-Modus arbeiten.

**CorelDRAW:** Im Makro-Editor (**Extras → Makros → Makro-Editor**, Alt+F11)
das Projekt **GlobalMacros** wählen und `VisiCutR.bas` mit **Datei → Datei
importieren …** einfügen. Danach stehen unter **Extras → Makros → Makro
ausführen** `AddToVisiCutR` (als Teil hinzufügen) und `OpenInVisiCutR`
(Projekt ersetzen); unter **Extras → Optionen → Anpassung → Befehle →
Makros** lassen sie sich als Symbol in eine Leiste ziehen oder auf ein
Tastenkürzel legen. Das Makro kopiert die Seite oder Auswahl in ein
vorübergehendes Dokument, wandelt dort Texte in Kurven um, exportiert es als
SVG (Millimeter mit viewBox, also maßgenau) und ruft
`visicutr --update [--add] -- datei.svg` auf.

Die exportierten SVG-Dateien bleiben im Temp-Ordner (`visicutr-…`), weil
VisiCutR sie beim Neuladen erneut liest.

[← Übersicht](README.md)
