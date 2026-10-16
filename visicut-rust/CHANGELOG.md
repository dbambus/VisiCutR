# Änderungen an VisiCutRust

Jede Version hat einen Abschnitt `## <Version> – <Datum>`; der Release-Workflow
übernimmt ihn als Beschreibung des GitHub-Releases.

## 0.19.0 – 16.10.2026

- **Zeichnung größer als die Zeichenfläche** (egui): Ragt eine importierte
  Zeichnung über die Zeichenfläche ihrer Datei hinaus, fragt die App, ob alles
  oder nur die Zeichenfläche übernommen wird. Beim Import mehrerer Dateien,
  auch als zusätzliche Teile, gilt eine Antwort für alle.
- **Windows-Oberfläche**: Das Mausrad zoomt die Arbeitsfläche, solange der
  Zeiger darüber steht. Die farbige Bearbeitungsvorschau wird in der
  angezeigten Größe gerechnet, dünne Linien bleiben sichtbar. Die Titelleiste
  ist hell wie die Oberfläche; Position und Größe passen in die Seitenleiste.
- **Sende-Dialog** (egui und macOS): Der allgemeine Hinweis unter den
  Aufträgen entfällt; Warnungen zum Auftrag bleiben.
- Formatierung und Clippy-Hinweis behoben, damit der Release-Workflow wieder
  durchläuft.

## 0.18.0 – 15.10.2026

- **Kamera wie in VisiCut** (Einstellungen → Lasercutter → Kamera, egui und
  macOS): **Suchen** findet VisiCam-Kameras im lokalen Netz; **Kamera-Benutzer**
  und **Kamera-Passwort** werden als HTTP-Basic-Anmeldung gesendet;
  **Aktualisierung (ms)** holt bei eingeblendetem Kamerabild laufend neue
  Bilder (mindestens 50 ms, nach Fehlern mit wachsendem Abstand).
- **Passwörter geschützt**: unter macOS im Schlüsselbund statt in der
  Geräteliste; Exporte lassen sie immer weg. VisiCut-Geräte bringen Benutzer,
  Passwort und Zeiten beim Import mit.

## 0.17.0 – 15.10.2026

- **Weitere Schnittreihenfolgen aus VisiCut** (Menü **Job →
  Schnittreihenfolge**, egui und macOS): „Wie in der Datei“, „Nächster Pfad“
  und „Kleinste zuerst“, portiert aus LibLaserCut. „Kürzeste Leerfahrten“
  bleibt Standard, „Klassisch“ unverändert byte-gleich mit Java.
- **Anfasser auf der Arbeitsfläche** wie in VisiCut: Ecken und Kantenmitten
  ändern die Größe (Kette hält das Seitenverhältnis, Umschalt kehrt das um),
  der Kreis über dem Motiv dreht es um seine Mitte mit Vorschau und Gradzahl
  (Umschalt: 15°-Schritte, ⌘Z widerruft).
- **Gravurauflösung je Schritt**: 166, 200, 250, 333, 500 oder 1000 DPI wie im
  Windows-Treiber des iLaser 4000 (`ESC D` als Teiler von 4000 DPI, nur für
  Gravurzeilen). Standard bleibt 500 DPI, byte-gleich mit Java; andere
  Auflösungen sind am Gerät noch nicht erprobt und werden mit Hinweis
  gesendet. Gravuren eines Auftrags brauchen dieselbe Auflösung.
- **Nur Umriss** (VisiCut „useOutline“) für Schneid- und Markierschritte:
  geschnitten wird nur der Rand der gemeinsamen Fläche aller Objekte;
  Flächenvereinigung mit `i_overlay` (MIT/Apache-2.0).
- **Als VisiCut-Projekt (PLF) exportieren** (Menü Datei): SVG je Teil in mm,
  Position und Zuordnung (Gesamt, Einzeln über IDs, Regeln, Rest, Ignorieren)
  mit den FAU-Profilnamen, damit Java-VisiCut seine Laserwerte findet.
- **PLF-Import**: übernimmt Gravurauflösungen und „Kontur“ (Nur Umriss);
  Zuordnungen ohne Bedingung gelten jetzt auch für Bilder, DXF und PDF.
- **Oberfläche**: In „Einzeln“ erscheinen nur Verfahren mit zugeordneten
  Objekten, die Objektliste wächst bis fünf Zeilen mit; Tooltips für
  Helligkeit, Invertieren, Bidirektional und Von unten nach oben.

## 0.16.0 – 15.10.2026

- **Kürzeste Leerfahrten ist Standard**: Die Schnittreihenfolge steht im Menü
  **Job** (egui und macOS) ohne „experimentell“; VisiCuts Reihenfolge heißt
  „Klassisch“. Ältere Projekte ohne gespeicherte Reihenfolge öffnen klassisch.
- **Alles in eine Datei** (Schalter in der Auftragsvorschau, bleibt
  gespeichert): ein einziger Auftrag `Job_…` mit allen Verfahren wie in
  Java-VisiCut, Gravuren zuerst; der Jobmodus wird nur bei einem Wechsel
  gesendet. Die Vorschau lädt beim Umschalten nicht mehr sichtbar neu.
- **Klarere Fehlermeldungen**: „Kein Material zum Lasern gewählt“ beim Senden
  und Exportieren; bei 0 % Leistung nennt die Meldung das Verfahren.
- **Arbeitsfläche zeigt die Bearbeitung**: Schnitt- und Markierkonturen in
  ihren Farben, Gravuren so, wie sie graviert werden; „Bearbeitung anzeigen“
  schaltet zurück auf das Original.
- **Hervorheben unter der Maus**: Objekte auf der Arbeitsfläche werden gelb
  umrandet und in der Objektliste markiert (und umgekehrt). Die Auswahl hat
  einen gestrichelten schwarzen Rahmen statt des blauen.
- **Raster und Lineale passen sich dem Zoom an** (Schritte 1–2–5) und nennen
  die Einheit; „Zurücksetzen“ statt „Einpassen“. Mausrad-Zoom funktioniert
  durchgehend (macOS).
- **Nesting**: Gravuren werden nebeneinander gelegt (weniger Gravurzeilen,
  der Laser graviert zeilenweise in X); gemessen wird an der Schnittlinie, bei
  0 mm Abstand schneidet „Doppelte Linien entfernen“ gemeinsame Kanten nur
  einmal. Importierte PDF- und EPS-Zeichnungen bleiben eine Gruppe.
- **PDF-Import** schneidet die Seite auf den sichtbaren Inhalt zu; der
  Seitenrand gilt nicht mehr als Motivrahmen.
- **Regeln**: Die Vorlagen „Alles schneiden/gravieren/…“ entfallen (dafür gibt
  es „Gesamt“); Farbwerte zeigen ein Farbfeld im Menü. Schritte haben ➕
  (Kopie anlegen) und ein umschaltbares Verfahren.
- **Einzeln**: Es erscheinen nur Verfahren, denen Objekte zugeordnet sind;
  die Objektliste wächst bis fünf Zeilen mit und scrollt darüber.
- **Menüs** (macOS): Symbolleiste mit Öffnen, Sichern, Vorschau, Export und
  Senden; **Datei → Lasercutter** wählt das Gerät, **Datei → Einstellungen …**
  (⌘,) verwaltet die Geräte. Der Projektname wird per Doppelklick im Kopf der
  Arbeitsfläche geändert. Rasteroptionen haben erklärende Tooltips.

## 0.15.0 – 14.10.2026

- **Profile folgen dem Lasercutter** (egui und macOS): Die Auswahl „Profile“
  entfällt; Material und Stärke kommen aus den Profilen des gewählten
  Lasercutters. Der Epilog-Zing-Katalog ist entfernt, da nur LTT-Geräte
  eingerichtet werden können.
- **Stärke nur bei Bedarf**: Materialien ohne echte Stärke zeigen keine
  Stärke-Zeile; bis zu drei Stärken erscheinen als eigene Knöpfe, mehr
  (Acryl) weiterhin im Menü.
- **Position und Größe wie in Grafikprogrammen** (egui und macOS):
  Bezugspunkt, X/Y und B/H mit der Einheit im Feld, rechts die Kette für das
  Seitenverhältnis; Drehen und Spiegeln als Symbole, der Winkel gilt mit ⏎
  oder ✓.
- **macOS-Oberfläche**: Das Bezugspunkt-Feld reagiert wieder auf Klicks
  (alle Klickflächen lagen oben links). Lange Material- und Gerätenamen
  werden in einer Zeile mit „…“ gekürzt. Der Kopf der Arbeitsfläche ist nicht
  mehr verblasst und angeschnitten. Das Bett lässt sich nicht mehr über den
  Ursprung oder den Rand hinaus verschieben und zeichnet nicht mehr über
  seinen Bereich hinaus; die Linealzahl 1000 hat Platz.

## 0.14.0 – 14.10.2026

- **Kein „Änderungen sichern?“ mehr** (egui und macOS): Beim Öffnen, bei
  **Neu** und beim Beenden legt die App ungesicherte Stände im Ordner `cache`
  des Einstellungsordners ab (höchstens zehn) und führt sie als
  „Name (ungesichert).vcr“ unter **Zuletzt geöffnet**. Der beim Beenden
  abgelegte Stand öffnet beim nächsten Start wieder. **Sichern** fragt bei
  einem Zwischenstand nach dem Ort und löscht ihn danach.
  **Datei → Zwischenspeicher leeren …** löscht nach Bestätigung alle Stände.
- **Auftragsvorschau zeigt das ganze Arbeitsbett** (1000 × 600 mm, egui und
  macOS) statt eines Ausschnitts um den Auftrag; macOS zeichnet den Bettrand.
- **Startpunkt oben rechts**: Der LTT fährt zur Referenz nach oben rechts.
  Jeder Gerätejob beginnt in Simulation und Zeitschätzung dort (1000, 0 mm);
  „Kürzeste Leerfahrten“ sucht den ersten Pfad ab dieser Position. Die
  gesendeten Daten mit der Reihenfolge „Wie VisiCut“ ändern sich nicht.
- **Verbindung im lokalen Netz unter macOS**: Das App-Bundle erklärt den
  Zugriff aufs lokale Netzwerk (`NSLocalNetworkUsageDescription`), damit macOS
  danach fragt. Verbindungsfehler nennen Adresse und Systemfehler; blockiert
  macOS den Zugriff, verweist die Meldung auf Systemeinstellungen →
  Datenschutz & Sicherheit → Lokales Netzwerk.

## 0.13.0 – 12.10.2026

- **Mehrere Teile** (egui und macOS) wie die PlfParts in VisiCut: Jede
  hinzugefügte Datei ist ein eigenes Teil mit eigener Position, Größe,
  Drehung und Zuordnung (Gesamt, Einzeln, Regeln; PLF-Dateien behalten ihre
  Zuordnung). Teile-Liste in der Seitenleiste, Klick auf ein Teil wählt es,
  **Datei → Hinzufügen …** (Strg/⌘+Umschalt+O), Ablegen mehrerer Dateien,
  **Teil duplizieren** und **Löschen** des gewählten Teils. Beim Senden werden
  alle Teile zusammengeführt, weiterhin ein LTT-Auftrag je Verfahren; die
  Vorschau zeigt alle Teile. Ältere Projekte öffnen unverändert als ein Teil.
- **Nullpunkt unten links** (VisiCut `originBottomLeft`, egui und macOS):
  Geräteoption, wird aus VisiCut-Geräten übernommen; Lineale und die
  Y-Koordinate zählen dann von unten links, der Bezugspunkt steht unten
  links. Nur Anzeige, der Auftrag ändert sich nicht.
- **Zuletzt geöffnet auch unter macOS**: **Ablage → Zuletzt geöffnet** listet
  die zehn zuletzt geöffneten oder gesicherten Dateien.

## 0.12.0 – 12.10.2026

- **Sprachen** (egui und macOS): Oberfläche, Meldungen des Rust-Kerns und der
  Importfilter auf Deutsch, Englisch, Französisch, Italienisch und
  Niederländisch; Auswahl unter **Darstellung → Sprache**, ohne Auswahl gilt
  die Systemsprache. Neu angelegte Standardwerte (Projektname, „Eigenes
  Material“, „Neues Material“, Schildtext, Namen neuer Formen) entstehen in
  der gewählten Sprache. Ein Test stellt sicher, dass jeder Text in jeder
  Sprache mit denselben Platzhaltern übersetzt ist.
- **Zuletzt geöffnet** (egui): **Datei → Zuletzt geöffnet** listet die zehn
  zuletzt geöffneten oder gesicherten Dateien; Sprache und Liste stehen in
  `app-settings.json` im Einstellungsordner.
- **Oberfläche wie unter macOS** (egui): Werkzeugleiste mit Öffnen, Sichern,
  Vorschau, Export und Senden statt Knöpfen am Ende der Seitenleiste;
  Seitenleiste in der Reihenfolge der macOS-App (Projekt, Rotationseinheit,
  Material, Objektzuordnung, Bearbeitung, Position und Größe) als Karten mit
  Bezeichnung links und Feld rechts; segmentierte Zuordnungswahl, farbige
  Verfahrenswahl, ein Abschnitt je Schritt; Vorschauleiste mit Zeitschätzung
  und Legende; Startansicht ohne Motiv wie unter macOS; besser lesbare
  Hinweistexte und Farbtabelle für die Regelvorlage „Farben“.

## 0.11.0 – 11.10.2026

- **Mehrere Dateien per Drag-and-drop hinzufügen** (egui und macOS): Ist schon
  ein Motiv geladen, kommen abgelegte Grafiken in Originalgröße rechts daneben
  (sonst darunter) hinzu, statt das Motiv zu ersetzen. Ohne Motiv öffnet die
  erste Datei, die übrigen werden angehängt. Bestehende Objekte behalten
  Nummern und Zuordnungen; bei Einzelzuordnung nennt ein Hinweis die noch
  nicht zugeordneten neuen Objekte. Gleichnamige IDs und CSS-Klassen (z. B.
  `cls-1` aus Illustrator) werden umbenannt. Widerrufen nimmt das ganze
  Ablegen zurück; nicht lesbare Dateien werden gemeldet.
- **Profile anderer Lasercutter**: Dropdown „Profile“ über dem Material
  wechselt zwischen den FAU-Profilen des LTT iLaser 4000 und des Epilog Zing
  (42 Materialien, 128 Profile, nur lesbar, mit Hinweis zur Prüfung am LTT).

## 0.10.0 – 11.10.2026

- **Absturz behoben** beim Wechsel zwischen Gesamt, Einzeln und Regeln
  (macOS): entfernte Schritte und Bedingungen werden nicht mehr über einen
  veralteten Index gelesen.
- **Nesting** funktioniert mit Kopien im Raster (z. B. 5 × 5 Kopien, 1 mm
  Abstand): Teile werden nach ihren sichtbaren Umrissen angeordnet, und jedes
  Teil wird nur einmal eingebettet statt mit dem ganzen Dokument.
- **Materialprofil ist der Standard**: Es wird bei Material, Stärke und
  Verfahren automatisch übernommen. Abweichende Werte sind orange markiert
  und lassen sich links vom Feld zurücksetzen.
- **Regeln** erklärt; neue erste Vorlage „Rot schneiden, Blau gravieren, Grün
  markieren“ (#ff0000, #0000ff, #00ff00).
- **Bezugspunkt** als Rahmen mit acht verbundenen Quadraten und freiem
  Mittelpunkt.
- **Arbeitsbett**: rote Warnung über dem Bett, wenn das Motiv nicht passt;
  Rasterschalter auch auf macOS; Zoom mit Mausrad und Trackpad um den
  Mauszeiger, Verschieben mit zwei Fingern.
- Dropdowns rechtsbündig mit sichtbarem Menüpfeil; „Drehachse“ heißt jetzt
  „Rotationseinheit“; „Auf dem Arbeitsbett zentrieren“ und die Hinweise am
  Ende der Seitenleiste entfallen.

## 0.9.0 – 11.10.2026

- **Transformieren** wie in Grafikprogrammen: Bezugspunkt (3 × 3) für X/Y
  und Größenänderungen, Drehen um einen beliebigen Winkel, 90°-Schritte,
  horizontal und vertikal spiegeln (ohne Verzerren).
- **Rechtsklick auf das Motiv**: Duplizieren, Kopien im Raster, Nesting,
  Drehen, Spiegeln, Zentrieren und Löschen.
- **Löschen**: Entf/⌫ (macOS auch ⌘⌫) entfernt das Motiv, „Objekt löschen“
  im Kontextmenü der Objektliste ein einzelnes Objekt; Zuordnungen rücken
  nach.
- **Widerrufen** (⌘Z/Strg+Z) für Löschen, Drehen, Spiegeln, Duplizieren und
  die Objekt-Werkzeuge (ein Schritt).
- **Duplizieren** (⌘D/Strg+D) legt eine Kopie rechts daneben oder darunter.
- macOS: Der Inspektor scrollt nicht mehr unter die Fensterknöpfe.

## 0.8.0 – 11.10.2026

- **Text und Formen ohne Inkscape** (Menü „Objekt“): Text mit den Schriften
  der App (fett, kursiv, mehrzeilig, Ausrichtung), Rechteck mit Eckenradius,
  Ellipse, Vieleck und Stern, je zum Gravieren, Schneiden oder Markieren.
  Text wird in Umrisse umgewandelt und daher überall gleich geschnitten.
- **Schild-Assistent**: Platte mit Eckenradius, 0/2/4 Befestigungslöchern,
  optionalem markiertem Rahmen und zentriertem Text in einem Schritt.
- **Kopien im Raster**: Spalten, Zeilen und Abstände; Zuordnungen gelten für
  alle Kopien.
- **Einfaches Nesting**: Teile (Objekte der obersten Ebene bzw. einer
  Inkscape-Ebene) in gewünschter Anzahl nach Umrissrechtecken auf eine
  Fläche verteilt, größte zuerst, optional um 90° gedreht; nicht passende
  Teile werden gemeldet.
- Das bestehende Motiv bleibt dabei unverändert eingebettet; Objektnummern,
  Einzelzuordnungen und Regeln bleiben gültig. Neue Schritte erhalten das
  Materialprofil.
- Releases: Jede neue Version auf `master` wird als vollständiges Release
  (nicht als Vorabversion) mit diesem Abschnitt veröffentlicht. Die
  macOS-App kann mit einer Developer ID signiert und notarisiert werden,
  sobald die Zertifikats-Secrets hinterlegt sind.

## 0.7.0 – 11.10.2026

- Maschinenwerte je Lasercutter: Zeilenversatz-Tabelle, Kurvenbeschleunigung
  und Verweilzeit am Konturstart; Import aus VisiCut-Gerätedateien.
- Zeilenversatz-Testmuster mit Übernahme des besten Blocks in die Tabelle.
- Experimentelle Kurvenplanung mit der Firmware-Rampe 0,1 g.
- Genauere Zeitschätzung (Verweilzeit je Kontur, Gravurfaktor 6).
- Firmware-Analyse des iLaser 4000 in PROTOCOL.md.

## 0.6.0 – 11.10.2026

- Gleicher Funktionsumfang in der macOS- und der Windows/Linux-Oberfläche:
  Menüleiste, Vorschau mit Zeitslider, Materialbibliothek, Einzel- und
  Regelzuordnung mit Wertvorschlägen.
- Option „Doppelte Linien entfernen“.

## 0.5.0 – 10.10.2026

- Funktionsgleichheit mit VisiCut Java: Bitmap-Vektorisierung,
  PLF-Zuordnungen, Clipping, Schnittreihenfolge „innen zuerst“,
  eingebauter EPS-Interpreter.
- Byte-genauer Vergleich mit LibLaserCut in 14 Testfällen.
- Experimentelle Schnittreihenfolge „Kürzeste Leerfahrten“.
