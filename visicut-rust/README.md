# VisiCutR für Windows, Linux und macOS

egui-Oberfläche, SVG-Verarbeitung und LTT-Treiber in Rust, auf allen drei
Plattformen gleich. Der Java-Quellbaum bleibt separat.
Erster Ziel-Lasercutter: LTT iLaser 4000 des FAU FabLabs.

## Starten

Die lokal gebaute App befindet sich in `dist/VisiCutR.app`.
Version 0.10.0 benötigt macOS 14 oder neuer, weder Java noch Maven.
Die macOS-App wird nur für Apple Silicon
gebaut und veröffentlicht; Intel-Macs werden nicht unterstützt.

Für einen Build aus dem Quellcode werden Rust 1.90 und die Xcode Command
Line Tools benötigt:

```sh
cd visicut-rust
bash scripts/bundle-macos.sh
open 'dist/VisiCutR.app'
```

Das mitgelieferte `rust-toolchain.toml` wählt Rust 1.90.

```sh
cargo test --lib --locked
```

Das Bundle ist lokal ad hoc signiert. Mit
`MACOS_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"` signiert das
Skript mit einer Developer ID (Hardened Runtime); die CI notarisiert dann
auch, wenn die Secrets hinterlegt sind (siehe README im Hauptverzeichnis).

Zum Entwickeln startet `cargo run --release --bin visicutr` dieselbe
Oberfläche direkt.

## Windows, Linux und GitHub Actions

Alle drei Plattformen nutzen dieselbe egui-Oberfläche (unter Windows und
Linux Kürzel mit Strg statt ⌘) mit denselben Funktionen:
Zoom und Lineale, Einzel- und Regelzuordnung mit Wertvorschlägen,
Materialbibliothek, Parametersätze, Bitmap-Vektorisierung, Schnittreihenfolge,
Text, Formen, Schilder, Kopien und Nesting,
Vorschau mit Zeitschätzung und Zeitslider, Geräte mit Maschinenwerten und
Zeilenversatz-Kalibrierung, Rotationseinheit und Kamera.

Der Workflow **VisiCutR releases** baut drei native Ziele auf GitHub-gehosteten Runnern:

| Ziel | Oberfläche | Download-Artefakt |
| --- | --- | --- |
| Windows x64 / MSVC | egui | ZIP mit EXE |
| Linux x64 / Ubuntu 24.04 | egui, X11/Wayland | tar.gz mit ausführbarer Datei |
| macOS Apple Silicon | egui | ZIP mit `.app` |

Bei Pull Requests und Änderungen an `master`/`main` laufen Rust-Tests auf
allen drei Zielen. Linux prüft zusätzlich Formatierung und Clippy; das
Mac-Ziel baut und signiert das Bundle. TCP-Tests verwenden ausschließlich
localhost. Die Downloads liegen unter **Actions → VisiCutR releases →
Artifacts** (14 Tage Aufbewahrung). Ein Push auf `master` veröffentlicht
nach erfolgreichen Builds auf allen drei Zielen ein vollständiges
[GitHub Release](https://github.com/dbambus/VisiCutR/releases)
`v<Cargo-Version>` mit drei Archiven und `SHA256SUMS.txt`, sofern es für diese
Version noch keines gibt; die Beschreibung ist der passende Abschnitt aus
[CHANGELOG.md](CHANGELOG.md). Weitere Pushes mit derselben Version bauen nur
Artefakte. Für ein neues Release Version in `Cargo.toml`/`Cargo.lock` erhöhen
und einen Changelog-Abschnitt `## <Version> – <Datum>` ergänzen. Tags
`v<Version>` veröffentlichen ebenfalls (Tag und Cargo-Version müssen
übereinstimmen); nur Versionen mit Suffix wie `-rc.1` werden als Vorabversion
markiert. Ein manueller Lauf auf einem Versionstag wiederholt die
Veröffentlichung, auf einem Branch baut er nur Artefakte.
Der macOS-Download heißt
`VisiCutR-<Version>-aarch64-apple-darwin.zip`.
Weitere Workflows gibt es nicht: Die alten Java-Distributionsbuilds und der
Auslöser für den externen VisiCut-Buildserver wurden entfernt.

Für lokale portable Builds aus `visicut-rust`:

```sh
cargo build --locked --release --bin visicutr
cargo test --locked --all-targets
```

Linux benötigt eine grafische Sitzung, OpenGL/EGL, libxkbcommon und einen
Desktop-Portal-Dienst für Dateidialoge. Der CI-Build setzt Ubuntu 24.04 oder
eine kompatible glibc-Laufzeit voraus. Die Pakete enthalten Hinweise zum Start
und zum Funktionsumfang. `scripts/package-ci.py` erstellt dieselben Archive
lokal nach einem Build für die jeweilige Host-Architektur.

### Dateien per Doppelklick öffnen

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


## Workflow

1. Eine SVG über **Öffnen** (⌘O) importieren oder **Beispiel** wählen.
   Dateien lassen sich auch ins Fenster ziehen, mehrere auf einmal: Ist
   schon ein Motiv geladen, kommt jede Datei als eigenes **Teil** in
   Originalgröße rechts neben die vorhandenen (sonst darunter); ebenso
   **Datei → Hinzufügen …** (Strg/⌘+Umschalt+O). Wie die PlfParts in VisiCut
   hat jedes Teil eigene Position, Größe, Drehung und Zuordnung; PLF-Dateien
   behalten ihre Zuordnung. Die Seitenleiste listet die Teile, ein Klick auf
   ein Teil (in der Liste oder auf dem Bett) wählt es, und alle Einstellungen
   für Position, Bearbeitung und Zuordnung gelten für das gewählte Teil.
   **Datei → Neu laden** (⌘R/Strg+R) liest die Dateien aller Teile erneut
   ein, etwa nach einer Änderung in Inkscape; Lage, Maßstab und Zuordnung
   bleiben, in VisiCutR geänderte Objekte werden ersetzt (⌘Z holt sie zurück).
   **Teil duplizieren** legt eine versetzte Kopie an, **Löschen** entfernt das
   gewählte Teil. Beim Senden werden alle Teile zusammengeführt, je Verfahren
   ein LTT-Auftrag; die Vorschau zeigt alle Teile. Widerrufen nimmt das ganze
   Ablegen zurück. Text, Formen, Kopien und Nesting wirken auf das gewählte
   Teil.
   Verknüpfte Bilddateien (PNG, JPEG, GIF, WebP, SVG) werden dabei relativ zur
   SVG geladen und in das Projekt eingebettet; Webadressen werden nicht
   abgerufen, fehlende Bilder werden als Hinweis gemeldet.
   Rasterbilder (PNG, JPEG, BMP, GIF) erhalten ihre Größe aus der
   Auflösung in der Datei (auf ganze DPI gerundet, 2835 px/m sind 72 DPI),
   sonst 72 DPI wie in VisiCut; G-Code (`.nc`,
   `.gcode`) wird als roter Pfad an seiner Position in mm übernommen.
   SVG-Dateien ohne viewBox oder mit Größe in px bekommen ihre Größe wie in
   VisiCut: 90 DPI, ab Inkscape 0.92 (Kopfzeile) 96 DPI, bei
   Illustrator-Exporten 72 DPI; Dateien mit mm-Größe und viewBox sind exakt.
   **PDF** wird ohne Zusatzprogramme direkt gelesen (Seite 1; bei mehreren
   Seiten erscheint ein Hinweis). Pfade bleiben Vektoren mit ihren Füll- und
   Linienfarben und Linienstärken, Text wird als Glyphenkontur übernommen,
   Rasterbilder werden eingebettet; die Größe stammt aus der Crop-/MediaBox
   (1 pt = 25,4/72 mm). Haarlinien (Linienstärke 0) erhalten 0,1 mm.
   Verschlüsselte PDFs werden mit Hinweis abgelehnt. **EPS/PS** wird ohne
   Zusatzprogramm von einem eingebauten PostScript-Interpreter gelesen (Pfade,
   Farben, Linienstärken, Prozeduren und Schleifen; die Größe folgt der
   BoundingBox). Text, Schriften, Bilder und andere Operatoren, die er nicht
   kennt, übergibt er an ein installiertes Ghostscript (`gs`, unter Windows
   `gswin64c`). Ohne Ghostscript erscheint ein Hinweis mit dem betroffenen
   Operator; dann hilft nur, die Datei als SVG oder PDF zu speichern.
   DXF-Dateien (ASCII oder binär) werden in Millimetern übernommen
   (`$INSUNITS`, ohne Angabe Millimeter); DXF-Ebenen und -Farben bleiben als
   Ebenen und Linienfarben für die Zuordnung erhalten.
2. Motiv positionieren, skalieren, drehen oder spiegeln. Liegt es außerhalb
   des Arbeitsbetts, erscheint eine rote Warnung über dem Bett. Mausrad und
   Trackpad-Zoom (Aufziehen) vergrößern um den Mauszeiger, zwei Finger
   verschieben das vergrößerte Bett.
3. **Material** und **Stärke** aus den nativen Auswahlmenüs wählen.
   Die Bibliothek enthält 31 Materialien und 115 Schnitt-, Gravur-, Markier-
   und 3D-Profile aus den öffentlichen FAU-LTT-Einstellungen. Die Profile
   ergeben sich aus dem gewählten Lasercutter; eine eigene Auswahl gibt es
   nicht.
   **Job → Materialbibliothek …** (⇧⌘M) bearbeitet Materialien und Profile,
   importiert/exportiert sie als JSON und stellt die FAU-Bibliothek wieder her;
   eigene Änderungen liegen im Einstellungsordner (siehe unten). Das Profil
   der gewählten Kombination wird automatisch übernommen. Weicht ein Wert
   davon ab, ist er orange markiert; der Kreispfeil ⟳ links vom Feld setzt ihn zurück.
   Beide Werte werden in Prozent angezeigt und eingegeben. Geschwindigkeit
   wird direkt als LTT-Prozentwert übertragen (zulässig: 0,1–100 %).
   Bei fehlendem oder deaktiviertem Profil bleiben die Parameter manuell.
   **Eigenes Material …** erlaubt eine eigene Bezeichnung und Stärke.
4. Unter **Objektzuordnung** zwischen **Gesamt**, **Einzeln** und **Regeln**
   wählen. **Einzeln**: jedem SVG-Objekt **Schneiden** (rot), **Gravieren**
   (blau), **3D-Gravur** (orange), **Markieren** (violett) oder **Ignorieren**
   zuweisen. Die Arbeitsfläche zeigt die Zuordnung sofort in diesen Farben:
   Schnitt- und Markierkonturen als Linien, Gravuren so, wie sie graviert
   werden („Bearbeitung anzeigen“ schaltet zurück auf das Original). **Regeln**: Schritte wählen Objekte nach Farbe, Linien- oder
   Füllfarbe, Linienstärke, Gruppe/Ebene, Typ oder ID; ein Restschritt nimmt
   alles Übrige, Ignorierregeln schließen Objekte aus. Vorlagen enthalten die
   FAU-Zuordnungen („rot schneiden, grün markieren, blau ignorieren, Rest
   gravieren“). Unter **Einstellungen → Job → Regeln und Vorlagen** stehen alle Vorlagen
   in einer Liste (übernehmen, Standard wählen); dort lassen sich
   die Regel-Schritte (Bedingungen, Verfahren, Rest) samt Ignorierregeln unter
   einem Namen sichern, umbenennen, löschen und als **Standard für neue
   Projekte** festlegen; Leistung und Geschwindigkeit kommen weiter aus dem
   Materialprofil. **Aus VisiCut importieren …** übernimmt Zuordnungen
   (`mappings/*.xml`) aus VisiCut-Einstellungen oder einem `.vcsettings`-Archiv.
   Die Vorlagen liegen in `mapping-templates.json` im Einstellungsordner.
   Die Wertliste neben jeder Bedingung zeigt die Werte der SVG.
   Die Liste zeigt
   Objektnummer, SVG-ID bzw. Bezeichnung und direkt gesetzte Farbe.
   Gruppen bleiben mit ihren Transformationen erhalten; Text, Bilder und
   SVG-Instanzen (`use`) zählen jeweils als ein Objekt.
   Ohne Einzelzuordnung gilt das gewählte Verfahren für das gesamte Motiv.
5. Leistung, Geschwindigkeit und Durchgänge je Schritt einstellen; **Weiterer
   Parametersatz** bearbeitet dieselben Objekte anschließend erneut mit anderen
   Werten. Vorhandene FAU-Profile lassen sich übernehmen. **Markieren** fährt
   Konturen mit eigenen Parametern ab; ohne Markierprofil startet die Leistung
   bei 0 % und muss vor der Vorbereitung eingestellt werden.
   **Schneiden** bearbeitet Pfadkonturen als Tangentialkurven und Kreisbefehle
   wie der Java-Treiber, innere Konturen vor den äußeren.
   **Schnittreihenfolge** im Menü **Job**: „Kürzeste Leerfahrten“ (Standard):
   innen zuerst, dann jeweils der nächstgelegene Pfad, beginnend am
   Referenzpunkt oben rechts; deutlich weniger Leerfahrt. „Klassisch“ ist
   VisiCuts Reihenfolge, byte-gleich mit Java; ältere Projekte ohne gespeicherte
   Reihenfolge öffnen damit. Dazu kommen VisiCuts weitere Sortierungen:
   „Wie in der Datei“ (unsortiert), „Nächster Pfad“ (ab dem ersten Pfad
   jeweils der nächste, ohne Vorrang für Löcher) und „Kleinste zuerst“ (nach
   Fläche des Umrissrechtecks). **Doppelte Linien entfernen**
   schneidet Abschnitte, die genau auf einer schon geschnittenen Linie
   desselben Schritts liegen (gemeinsame Kanten, doppelt platzierte Objekte),
   nur einmal; die Schrittbeschreibung nennt, wie viel entfernt wurde. Neue
   Projekte und Dateien ohne diese Angabe schalten es ein, gespeicherte
   Projekte behalten ihre Wahl (Windows/Linux: **Einstellungen → Job**,
   macOS: Seitenleiste).
   **Nur Umriss** (je Schneid- oder Markierschritt, VisiCut „useOutline“)
   schneidet nur den Rand der Fläche, die alle Objekte des Schritts zusammen
   bedecken: Überlappungen und innere Linien entfallen, Löcher der
   Gesamtfläche bleiben, Bilder zählen als Rechteck, reine Linien tragen
   nichts bei. Die Flächen vereinigt die Bibliothek `i_overlay` (reines Rust,
   MIT/Apache-2.0). **Gravieren**
   rastert mit wählbarer Auflösung (166, 200, 250, 333, 500 oder 1000 DPI wie
   im Windows-Treiber, Standard 500; nur 500 DPI ist am Gerät erprobt, alle
   Gravuren eines Auftrags brauchen dieselbe Auflösung) und wählbarem Verfahren (Standard „Halbton
   aufgehellt“ wie im FAU-Profil, außerdem
   Floyd-Steinberg, Halbton, Geordnet, Mittelwert, Raster, Zufall, Schwellwert),
   Helligkeit, Invertierung, bidirektional oder einseitig und von oben oder
   unten. **3D-Gravur** steuert die Leistung je Pixel nach der Helligkeit.
   Text wird beim Schneiden und Markieren entlang der Glyphenumrisse
   bearbeitet (wie in Java). Schriften kommen vom System; fehlen sie, greifen
   die mit egui gelieferten Schriften Ubuntu Light und Hack (auch für
   Vorschau und Gravur). Zeichen ohne passende Schrift werden gemeldet statt
   verworfen.
6. **Vorschau & Zeit** (⇧⌘P) berechnet den Auftrag im Hintergrund. Die Vorschau
   zeigt tatsächliche Schnittkonturen rot und Gravurflächen blau; ignorierte
   Objekte fehlen. Markierkonturen erscheinen violett. Der **Zeitslider**
   zeigt auf dem ganzen Arbeitsbett den Ablauf mit Laserkopfposition,
   Leerfahrten, Rasterzeilen und Durchgängen; jeder Auftrag beginnt wie am
   LTT am Referenzpunkt oben rechts. **Abspielen/Pause**, Anfang/Ende und **1× bis 100×** erlauben
   die Simulation. Beim Verschieben pausiert die Wiedergabe. Bereits
   bearbeitete Bereiche sind kräftig, geplante Bereiche blass dargestellt.
   Angezeigt werden die geschätzte Gesamtdauer, Dauer und
   Parameter je Schritt sowie die Dateigröße. Änderungen verwerfen die alte
   Vorschau und erfordern eine neue Berechnung.
7. Im Vorschaudialog **An Lasercutter senden …** wählen oder LTT exportieren.
   Pro aktivem Verfahren entsteht **ein eigener Auftrag**, maximal vier:
   **Engrav_… → Eng3D_… → Mark_… → Cut_…**. Diese Präfixe stehen auch im Gerätenamen
   (maximal 15 ASCII-Zeichen). Jeder Auftrag erhält eine eigene TCP-Verbindung
   und muss am Gerät separat gestartet werden. Es gibt keinen Autostart.
   Bei einem Fehler wird gestoppt und die App nennt bereits übertragene
   Aufträge; es gibt keine automatische Wiederholung. Beim Export mehrerer
   Aufträge wird ein Zielordner für die einzelnen `.ltt`-Dateien gewählt.
   Mit **Alles in eine Datei** in der Auftragsvorschau entsteht stattdessen ein
   einziger Auftrag **Job_…** mit allen Verfahren wie in Java-VisiCut:
   Gravuren zuerst, dann Markieren und Schneiden. Die Wahl bleibt gespeichert.
8. Mit ⌘S das `.vcr`-Projekt inklusive SVG, Objektzuordnungen und aller
   Parametersätze speichern.

Zum Ausprobieren im Beispiel **Rahmen → Schneiden**, **Gravurkreis → Gravieren**
und **Dreieck → Markieren** wählen; Markierparameter passend zum Material setzen.

Die Dauer ist eine Schätzung aus den erzeugten, quantisierten Fahrwegen,
Geschwindigkeit und Durchgängen. Bei Gravur zählen Rasterzeilen, Overscan,
Rückfahrten und 0,1 s Zusatzzeit je Zeile dazu. Nominalwerte sind 338,677 mm/s
für Schnitt und der Faktor 6,4 für Gravur; Beschleunigung (2000 mm/s²) und
Kurvenplanung folgen dem Java-Treiber. Geräteeinstellungen,
Übertragung und Bedienzeiten sind nicht vollständig modelliert; die Schätzung
ist noch nicht am Gerät kalibriert. Die Simulation ist eine Vorschau ohne
Geräteverbindung und keine Live-Anzeige eines laufenden Lasers.

Standard-Hostname ist `lasercutter2`, Port `9100`. Der Mac muss den Namen im
FabLab-Netz auflösen können; alternativ eine vom FabLab bestätigte IP
eintragen. Die Übertragung erfordert eine ausdrückliche Bestätigung in
der App und startet den Laser nicht automatisch.

Das Projektformat ist ein versioniertes JSON mit eingebetteter SVG.
Material, Stärke, Verfahren, Objektzuordnungen und Parameter werden mitgespeichert.
Neue Projekte verwenden Formatversion 2 mit `speed_percent`.
Projekte aus Version 0.1/0.2 mit `speed_mm_s` werden beim Öffnen entsprechend
dem gespeicherten Verfahren in Prozent umgerechnet. Beim nächsten Sichern
wird das neue Format verwendet; die ursprüngliche Datei wird beim Öffnen
nicht verändert.
Entwürfe können auch ohne Motiv oder mit einer noch unpassenden Position
gesichert werden; das Senden prüft strengere Bedingungen.
VisiCut-Projekte (`.plf`) werden importiert: Alle Teile erscheinen als
benannte Gruppen an ihrer gespeicherten Position und Größe auf dem
Arbeitsbett. Die Zuordnungen der SVG-Teile werden als Bearbeitungsschritte
übernommen: Bedingungen nach Farbe, Linien- und Füllfarbe, Linienstärke, Typ,
ID und Gruppe (auch negiert), Rest und Ignorieren, Verfahren Schneiden,
Markieren, Gravieren (mit Rasterverfahren, Invertierung und
Helligkeitsverschiebung) und 3D-Gravieren. Laser-Einstellungen stehen nicht in
der PLF-Datei, sondern lokal in VisiCut pro Gerät, Material und Stärke. Die
übernommenen Schritte verwenden deshalb 20 % Leistung, 100 % Geschwindigkeit
und einen Durchgang und sind vor dem Senden anzupassen. Nicht übernommen
werden Sortierungen je Teil (die Schnittreihenfolge gilt für
das ganze Projekt), Vektor-Auflösungen außer 500 DPI, Gravurauflösungen, die
der iLaser 4000 nicht kennt (dann 500 DPI), sowie Bedingungen bei Teilen, die
nicht SVG sind (PNG, JPG, DXF, G-Code, PDF); Zuordnungen ohne Bedingung (alle
Objekte, Rest) gelten auch dort, etwa „Bild gravieren“. Der Hinweis beim
Öffnen nennt jeden dieser Fälle und jeden Filter, der unbekannt ist.
**Datei → Als VisiCut-Projekt (PLF) exportieren …** schreibt das Projekt so,
dass Java-VisiCut es öffnet: je Teil eine SVG in Millimetern, ihre Position
und die Zuordnung (Gesamt, Einzeln über Objekt-IDs, Regeln, Rest, Ignorieren;
Profile `cut` und `mark` samt „Nur Umriss“, `engrave` mit Rasterverfahren und Auflösung,
`engrave 3d`). Objekte ohne ID erhalten dafür eine. Leistung, Geschwindigkeit
und Durchgänge enthält das Format nicht; Java-VisiCut nimmt sie aus seinen
Profilen für Material und Stärke. Weitere Parametersätze, „Doppelte Linien
entfernen“ und „Kürzeste Leerfahrten“ (gespeichert als „innen zuerst“) gehen
dabei verloren; der Hinweis nach dem Export nennt das.
Parametrische SVG (`.parametric.svg`, `.psvg`) werden mit den Standardwerten
ihrer Parameter bzw. den in der PLF gespeicherten Werten erzeugt; der Hinweis
nennt die verwendeten Werte. LaserScript-Dateien (`.ls`) laufen in einer
abgeschotteten JavaScript-Umgebung ohne Datei- und Netzzugriff mit
Zeitlimit (5 s); `prompt`-Abfragen erhalten ihren Vorgabewert.

## Inkscape und Befehlszeile

Wie in Java-VisiCut lässt sich eine Zeichnung direkt aus Inkscape (1.0 oder
neuer) an VisiCutR schicken.

**Installieren:** In VisiCutR unter **Einstellungen → Allgemein → Inkscape**
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

## Text, Formen, Schilder, Kopien und Nesting

Das Menü **Objekt** ergänzt das Motiv ohne Umweg über Inkscape:

- **Text …**: eine oder mehrere Zeilen in einer Schrift der App (Liste aller
  installierten und mitgelieferten Schriften), fett/kursiv, links/mittig/
  rechts, Zeilenabstand. Der Text wird sofort in Umrisse umgewandelt und ist
  danach nicht mehr editierbar, dafür auf jedem Rechner gleich.
- **Form …**: Rechteck mit Eckenradius, Ellipse, Vieleck und Stern in mm.
- **Schild …**: Platte mit Eckenradius, 0, 2 oder 4 Löcher, optional ein
  markierter Rahmen und zentrierter Text – alles in einem Schritt.
- Jedes neue Element wird je nach Wahl graviert (schwarz gefüllt), geschnitten
  (rote Kontur) oder markiert (blaue Kontur). In der Einzelzuordnung landet es
  im passenden Schritt; braucht die Gesamtbearbeitung ein zweites Verfahren,
  wechselt das Projekt zur Einzelzuordnung und das bisherige Motiv behält seine
  Werte. Neue Schritte erhalten das Materialprofil.
- **Kopien im Raster …**: Spalten, Zeilen und Abstände; Zuordnungen gelten für
  alle Kopien.
- **Teile verschachteln (Nesting) …**: Die Objekte der obersten Ebene (bzw.
  einer Inkscape-Ebene) werden in der gewünschten Anzahl nach ihren
  Umrissrechtecken auf eine Fläche verteilt (größte zuerst, freie Rechtecke,
  optional um 90° gedreht). Teile, die nicht passen, werden gemeldet. Echte
  Konturverschachtelung ist nicht enthalten. Eine importierte PDF- oder
  EPS-Zeichnung ist eine Gruppe und bleibt dabei zusammen. Gemessen wird ohne Strichbreite,
  also an der Schnittlinie: Bei 0 mm Abstand liegen benachbarte Kanten genau
  aufeinander, und **Doppelte Linien entfernen** wird eingeschaltet, damit sie
  nur einmal geschnitten werden.

**Position und Größe** arbeiten wie in Grafikprogrammen: Der Bezugspunkt
(3 × 3 Kästchen) legt fest, auf welchen Punkt des Motivs sich X/Y beziehen
und welcher Punkt bei Größenänderungen stehen bleibt. Darunter lässt sich das
Motiv um einen Winkel (positiv im Uhrzeigersinn) oder in 90°-Schritten drehen
und horizontal oder vertikal spiegeln; Verzerren gibt es bewusst nicht.
Auf der Arbeitsfläche hat das gewählte Teil Anfasser wie in VisiCut: Ecken
und Kantenmitten ändern die Größe, die gegenüberliegende Seite bleibt stehen
(die Kette bei B/H hält das Seitenverhältnis, Umschalt kehrt das beim Ziehen
um). Der Kreis über der Mitte (bei wenig Platz darunter) dreht das Motiv um
seine Mitte; beim Ziehen zeigt eine Vorschau den Winkel, Umschalt rastet in
15°-Schritten ein, ⌘Z widerruft die Drehung.
Der **Rechtsklick auf das Motiv** bietet Duplizieren, Kopien im Raster,
Nesting, Drehen, Spiegeln, Zentrieren und Löschen. **Entf/⌫** (macOS auch
⌘⌫) löscht das Motiv, sobald das Arbeitsbett aktiv ist; einzelne Objekte
löscht der Rechtsklick in der Objektliste. **Widerrufen** (⌘Z/Strg+Z)
nimmt diese Aktionen nacheinander zurück (bis zu 50 Schritte),
**Wiederholen** (⌘⇧Z/Strg+Umschalt+Z) stellt sie wieder her.

Das bisherige Motiv bleibt dabei unverändert als eingebettetes SVG erhalten,
Objektnummern und Regeln bleiben gültig. Ein SVG mit DTD-Entitäten
(manche Illustrator-Dateien) lässt sich nicht erweitern.

## Lasercutter, Rotationseinheit und Kamera

**Datei → Lasercutter** wählt den aktiven Lasercutter, **Datei →
Einstellungen …** (⌘, bzw. Strg+,; unter macOS auch im App-Menü) verwaltet
mehrere LTT iLaser 4000:
Name, Hostname, Port, Rotationseinheit, Kamera-URL, Kamerakalibrierung,
Nullpunkt unten links und den Hinweis nach dem Senden. Andere
Lasercutter-Typen werden nicht unterstützt. **Nullpunkt unten links** (VisiCut
`originBottomLeft`) lässt Lineale und die Y-Koordinate von unten links zählen
und setzt den Bezugspunkt nach unten links; der Auftrag ändert sich nicht.
Beim ersten Start ist das FAU-Gerät aus den VisiCut-Einstellungen eingerichtet.
Die Auswahl im Inspektor bestimmt Ziel und verfügbare Funktionen.

- **Importieren** liest VisiCut-Gerätedateien (`devices/*.xml`),
  `.vcsettings`-Archive und eigene Exporte (`.vcrdevices`). **Herunterladen**
  lädt die Labor-Einstellungen aus VisiCuts Liste und übernimmt nur
  LTT-iLaser-4000-Geräte. **Exportieren** schreibt alle Geräte als JSON.
- Die Geräteliste liegt unter `~/Library/Application Support/VisiCutR`
  (Windows `%APPDATA%\VisiCutR`, Linux `~/.config/visicutr`);
  `VISICUTR_CONFIG_DIR` setzt einen anderen Ordner.
- **Maschine**: Zeilenversatz-Tabelle für die bidirektionale Gravur
  (10 … 100 %, 1/4000 Zoll), Kurvenbeschleunigung (LibLaserCut 2000 mm/s²
  oder experimentell „Firmware 0,1 g“) und Verweilzeit am Konturstart für die
  Zeitschätzung. VisiCut-Gerätedateien liefern Tabelle und Beschleunigung mit.
- **Zeilenversatz kalibrieren**: Testmuster öffnen und gravieren, je
  Geschwindigkeit den Block mit den geradesten Kanten wählen und in die
  Tabelle übernehmen (siehe [PROTOCOL.md](PROTOCOL.md)).
- **Rotationseinheit**: Bei Geräten mit Rotationseinheit lässt sie sich je Projekt mit
  Werkstückdurchmesser einschalten (siehe [PROTOCOL.md](PROTOCOL.md)).
- **Autofokus, Druckluft, Absaugung** steuert der LTT-Treiber nicht – auch
  nicht im Java-Original. Der Gerätehinweis erinnert nach dem Senden daran.
- **Kamera** (⌘K): Das Foto der Kamera-URL wird über die Homographie der
  Kalibrierung entzerrt und unter das Arbeitsbett gelegt (⇧⌘K aktualisiert).
  Das FAU-Kamerabild ist nur im FAU-Netz erreichbar.
- **Kamera-Einstellungen** (Einstellungen → Lasercutter → Kamera, aufgebaut
  wie VisiCuts Dialog): **Suchen** neben der Kamera-URL findet VisiCam-Kameras
  im lokalen Netz (UDP-Rundsendung „VisiCamDiscover“ an Port 8888, drei
  Sekunden); eine wird direkt übernommen, mehrere stehen zur Wahl. Benutzer und
  Passwort gehen als HTTP-Basic-Anmeldung mit, nur wenn beides gesetzt ist.
  Unter macOS liegt das Passwort im Schlüsselbund, unter Windows und Linux in
  der Geräteliste; Exporte lassen es immer weg. **Aktualisierung (ms)** (VisiCut
  „Zeiten (ms) Kamera“) holt bei eingeblendetem Bild nach jeder Pause (mindestens
  50 ms) ein neues, 0 lädt nur beim Einblenden und mit ⇧⌘K; nach Fehlern
  mit wachsendem Abstand ab 10 s. VisiCut-Geräte mit Zeiten 0 (dort 50 ms)
  bleiben hier beim Einzelbild.
- **Kalibrieren**: Im Kalibrierdialog die Kalibrierseite als Projekt öffnen
  und auf Restmaterial markieren (je Marker ein 10-mm-Kreuz mit
  Zählstrichen wie in VisiCut). Danach ein Foto aufnehmen, die nummerierten
  Marker auf die Kreuze ziehen und übernehmen. Mindestens vier Punkte, nicht
  auf einer Linie. Neue Kalibrierungen nutzen ein Raster aus 5 × 3 Kreuzen.
  „Kreuze erkennen“ setzt die Punkte auf die Kreuzmitten; mit vorhandener
  Kalibrierung geschieht das nach dem Foto automatisch. Im Foto lässt sich
  zoomen (Zwei-Finger-Zoom, ⌘/Strg + Mausrad) und mit den Pfeiltasten
  feinjustieren.
- **Objektiv**: Ab 6 Kreuzen bestimmt die Kalibrierung die radiale
  Verzeichnung (k1, ab 10 Kreuzen auch k2). Das Profil „Raspberry Pi Camera
  v2“ (IMX219, 3,04 mm) liefert Bildmitte, Brennweite und Startwert.
- **Passmarken**: Runde Zielscheiben-Aufkleber (Ring mit Punkt) am Rahmen
  werden beim Kalibrieren erkannt und gespeichert. Spätere Kamerabilder
  werden daran nachgeführt, wenn sich die Kamera z. B. mit dem Deckel bewegt.
- **Kameraserver**: `visicutr-cameraserver` ersetzt `tools/cameraserver` und
  beantwortet jede HTTP-Anfrage mit einem frischen Foto eines
  Aufnahmebefehls, z. B.
  `visicutr-cameraserver -- gphoto2 --capture-image-and-download --stdout` oder
  `visicutr-cameraserver --output snap.jpg -- imagesnap -q snap.jpg`
  (`--port`, `--rotate 90`). Er liegt in `VisiCutR.app/Contents/MacOS`
  bzw. neben der Windows-/Linux-Programmdatei.

Windows und Linux bieten dieselben Funktionen in der egui-Oberfläche mit
Menüleiste Datei, Bearbeiten, Darstellung und Job.

Beide Oberflächen gibt es auf Deutsch, Englisch, Französisch, Italienisch
und Niederländisch (**Darstellung → Sprache**), einschließlich der Meldungen
des Rust-Kerns und der Importfilter. Ohne Auswahl folgt die Sprache dem
System; nicht unterstützte Systemsprachen ergeben Englisch. Neu angelegte
Standardwerte (Projektname, „Eigenes Material“, „Neues Material“, Schildtext,
Namen neuer Formen) entstehen in der gewählten Sprache; bereits gespeicherte
Werte bleiben unverändert. Unter Windows und Linux listet **Datei → Zuletzt
geöffnet** die zehn zuletzt geöffneten oder gesicherten Dateien; Sprache und
Liste stehen in `app-settings.json` im Einstellungsordner neben der
Geräteliste. Die macOS-App speichert die Sprache in den Benutzereinstellungen;
der UI-Test läuft immer auf Deutsch.

**Updates**: Beide Oberflächen fragen höchstens alle 14 Tage beim Start bei
GitHub nach dem neuesten Release von `dbambus/VisiCutR`
(`api.github.com/repos/dbambus/VisiCutR/releases/latest`). Ist es neuer als
die laufende Version, erscheint ein Hinweis mit den Release-Notizen und einem
Link zur Download-Seite; installiert wird nichts automatisch. Abschalten unter
**Einstellungen → Allgemein** (macOS) bzw. im Einstellungsfenster (Windows,
Linux); dort und im App-Menü (macOS) lässt sich auch sofort prüfen. Fehler der
automatischen Prüfung erscheinen nur in der Statuszeile.

Ungesicherte Änderungen fragen nicht nach dem Sichern (beide Oberflächen):
Beim Öffnen, bei **Neu** und beim Beenden legt die App den Stand im Ordner
`cache` des Einstellungsordners ab (höchstens zehn, ältere werden gelöscht)
und führt ihn als „Name (ungesichert).vcr“ unter **Zuletzt geöffnet**. Der beim
Beenden abgelegte Stand öffnet beim nächsten Start wieder. Ein Zwischenstand
hat keinen eigenen Speicherort: **Sichern** fragt nach dem Ort und löscht danach
den Zwischenstand. Nur wenn das Ablegen scheitert, kommt die frühere Rückfrage.
**Ablage/Datei → Zwischenspeicher leeren …** löscht nach einer Bestätigung alle
abgelegten Stände; gesicherte Projekte und das geöffnete Projekt bleiben.

Übersetzungen: Der deutsche Text ist der Schlüssel (`tr("…")`, mit
Platzhaltern `tr!("… {name}", name = …)`); die Tabellen liegen in
`resources/i18n/<sprache>.json` und werden in den Rust-Kern einkompiliert.
Ein Test prüft, dass jeder Text aus `src/` in jeder Sprache mit denselben
Platzhaltern übersetzt ist und keine veralteten Einträge bleiben. `cargo test --lib i18n::tests::export_missing -- --ignored`
schreibt fehlende Texte nach `target/i18n/missing-<sprache>.json`.

## Entwicklung

`src/lib.rs` ist der Rust-Kern, `src/main.rs` die egui-Oberfläche. Die
frühere native macOS-Oberfläche (SwiftUI) ist entfernt; das macOS-Bundle
enthält dieselbe egui-Oberfläche wie Windows und Linux.

`tests/java_parity.rs` vergleicht die erzeugten LTT-Aufträge Byte für Byte
mit Referenzen aus dem originalen Java-VisiCut; neu erzeugt werden sie mit
`scripts/java-parity/generate.sh` (siehe `scripts/java-parity/README.md`).

Die Materialdaten sind in `resources/materials.json` eingebettet, die
Profile der anderen FAU-Lasercutter in `resources/profiles/`.
Originaldateien liegen unter `reference/fau-settings`.
Der Import lässt sich mit `scripts/import-fau-materials.py` aus einem
lokalen Checkout der FAU-Einstellungen wiederholen. 0 mm bedeutet in
den FAU-Profilen „nicht festgelegt“ und wird entsprechend angezeigt.
Profile mit 0 % Leistung werden nicht als übernehmbare Presets angeboten.

Details und fehlende Funktionen: [PROTOCOL.md](PROTOCOL.md).
Lizenz: LGPL-3.0-or-later, siehe `../COPYING.LESSER`.
