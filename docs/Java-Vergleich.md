# Byte-Vergleich mit Java-VisiCut

`generate.sh` schickt jeden Fall aus `tests/java_parity/` durch die originale
Java-Pipeline: VisiCuts SVG-Import, `VectorProfile`/`RasterProfile`/
`Raster3dProfile` und LibLaserCuts `LaserToolsTechnicsCutter` mit dem
FAU-Gerät aus `reference/FAU-LTT-iLaser-4000.xml`. Das Ergebnis landet als
`expected.ltt` neben dem Fall. `cargo test --test java_parity` vergleicht die
Rust-Ausgabe damit Byte für Byte; Java wird dafür nicht gebraucht.

```sh
bash scripts/java-parity/generate.sh             # alle Fälle
bash scripts/java-parity/generate.sh cut_curves  # einzelne Fälle
REBUILD=1 bash scripts/java-parity/generate.sh   # Java neu bauen
cargo test --locked --release --test java_parity  # mit Varianten (variants=); ohne --release nur die Grundfälle
```

Benötigt werden Java 17 oder neuer, Maven, Git und Zugriff auf Maven Central.
Der Java-Quellbaum liegt nicht mehr auf `master`: Der Tag `java-reference`
enthält den Stand, mit dem die Referenzen erzeugt sind (VisiCut 1.9-SNAPSHOT,
t-oster/VisiCut c967f127, mit LibLaserCut 90ccd5f1 als Submodul).
`generate.sh` legt beim ersten Lauf daraus einen Worktree unter
`$TMPDIR/visicut-java-parity/java` an, holt das Submodul und baut LibLaserCut und
VisiCut (`mvn initialize` installiert die kabeja-Jars aus `legacy/`).
`JAVA_TREE=<Ordner>` nimmt stattdessen einen anderen Java-Checkout, etwa einen
der Java-Fix-Branches `fix/*`; nach einem Wechsel `REBUILD=1` setzen. VisiCut läuft
ohne Oberfläche mit eigenem `user.home` im Temp-Ordner und liest keine
Benutzereinstellungen.

## Ein Fall

`case.properties` beschreibt den Auftrag für beide Seiten:

| Schlüssel | Bedeutung |
| --- | --- |
| `name` | Projektname; der Gerätename ist wie in Rust `<Präfix>_<name>`, höchstens 15 Zeichen |
| `svg` | Eingabedatei im Fallordner |
| `operation` | `cut`, `mark`, `engrave` oder `engrave3d` |
| `power`, `speed`, `passes` | Parameter in Prozent bzw. Durchgänge |
| `additional` | weitere Parametersätze `Leistung:Tempo:Durchgänge;…` |
| `thickness_mm` | Materialstärke; VisiCut addiert sie zum Fokus |
| `x_mm`, `y_mm`, `width_mm`, `height_mm` | Lage und Größe der SVG auf dem Bett |
| `dither` | Klassenname aus `liblasercut.dithering`, z. B. `FloydSteinberg` |
| `invert`, `color_shift`, `unidirectional`, `bottom_up` | Rasteroptionen |
| `compare=header` | nur bis zur ersten Rasterzeile vergleichen (Grund im Kommentar) |
| `scale` | Teil einheitlich skalieren wie in der VisiCut-Oberfläche (Rust: `width_mm`/`height_mm` entsprechend setzen) |
| `rust_file` | Datei, die Rust statt `svg` liest (etwa eine PDF, während Java eine geometrisch gleiche SVG bekommt); Nicht-SVG-Dateien laufen auf beiden Seiten durch die Importer (DXF, EPS, PNG/JPG/BMP, G-Code) |
| `place=bbox` | linke obere Ecke der Geometrie (ohne Konturbreiten) statt des Dokumentursprungs auf `x_mm`/`y_mm` legen, wie beim Platzieren in der Oberfläche; nötig für DXF, EPS und PDF |
| `order` | VisiCut-Reihenfolge `FILE`, `NEAREST`, `INNER_FIRST` (Standard, „Klassisch“) oder `SMALLEST_FIRST` |
| `rotary_diameter_mm` | Rotationsachse mit diesem Walzendurchmesser |
| `variants` | weitere Läufe desselben Falls mit überschriebenen Schlüsseln, `order=FILE, rotary_diameter_mm=100`; Referenzen `expected@order-FILE.ltt`, Abweichungen `deviation@order-FILE=`; andere Zeichen als Buchstaben, Ziffern, `-`, `_` und `.` werden im Namen zu `_` (`additional=30:80:1` → `@additional-30_80_1`) |
| `mappings` | Zuordnungen in Reihenfolge, `[Verfahren:] Filter -> Leistung:Tempo[:Durchgänge]; …` (Verfahren `cut`, `mark`, `engrave`, `engrave3d`, sonst `operation`); Filter `*` (alles Übrige), `all` (alles) oder mit `&` verbundene `[!]Attribut=Wert` bzw. `[!]Attribut<=Wert` mit VisiCuts Attributnamen (`Color`, `Stroke Color`, `Fill Color`, `Stroke Width`, `Type`, `Id`, `Group`) |
| `outline=true` | „Nur Umriss“ (VisiCuts `useOutline`) |
| `svg=…plf` | VisiCut-Projekt: Lage, Teile und Zuordnungen aus der Datei (`x_mm`/`y_mm` = 0); geschrieben mit `make-plf.sh` (Java `MakePlf.java`), nicht von `generate.sh` |
| `settings` | Laserwerte der PLF-Zuordnungen mit Profil in Teil- und Zuordnungsreihenfolge, `Leistung:Tempo[:Durchgänge]; …` |
| `single_job=true` | alle Verfahren in einem Auftrag (Rust `prepare_with(…, true)`, Name `Job_…`) |
| `expect=reject` | beide Seiten müssen den Auftrag ablehnen (Java: `expected.error`), etwa bei Gravuren über den Bettrand |
| `deviation` | dokumentierte Abweichung mit Grund: wird gemeldet, lässt den Test nicht scheitern; der Test weist darauf hin, wenn der Fall wieder byte-gleich ist |

Ohne `width_mm`/`height_mm` bestimmen beide Seiten die Größe selbst aus der SVG
(Java: `determineResolution`, Rust: usvg mit 96 DPI). Kann Java einen Fall nicht
verarbeiten, schreibt `generate.sh` den Grund nach `expected.error` statt
`expected.ltt`; der Test meldet das als Abweichung.

Abweichungen meldet der Test mit dem ersten abweichenden Byte und legt die
Rust-Ausgabe unter `target/tmp/<fall>.ltt` ab.

## Grenzen

- Rust erzeugt pro Verfahren einen eigenen Auftrag, Java einen gemeinsamen;
  jeder Fall enthält daher genau ein Verfahren.
- Java2D und tiny-skia rastern Kanten verschieden (ohne Antialiasing nach
  Pixelmitte mit eigener Rundung, mit Antialiasing mit anderer
  Teilabdeckung). Rasterfälle legen ihre Kanten deshalb auf ganze
  500-DPI-Pixel, Lage und Größe sind in `f32` exakt (127 mm = 2500 px), und
  ein Hintergrund um eine Viertelpixel versetzt sorgt dafür, dass beide
  Seiten den Rasterbereich gleich abschneiden.
- „Zufall“ ist in Java ungeseedet und wird nicht verglichen.
- Bewusste Abweichungen von Java stehen im [LTT-Protokoll](LTT-Protokoll.md).
- Das Audit vom 2026-10-21 ([Bericht](audits/Java-Vergleich-2026-10-21.md)) vergleicht 249 Grenzfälle,
  darunter 59 Importe (PNG/JPEG/BMP/GIF, G-Code, DXF, EPS, PDF), und begründet jede
  Abweichung; die Gründe stehen auch als `deviation=` in den Fällen.
- Das Audit vom 2026-10-22 ([Bericht](audits/Java-Vergleich-2026-10-22.md)) wiederholt den Vergleich und prüft
  zusätzlich die neun Java-Fix-Branches (`fix/*`) einzeln und gemeinsam: 22 Fälle werden
  allein durch die Java-Fixes byte-gleich.
- Das Audit vom 2026-10-28 ([Bericht](audits/Java-Vergleich-2026-10-28.md)) prüft VisiCutR 0.23.0 und ergänzt
  11 Fälle für Leistungen unter 0,1 % und den Bettrand; seitdem muss wie in Java nur
  die Zeichnung im Bett liegen, nicht die ganze Seite. Im Nachtrag laufen die Reihenfolgen
  FILE, NEAREST, SMALLEST_FIRST und die Rotationsachse als 570 Varianten mit.

## Stand

878 Vergleiche (Fälle und Varianten): 772 byte-gleich, 4 lehnen beide Seiten
gleich ab, 102 weichen begründet ab (`deviation=` im Fall, Klassen im
[Bericht vom 2026-10-28](audits/Java-Vergleich-2026-10-28.md)). Geprüft sind
SVG-Geometrie, Einheiten und Transformationen, Striche, Schnittreihenfolgen
(Klassisch, Wie in der Datei, Nächster Pfad, Kleinste zuerst), Parametersätze,
Gravur mit allen Rasterverfahren außer „Zufall“, 3D-Gravur (Kopf), Importe
(Bilder, G-Code, DXF, EPS, PDF), die Rotationseinheit (50, 100, 300 mm),
Zuordnungen, „Nur Umriss“, gemischte Verfahren, VisiCut-Projekte (PLF) sowie
VisiCuts Beispiele für parametrische SVG und LaserScript.

Nicht geprüft: PDF mit Text oder Bildern, Bilder außer PNG und parametrische
Teile in einer PLF, die Grenze von 100 Umdrehungen auf der Walze und mehrere
Teile aus VisiCutR-Projekten. Nicht vergleichbar: andere Gravurauflösungen als
500 DPI (Javas LTT-Treiber kennt nur diese) und Javas „Doppelte Pfade löschen“
(VisiCutRs „Doppelte Linien entfernen“ arbeitet auf Segmenten).

Die PLF-Fälle schreibt Java-VisiCut selbst mit `scripts/java-parity/make-plf.sh`
(`MakePlf.java`); es nutzt denselben Java-Baum wie `generate.sh`.

[← Übersicht](README.md)
