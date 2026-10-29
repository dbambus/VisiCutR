# Inkscape und Befehlszeile

Wie in Java-VisiCut lässt sich eine Zeichnung direkt aus Inkscape (1.0 oder
neuer) an VisiCutR schicken.

**Installieren:** In VisiCutR unter **Einstellungen → Software-Add-ins → Inkscape**
auf **Inkscape-Erweiterung installieren** klicken und Inkscape neu starten.
VisiCutR legt die Erweiterung in den Benutzerordner von Inkscape, jeweils im
Unterordner `visicutr`:

| Plattform | Ordner |
|-----------|--------|
| macOS     | `~/Library/Application Support/org.inkscape.Inkscape/config/inkscape/extensions` |
| Windows   | `%APPDATA%\inkscape\extensions` |
| Linux     | `$XDG_CONFIG_HOME/inkscape/extensions` (sonst `~/.config/inkscape/extensions`) |

Dabei merkt sie sich den Programmpfad von VisiCutR (`visicutr-path.txt`).
Wurde VisiCutR verschoben oder aktualisiert, zeigt die Einstellung
„veraltet“; dann erneut installieren. Zum Installieren von Hand liegen die
Dateien im Download unter `inkscape-extension/` (macOS:
`VisiCutR.app/Contents/Resources/inkscape-extension`); den Ordner in das
Erweiterungsverzeichnis kopieren. Inkscape als Snap oder Flatpak nutzt
eigene Ordner (z. B. `~/snap/inkscape/current/.config/inkscape/extensions`,
`~/.var/app/org.inkscape.Inkscape/config/inkscape/extensions`); dorthin
den Ordner von Hand kopieren. Findet die Erweiterung VisiCutR nicht, sucht
sie in `/Applications/VisiCutR.app`, `%LOCALAPPDATA%\Programs\VisiCutR`,
`%ProgramFiles%\VisiCutR`, `~/.local/bin` und im `PATH`; die Variable
`VISICUTR_BIN` gibt den Pfad fest vor.

**Benutzen:** In Inkscape unter **Erweiterungen → Lasercut Path**:

- **Add to VisiCutR** fügt die Zeichnung als neues Teil zum offenen Projekt
  hinzu (ist keines offen, öffnet sie sich).
- **Open in VisiCutR** ersetzt das Projekt; ungesicherte Änderungen kommen
  wie beim Öffnen in den Zwischenspeicher.

Ist etwas ausgewählt, wird nur die Auswahl übergeben, an ihrer Lage auf der
Seite. Inkscape wandelt vorher Texte, Formen und Klone in Pfade um.

**Eine Instanz:** Läuft VisiCutR schon, übergibt ein weiterer Start seine
Dateien an das offene Fenster, holt es nach vorn und beendet sich. Die
laufende Instanz wartet dazu auf einem freien Port an `127.0.0.1` und legt
Port und ein Zufallskennwort in `instance.json` im Einstellungsordner ab
(nur für den eigenen Benutzer lesbar); andere Benutzer desselben Rechners
haben eigene Instanzen.

```sh
visicutr zeichnung.svg          # öffnen (ersetzt das Projekt)
visicutr --add teil1.svg teil2.dxf  # als Teile hinzufügen
visicutr --update --add teil1.svg   # Teil „teil1“ aktualisieren, sonst hinzufügen
visicutr --demo                 # Beispiel, immer in einem neuen Fenster
visicutr --version              # Version ausgeben
visicutr --laserdevice "LTT iLaser 4000" --material Pappel --thickness 3 teil.svg
visicutr --material Pappel --thickness 3 --execute teil.plf
```

Wie in Java-VisiCut wählen `--laserdevice`, `--material` und `--thickness`
Lasercutter, Material und Dicke (Namen ohne Rücksicht auf Groß- und
Kleinschreibung, die Dicke muss es im Materialprofil geben); die Laserwerte
kommen dann aus dem Materialprofil. `--execute` zeigt nach dem Öffnen gleich
die Sende-Bestätigung; anders als in Java sendet VisiCutR nie ohne
Bestätigung im Fenster. Gab es beim Öffnen einen Fehler, entfällt das Senden.
`--mapping` wird wie in Java nicht unterstützt.

Mehrere Dateien ohne `--add` werden wie beim Ablegen hinzugefügt. Unter
macOS liegt das Programm in `VisiCutR.app/Contents/MacOS/visicutr`.

[← Übersicht](README.md)
