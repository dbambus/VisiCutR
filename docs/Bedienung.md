# Bedienung

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
   Schickt die Inkscape-Erweiterung eine Zeichnung erneut, aktualisiert sie
   das Teil gleichen Namens ebenso, statt ein neues anzulegen.
   **Teil duplizieren** legt eine versetzte Kopie an, **Löschen** entfernt das
   gewählte Teil. Beim Senden werden alle Teile zusammengeführt, je Verfahren
   ein LTT-Auftrag; die Vorschau zeigt alle Teile. Widerrufen nimmt das ganze
   Ablegen zurück. Text, Formen, Kopien und Nesting wirken auf das gewählte
   Teil.
   Verknüpfte Bilddateien (PNG, JPEG, GIF, WebP, SVG) werden dabei relativ zur
   SVG geladen und in das Projekt eingebettet; Webadressen und
   Netzwerkpfade (`\\server\…`, `file://server/…`) werden nicht abgerufen,
   fehlende Bilder werden als Hinweis gemeldet. Ein Bild außerhalb des
   Ordners der SVG wird mit Hinweis samt Pfad eingebettet, damit eine fremde
   SVG nicht unbemerkt andere Dateien ins Projekt holt.
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
   **Stempel-Schulter** (0 … 3 mm, Standard 0 = aus) gibt Stempeln schräge,
   stabile Flanken wie „Stempel/Offset“ im LTT-Treiber. Neben stehenbleibenden
   Flächen steigt die Leistung über diese Breite gleichmäßig an, statt sofort
   voll zu gravieren. Für Stempel das Motiv schwarz zeichnen und „Farben
   invertieren“ und „Spiegeln“ wählen. Die Schulter wirkt mit 3D-Gravur
   (Leistung je Pixel) oder mit Halbton-Rasterung (Punktdichte), nicht mit
   „Schwellwert 50 %“.
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
Parametrische SVG (`.parametric.svg`, `.psvg`) starten mit den
Standardwerten ihrer Parameter bzw. den daneben gespeicherten Werten
(`<datei>.parameters`). Wie in Java-VisiCut zeigt die Seitenleiste für das
gewählte Teil einen Abschnitt **Parameter** mit einem Feld je Parameter
(Zahlen mit `min`/`max`, Auswahllisten, Ja/Nein, Text; Beschriftung aus
`label` bzw. `label_<sprache>`); nach jeder Eingabe wird die Grafik neu
erzeugt, Lage, Maßstab und Zuordnung bleiben. Vorlage und Werte werden mit
dem Projekt gespeichert. Parametrische Teile in VisiCut-Projekten (PLF)
werden beim Öffnen eigene Teile an ihrer Stelle; Lage, Drehung und Maßstab
aus der PLF-Datei bleiben beim Ändern der Parameter erhalten. LaserScript-Dateien (`.ls`) laufen in einer
abgeschotteten JavaScript-Umgebung ohne Datei- und Netzzugriff mit
Zeitlimit (5 s); `prompt`-Abfragen erhalten ihren Vorgabewert. Diese
Skripte, die Ausdrücke parametrischer SVG und das Umwandeln von PDF-Seiten
laufen in einem eigenen Hilfsprozess (`visicutr --import-worker`), der nach
dem Zeitlimit (Skripte 5 s, PDF 60 s) beendet wird und unter Linux
höchstens 2 GB Speicher belegen darf: Eine präparierte Datei kann so
VisiCutR nicht zum Absturz bringen.

## Sprache, Updates und Zwischenspeicher

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

[← Übersicht](README.md)
