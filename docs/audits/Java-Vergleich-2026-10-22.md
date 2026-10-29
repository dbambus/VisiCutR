# Audit: Byte-Gleichheit VisiCutR („Klassisch“) gegenüber Java-VisiCut, mit und ohne die Java-Fix-Branches

Stand 2026-10-22, VisiCutR 1.0.0-rc.2 (Commit c3deb581), Java-VisiCut 1.9-SNAPSHOT
(t-oster/VisiCut master c967f127) mit LibLaserCut 90ccd5f1 (Submodul-Stand dieses Commits),
FAU-Gerät `reference/FAU-LTT-iLaser-4000.xml`, Java 21.0.13, Maven 3.10.0. Vorgänger:
`AUDIT-2026-10-21.md` (gleiche 249 Fälle, nur Java master).

Neu gegenüber dem Vorgänger: Die neun Java-Fix-Branches, die als Pull Requests für
t-oster/VisiCut vorbereitet sind (je ein Commit auf c967f127, im Fork dbambus/VisiCutR),
werden einzeln und gemeinsam mitgeprüft. `fix/eps-boundingbox` wurde während des Audits
nach Befund 2 überarbeitet (Commit e6c1de4e); die Zahlen gelten für diesen Stand.

## Ergebnis in einem Satz

Von **249 Fällen** (190 SVG-Fälle, 59 Importe) sind gegen Java master **173 byte-gleich**
(76 Abweichungen: 68 verschiedene Ausgaben, 6 lehnt Rust ab, 2 kann Java nicht lesen);
mit **allen neun Fix-Branches** auf der Java-Seite sind **195 byte-gleich** (54 Abweichungen),
**22 Fälle werden allein durch die Java-Fixes identisch**, kein bisher identischer Fall kippt,
und die Java-Ausgabe ändert sich ausschließlich in den 27 Fällen, die die Fixes betreffen.
Auf der Rust-Seite war nichts zu ändern: Die 21 Fälle waren Abweichungen, bei denen Java
von der SVG-Spezifikation, der G-Code-Norm, dem DXF-Kopf oder dem EPS-DSC abwich und
VisiCutR schon richtig lag. Die verbleibenden 55 Abweichungen sind dieselben Klassen wie im
Vorgänger: 7 nicht vergleichbare Fälle (Ablehnungen), 14 weitere svgSalamander-Eigenheiten,
13 Bild-Resamplings, 6 Rasterkanten, 4 Schriften, 4 Präzisionsgrenzen (Bogen→Bézier, f32-Lage,
PDF-Kurve), 3 DXF, 2 EPS, 1 PDF-Clip.

## Vorgehen

* Fälle, Harness und Vergleich wie im Vorgänger: `tests/java_parity/<fall>/` mit
  `case.properties`, `scripts/java-parity/generate.sh` bzw. `LttParity.java` für die
  Java-Seite, `cargo test --test java_parity` für VisiCutR in der Reihenfolge **Klassisch**
  (`CutOrder::VisiCut`, doppelte Linien nicht entfernt, 500 DPI); `deviation=` in
  `case.properties` kennzeichnet dokumentierte Abweichungen, der Test meldet, sobald eine
  wieder byte-gleich ist.
* **Basis:** LibLaserCut 90ccd5f1 und VisiCut c967f127 wurden frisch gebaut und alle
  249 Referenzen neu erzeugt; sie sind mit den eingecheckten `expected.ltt`/`expected.error`
  byte-gleich (Java-Seite reproduzierbar). `cargo test` gegen diese Referenzen:
  76 dokumentierte Abweichungen, keine undokumentierte.
* **Fixes einzeln:** Für jeden der neun Branches wurde ein eigenes Jar gebaut und alle
  249 Referenzen erzeugt; verglichen wurde, welche Fälle sich gegenüber der Basis ändern.
* **Fixes gemeinsam:** Alle neun Branches wurden auf c967f127 zusammengeführt (ein Konflikt:
  beide EPS-Branches ändern denselben Javadoc-Kommentar in `EPSImporter.java`, inhaltlich
  unabhängig), ein Jar gebaut, alle Referenzen erzeugt und `cargo test` gegen diese
  Referenzen gefahren (die Referenzen im Baum danach wieder auf den eingecheckten Stand
  gesetzt). Die Einzel-Jars zeigen, welcher Branch welchen Fall verändert; bei jedem
  veränderten Fall ist die Ausgabe des Einzel-Jars gleich der des Gesamt-Jars, außer bei
  den beiden EPS-Fällen, die beide EPS-Branches brauchen. Der überarbeitete
  `fix/eps-boundingbox` wurde allein (alle Fälle) und zusammen mit
  `fix/eps-page-background` (die zehn EPS-Fälle) gebaut und geprüft.
* Verbleibende Abweichungen der veränderten Fälle wurden bis zum ersten abweichenden Byte
  zerlegt (Kopf mit Bounding Box, Vektorbefehle, Rasterzeilen); der EPS-Befund wurde mit
  drei abgewandelten EPS-Dateien experimentell bestätigt (Befund 2).

## Befunde

### 1. Was die neun Fix-Branches bringen

| Branch (Commit) | Änderung in Java | Veränderte Fälle | Byte-gleich mit Rust |
| --- | --- | --- | --- |
| `fix/svg-dasharray` (a276a450) | ungerade `stroke-dasharray` verdoppelt, Summe 0 oder negative Werte → durchgezogen (vorher Endlosschleife/OutOfMemoryError), Flachheit nicht mehr auf 1 px angehoben | `cut_dash_odd`, `cut_dash_zero` | 2 von 2 |
| `fix/svg-hidden-elements` (b091721b) | `display="none"`, `visibility="hidden"`, `opacity="0"` auch als Attribut und vererbt | `cut_hidden` | 1 von 1 |
| `fix/svg-viewbox-aspect` (b6ced211) | `preserveAspectRatio` (meet/slice, Ausrichtung) beachten, `x`/`y` des Wurzelelements ignorieren | `cut_viewbox_aspect_meet`, `cut_viewbox_xminymin`, `cut_root_xy` | 3 von 3 |
| `fix/svg-salamander-workarounds` (d0b7d8e6) | `skewX`/`skewY` als `matrix()`, `href` ohne `xlink:` an `use`/`image`, Einheit `pc` → `pt`, jeweils vor dem Parsen umgeschrieben | `cut_skew`, `cut_defs_use`, `cut_units_pc` | 3 von 3 |
| `fix/gcode-import` (6772074e) | `G0`/`G1`/`G2`/`G3` ohne führende Null, Kleinbuchstaben, Kommentare, `G91`, `I`/`J` relativ (`G90.1` absolut), `R`-Bögen | `import_gcode_short_form`, `import_gcode_comments`, `import_gcode_relative_g91`, `import_gcode_arc_rel_ij`, `import_gcode_arc_r` | 5 von 5 |
| `fix/dxf-insunits` (2c477030) | `$INSUNITS` skaliert die Grundtransformation | `import_dxf_insunits_inch` | 1 von 1 |
| `fix/eps-page-background` (21e126f2) | weißes Seitenrechteck des FreeHEP-Interpreters aus dem SVG entfernt | alle 10 `import_eps_*` | 5 von 10 allein (`lines`, `fill_only`, `procedure_loop`, `linewidth_zero`, `dash`); 6 mit `fix/eps-boundingbox` |
| `fix/eps-boundingbox` (e6c1de4e, nach Befund 2 überarbeitet) | `%%BoundingBox` als Eckpunkte, Seite genau in Größe der ganzzahligen Box, HiResBoundingBox als Ursprung der Grafik | `import_eps_bbox_offset`, `import_eps_hires_bbox` | 0 von 2 allein (Seitenrahmen bleibt); mit `fix/eps-page-background` beide (Befund 2) |
| `fix/image-resolution` (b1a45605) | PNG/JPEG/BMP-Auflösung aus den ImageIO-Metadaten, auf ganze DPI gerundet | `import_png_grey_phys300` | 0 von 1: Lage und Zeilenzahl jetzt gleich, Zeilen durch Resampling verschieden (Befund 3) |

Summe: 22 neu byte-gleiche Fälle. Kein Branch verändert einen Fall, der ihn nicht betrifft,
und keiner macht einen bisher identischen Fall abweichend (Java-Ausgabe aller anderen
222 Fälle unverändert).

### 2. EPS: FreeHEP passt die ganzzahlige BoundingBox selbst in die Seite ein

`import_eps_hires_bbox` hat `%%BoundingBox: 10 10 151 81` und
`%%HiResBoundingBox: 10.5 10.5 150.5 80.5` (Dreieck 140 × 70 pt). Die erste Fassung des
Branches (6d07e4cb) bildete die Seite aus der HiRes-Box (140 × 70 pt, aufgerundet) und blieb
abweichend: Bounding Box im LTT 7668 × 3834 statt 7778 × 3889 Einheiten (Rust 49,39 × 24,69 mm
wie die Datei), also gleichmäßig um 70/71 gestaucht. Aus dem Bytecode von
`org.freehep.postscript.PSDevice`/`Processor` (freehep-psviewer 2.0) ergibt sich der
Mechanismus: FreeHEP liest die **ganzzahlige** `%%BoundingBox` selbst (als
`java.awt.Rectangle`, daher der `NullPointerException` bei Dezimalwerten), skaliert sie
gleichmäßig mit `min(Seitenbreite/Boxbreite, Seitenhöhe/Boxhöhe)` in die übergebene Seite,
legt die untere linke Ecke der Box an die Seitenecke, spiegelt y an der Seitenhöhe und ersetzt
die Transformation des `Graphics2D` durch seine eigene. VisiCuts bisherige Verschiebung
`setTransform(translate(−x, −y))` war deshalb schon immer wirkungslos, und beim alten
Lesefehler (x2/y2 als Breite/Höhe) entstand der Maßstab `min(x2/(x2−x1), y2/(y2−y1))`.

Experiment mit dem Gesamt-Jar vor der Überarbeitung:

| Datei | Java-Bounding-Box | Ergebnis |
| --- | --- | --- |
| wie im Fall (Box 10 10 151 81, HiRes 10.5 … 80.5) | 7668 × 3834 | weicht ab |
| ganzzahlige Box auf 10 10 150 80 gesetzt | 7778 × 3889 | wie Rust |
| HiRes-Zeile entfernt | 7778 × 3889 | wie Rust |
| `%%BoundingBox` mit Dezimalwerten, ohne HiRes | NullPointerException in FreeHEP (`bb is null`) | Java-Fehler, mit und ohne Fix |

Überarbeitung (e6c1de4e): Die Seite ist genau so groß wie die ganzzahlige Box, damit der
Maßstab 1 bleibt; die wirkungslose Verschiebung entfällt; eine HiResBoundingBox verschiebt
die Grundtransformation der importierten Grafik so, dass die genaue Ecke der Zeichnung zum
Ursprung wird (x: `hx1 − x1`, y: `y2 − hy2`, weil y in PostScript nach oben läuft). Damit ist
`import_eps_hires_bbox` zusammen mit `fix/eps-page-background` byte-gleich; allein verändert
der Branch wie zuvor nur die beiden BoundingBox-Fälle und keinen anderen. Dezimalwerte in
`%%BoundingBox` (laut DSC nicht erlaubt) bringen FreeHEP unabhängig vom Fix zum Absturz.

### 3. Bilder: nach dem DPI-Fix bleibt nur das Resampling

Mit `fix/image-resolution` liest Java die 300 DPI der pHYs-Angabe wie Rust;
`import_png_grey_phys300` hat damit dieselbe Lage und 53 statt 222 Rasterzeilen auf beiden
Seiten. Die Zeilen selbst weichen weiter ab (1631 Bytes), aus demselben Grund wie bei
allen 72-DPI-Bildern: Java rendert mit `RENDER_QUALITY` (bilinear), Rust nearest-neighbour.
Byte-gleiche Bildgravuren gibt es nur, wenn Bild- und Gravurauflösung übereinstimmen
(`engrave_png_gradient` u. a.); mit dem Fix ist das in Java jetzt für 500-DPI-Bilder
erreichbar, vorher nie.

### 4. Die übrigen 54 Abweichungen (unverändert gegenüber dem Vorgänger)

| Klasse | Fälle | Begründung |
| --- | --- | --- |
| Ablehnung statt Ausgabe (7) | `cut_all_empty`, `cut_image_in_cut`, `cut_sets_26_groups`, `engrave_white_only`, `engrave_colorshift_255`, `import_png_cut`; `import_gif_grey` (Java) | Rust lehnt leere Aufträge, Rasterbilder beim Schneiden und mehr als 16 Parametersätze ab; Java hat keinen GIF-Importer |
| svgSalamander/Java2D weicht von der Spezifikation ab (14) | `cut_clip_rect`, `cut_marker_stroke`, `cut_nested_svg`, `cut_use_symbol`, `cut_dash_units`, `cut_dash_scaled`, `cut_dash_basic`, `cut_dash_curve`, `cut_dash_closed`, `cut_no_paint`, `cut_stroke_opacity_zero`, `cut_rect_rounded`, `cut_rect_rx_only`, `cut_rect_rx_clamped` | clipPath, Marker, verschachteltes `svg`, `symbol`-viewBox, Einheiten und Transformation in `stroke-dasharray`, float-Strichlängen, Flachheit 1 px gestrichelter Formen, unsichtbare Formen geschnitten, Startpunkt abgerundeter Rechtecke; keiner dieser Punkte ist in einem Fix-Branch enthalten |
| Präzisionsgrenzen (4) | `cut_arc_path`, `cut_relative_cmds`, `cut_fractional_position`, `import_pdf_curves` | Bogen→Bézier-Zerlegung usvg vs. svgSalamander, f32-Lage im Projekt, Kontrollpunkte aus der PDF in f32 |
| Schriften (4) | `cut_text`, `engrave_text`, `import_dxf_text`, `import_eps_text` | Systemschriften und Glyphenkonturen |
| Rasterkanten und Bildresampling in SVGs (6) | `engrave_rotated_45`, `engrave_circle`, `engrave_bg_offgrid`, `engrave_png_scaled`, `engrave_png_offgrid`, `engrave3d_full_compare` | Java2D vs. tiny-skia/resvg; 3D-Zeilen wie in PROTOCOL.md |
| Bildimport (13) | `import_png_*` (10 außer `cut`), `import_jpeg_*` (2), `import_bmp_grey` | Resampling auf 500 DPI (Befund 3); bei JPEG dazu der Decoder, bei 16 Bit die Umrechnung, bei RGBA die Alpha-Verrechnung; `phys72`: 72,009 DPI vor dem pHYs-Rundungsfix in Rust, jetzt nur noch Resampling |
| DXF (3) | `import_dxf_spline`, `import_dxf_insert_block`, `import_dxf_layers_colors` | SPLINE-Näherung, INSERT-Verarbeitung, Reihenfolge nach Ebenen |
| EPS (2) | `import_eps_curves`, `import_eps_transform` | ein Abflachungspunkt, eine Einheit in der Bounding Box |
| PDF (1) | `import_pdf_clip` | Java kennt clipPath nicht |

## Nicht abgedeckt

Wie im Vorgänger: Der Harness vergleicht je Fall ein Verfahren mit Gesamtzuordnung. Nicht
geprüft wurden Rotationseinheit, mehrere Teile und PLF-Dateien, LaserScript, parametrische
SVG, Zuordnung nach Farbe oder Regeln, „Nur Umriss“, gemischte Verfahren, die Reihenfolgen
FILE/NEAREST/SMALLEST_FIRST, andere Auflösungen als 500 DPI, PDF mit Text oder Bildern und
„Zufall“-Dithering. Die Fix-Branches wurden nur über diese 249 Fälle geprüft; dass sie die
Java-Ausgabe sonst nirgends ändern, gilt für diese Fälle, nicht für beliebige Dateien
(etwa `fix/svg-hidden-elements` bei vererbter `opacity` in verschachtelten Gruppen oder
`fix/gcode-import` bei Vollkreisen mit `R`).

## Empfehlungen

1. Die neun Branches als Pull Requests einreichen; sie bringen 22 Fälle auf byte-gleich
   und ändern sonst nichts. `fix/eps-boundingbox` ist lokal überarbeitet (e6c1de4e) und muss
   im Fork noch mit `--force` nachgeschoben werden (origin steht auf 6d07e4cb).
2. Sobald die Fixes in t-oster/VisiCut sind und der Fork sie übernommen hat: Referenzen mit
   `generate.sh` neu erzeugen, die 22 `deviation=`-Einträge entfernen (der Test nennt sie),
   die vier Einträge von `import_eps_curves`, `import_eps_text`, `import_eps_transform` und
   `import_png_grey_phys300` auf die neuen Gründe kürzen.
3. Auf der Rust-Seite ist aus diesem Audit nichts zu ändern. Will man weitere Fälle
   byte-gleich bekommen, bleiben nur Java-Änderungen (clipPath, Marker, verschachteltes
   `svg`, `symbol`, Striche) oder bewusste Nachbildungen von Java-Eigenheiten in Rust
   (Startpunkt abgerundeter Rechtecke, Flachheit gestrichelter Formen), die den Vorteil
   der Spezifikationstreue aufgeben würden.

## Alle Fälle

Spalten: Ergebnis gegen Java master (ohne Fixes) und gegen Java mit allen neun Fix-Branches
(mit Fixes). „(Fix)“ heißt: byte-gleich allein durch die Java-Änderung; die Rust-Ausgabe ist
in beiden Läufen dieselbe.

### A Geometrie-Primitive und Pfadbefehle (ohne Fixes 36/42, mit Fixes 36/42 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `cut_rect` | Rectangle cut once; straight lines only. | ✅ identisch | ✅ identisch |  |
| `cut_circle` | Closed circle: LTT circle command (PJ PB) in Java. | ✅ identisch | ✅ identisch |  |
| `cut_curves` | Cubic Béziers: tangent curves with speed planning. | ✅ identisch | ✅ identisch |  |
| `cut_line_el` | A single open <line> element. | ✅ identisch | ✅ identisch |  |
| `cut_polyline_el` | Open <polyline> zigzag with sharp corners (curveOrLine splits). | ✅ identisch | ✅ identisch |  |
| `cut_polygon_el` | <polygon> pentagon, closed implicitly. | ✅ identisch | ✅ identisch |  |
| `cut_star_selfintersect` | Self-intersecting star path (overlapping segments, no fill rule). | ✅ identisch | ✅ identisch |  |
| `cut_ellipse` | <ellipse> with rx != ry: flattened Béziers, no circle command. | ✅ identisch | ✅ identisch |  |
| `cut_ellipse_round` | <ellipse> with rx == ry: should take the circle command like <circle>. | ✅ identisch | ✅ identisch |  |
| `cut_circle_tiny` | Circle r=0.5 mm (few flattened points: circle detection off?). | ✅ identisch | ✅ identisch |  |
| `cut_circle_r3` | Circle r=3 mm, a typical small hole. | ✅ identisch | ✅ identisch |  |
| `cut_circle_large` | Circle r=120 mm: above the 101 mm circle limit, sent as joint curves. | ✅ identisch | ✅ identisch |  |
| `cut_circle_r101` | Circle r=101 mm: exactly at the circle-command limit. | ✅ identisch | ✅ identisch | behoben: px→mm in Javas Reihenfolge |
| `cut_circle_r100_9` | Circle r=100.9 mm: just below the limit. | ✅ identisch | ✅ identisch |  |
| `cut_circle_r102` | Circle r=102 mm: just above the limit. | ✅ identisch | ✅ identisch |  |
| `cut_circle_speed_limit` | Small circle at full speed: speed and power reduced for the circle command. | ✅ identisch | ✅ identisch |  |
| `cut_rect_rounded` | <rect> with rx=ry=5 (corner arcs). | ❌ weicht ab | ❌ weicht ab | Startpunkt abgerundeter Rechtecke: usvg beginnt oben nach rx, Java2D links unten |
| `cut_rect_rx_only` | <rect> with rx only (ry defaults to rx). | ❌ weicht ab | ❌ weicht ab | Startpunkt abgerundeter Rechtecke: usvg beginnt oben nach rx, Java2D links unten |
| `cut_rect_rx_clamped` | <rect> with rx larger than half the width (must be clamped). | ❌ weicht ab | ❌ weicht ab | Startpunkt abgerundeter Rechtecke: usvg beginnt oben nach rx, Java2D links unten; Begrenzung von rx auf die halbe Breite anders |
| `cut_arc_path` | Path with A commands, all four flag combinations. | ❌ weicht ab | ❌ weicht ab | Bogen→Bézier-Umwandlung: usvg und svgSalamander zerlegen A-Bögen in andere Bézier-Stücke, daher andere Abflachungspunkte |
| `cut_arc_rotated` | Elliptical arc with x-axis-rotation 30. | ✅ identisch | ✅ identisch |  |
| `cut_arc_degenerate` | Arc with too small radius (scaled up per spec) and zero radius (a line). | ✅ identisch | ✅ identisch |  |
| `cut_arc_full_round_trip` | Two half arcs forming a full circle (circle detection on arcs). | ✅ identisch | ✅ identisch |  |
| `cut_quadratic` | Q and T commands (quadratic Béziers, smooth continuation). | ✅ identisch | ✅ identisch |  |
| `cut_relative_cmds` | Only relative commands: m l h v c s q t a z. | ❌ weicht ab | ❌ weicht ab | Bogen→Bézier-Umwandlung: usvg und svgSalamander zerlegen A-Bögen in andere Bézier-Stücke, daher andere Abflachungspunkte |
| `cut_implicit_cmds` | Implicit linetos after M, repeated args, compact separators, scientific notation. | ✅ identisch | ✅ identisch |  |
| `cut_multi_subpath` | One path with three subpaths: closed, open, closed via relative m after z. | ✅ identisch | ✅ identisch |  |
| `cut_close_then_continue` | Segment after Z starts at the subpath start point without a new M. | ✅ identisch | ✅ identisch | behoben: Kontur läuft nach Z ohne Abheben weiter |
| `cut_zero_length` | Duplicate consecutive points and zero-length segments. | ✅ identisch | ✅ identisch |  |
| `cut_only_moveto` | Path with lone M commands beside a normal rect (empty shape ignored?). | ✅ identisch | ✅ identisch |  |
| `cut_all_empty` | Only empty paths: nothing to cut on either side. | ⛔ Rust lehnt ab | ⛔ Rust lehnt ab | Java schreibt einen leeren Auftrag, Rust lehnt ab |
| `cut_tiny_shape` | Rect of 0.03 mm (sub pixel) beside a normal rect: ShapeConverter drops its lines. | ✅ identisch | ✅ identisch |  |
| `cut_tiny_only` | Only a sub-pixel rect: zero-length closed element. | ✅ identisch | ✅ identisch |  |
| `cut_subpixel_steps` | Polyline with 0.01 mm steps: lineto dropped unless the truncated pixel changes. | ✅ identisch | ✅ identisch | behoben: viewBox-Maßstab exakt aus Breite/Höhe in mm |
| `cut_collinear` | Polyline with collinear intermediate points (angle 0, straight-run filtering). | ✅ identisch | ✅ identisch |  |
| `cut_small_angles` | Polyline with 0.5°, 2° and 10° bends: curve-vs-line thresholds. | ✅ identisch | ✅ identisch |  |
| `cut_long_spiral` | Spiral polyline with 3000 points (reinterpolation, long speed planning). | ✅ identisch | ✅ identisch | behoben: Transformationen in f64 statt f32 |
| `cut_near_straight_curve` | Cubic that is almost straight (flatness just around 0.2 px). | ✅ identisch | ✅ identisch |  |
| `cut_curve_cusp` | Cubic with a cusp and one with a loop. | ✅ identisch | ✅ identisch |  |
| `cut_curve_tiny` | Tiny cubic of 0.2 mm beside a rect. | ✅ identisch | ✅ identisch |  |
| `cut_curve_huge` | Cubic spanning 500 mm (hits the 10-level subdivision limit?). | ✅ identisch | ✅ identisch | behoben: Transformationen in f64 statt f32 |
| `cut_curve_two_passes` | Curves with two passes (joint curves repeated). | ✅ identisch | ✅ identisch |  |

### B Transformationen, viewBox, Einheiten, Platzierung (ohne Fixes 25/31, mit Fixes 30/31 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `cut_group_nested` | Nested groups with translate, scale and rotate. | ✅ identisch | ✅ identisch |  |
| `cut_rotate45` | Rect rotated by 45° about its centre. | ✅ identisch | ✅ identisch |  |
| `cut_skew` | Rect with skewX(20). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-salamander-workarounds` |
| `cut_mirror` | Mirrored group: scale(-1,1) with translate (orientation reversed). | ✅ identisch | ✅ identisch |  |
| `cut_matrix` | Arbitrary matrix() transform. | ✅ identisch | ✅ identisch |  |
| `cut_transform_on_path` | Transform attribute directly on a path with curves. | ✅ identisch | ✅ identisch | behoben: Transformationen in f64 statt f32 |
| `cut_viewbox_scale2` | viewBox twice the size of width/height (0.5 mm per unit). | ✅ identisch | ✅ identisch |  |
| `cut_viewbox_offset` | viewBox with negative origin (-20 -10). | ✅ identisch | ✅ identisch |  |
| `cut_viewbox_aspect_meet` | viewBox aspect differs from width/height (xMidYMid meet: letterboxing or stretch?). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-viewbox-aspect` |
| `cut_viewbox_aspect_none` | preserveAspectRatio=none with a differing aspect (non-uniform scale). | ✅ identisch | ✅ identisch |  |
| `cut_viewbox_xminymin` | preserveAspectRatio=xMinYMin meet with a differing aspect. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-viewbox-aspect` |
| `cut_units_unitless` | Unitless width/height, no viewBox, no editor hint (Java 90 DPI default vs 96 DPI). | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_units_px` | Explicit px units, no viewBox. | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_units_unitless_vb` | Unitless width/height with a matching viewBox. | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_units_inkscape092` | Unitless with an Inkscape 0.92 header (96 DPI in Java). | ✅ identisch | ✅ identisch |  |
| `cut_units_inkscape091` | Unitless with an Inkscape 0.91 header (90 DPI in Java). | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_units_illustrator` | Unitless with an Adobe Illustrator comment (72 DPI in Java). | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_units_in` | Width/height in inches with a viewBox. | ✅ identisch | ✅ identisch |  |
| `cut_units_cm` | Width/height in cm with a viewBox. | ✅ identisch | ✅ identisch |  |
| `cut_units_pt` | Width/height in pt with a viewBox. | ✅ identisch | ✅ identisch |  |
| `cut_units_pc` | Width/height in pc with a viewBox. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-salamander-workarounds` |
| `cut_units_mm_no_vb` | Width/height in mm without a viewBox (user unit = px at which DPI?). | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_no_size` | Only a viewBox, no width/height. | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_percent_size` | Percent width/height with a viewBox (Java: cannot use percent). | ✅ identisch | ✅ identisch | behoben: Dokumentgröße wie VisiCuts determineResolution (90/96/72 DPI) |
| `cut_root_xy` | Root <svg> with x and y attributes (Java reads them into the transform). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-viewbox-aspect` |
| `cut_scaled_placement` | Part scaled to 150 % on the bed (non-integer pixel coordinates). | ✅ identisch | ✅ identisch |  |
| `cut_scaled_odd` | Part scaled to 33.3 % (irrational pixel grid). | ✅ identisch | ✅ identisch |  |
| `cut_at_origin` | Part at the bed origin, shape touching 0/0. | ✅ identisch | ✅ identisch |  |
| `cut_at_bed_corner` | Shape ending exactly at the bed corner 1000/600. | ✅ identisch | ✅ identisch |  |
| `cut_fractional_position` | Position 12.345/67.891 mm (non-integer pixel grid). | ❌ weicht ab | ❌ weicht ab | Lage im Projekt als f32 (12,345 mm ist nicht exakt darstellbar), Java rechnet in double |
| `cut_big` | 900 x 500 mm outline with a diagonal (large coordinates). | ✅ identisch | ✅ identisch |  |

### C Stil, Sichtbarkeit, Referenzen, Striche (ohne Fixes 8/25, mit Fixes 12/25 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `cut_hidden` | display:none, visibility:hidden, opacity:0, a hidden group; one visible rect. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-hidden-elements` |
| `cut_fill_only` | Rect with fill and no stroke. | ✅ identisch | ✅ identisch |  |
| `cut_no_paint` | fill=none and stroke=none (invisible shape) beside a visible rect. | ❌ weicht ab | ❌ weicht ab | Java schneidet Formen ohne Füllung und Kontur, Rust überspringt sie |
| `cut_stroke_opacity_zero` | stroke-opacity=0 and fill-opacity=0 (painted but transparent). | ❌ weicht ab | ❌ weicht ab | Java schneidet voll transparente Formen, Rust überspringt sie |
| `cut_css_style` | Styling through <style> classes and the style attribute. | ✅ identisch | ✅ identisch |  |
| `cut_stroke_width_zero` | stroke-width=0 (still a contour?) beside a normal rect. | ✅ identisch | ✅ identisch | behoben: stroke-width 0 wird als Haarlinie geschnitten |
| `cut_defs_use` | <use> of a rect from <defs> with x/y and a transform. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-salamander-workarounds` |
| `cut_use_symbol` | <symbol> with its own viewBox used at two sizes. | ❌ weicht ab | ❌ weicht ab | Java zeichnet das symbol selbst mit und ignoriert dessen viewBox |
| `cut_clip_rect` | clipPath (rect) on a group with a circle crossing the clip edge. | ❌ weicht ab | ❌ weicht ab | Java ignoriert clipPath, Rust schneidet die Schnittmenge |
| `cut_dash_basic` | stroke-dasharray 4 2 on a straight line. | ❌ weicht ab | ❌ weicht ab | Java DashedIterator rechnet in float: ein Strich 1 px länger |
| `cut_dash_closed` | Dashed rect and circle (pattern across the start point). | ❌ weicht ab | ❌ weicht ab | Java flacht gestrichelte Formen mit Flachheit 1 px ab (DashedShape), Rust 0,2 px |
| `cut_dash_offset` | Dashed line with stroke-dashoffset 3. | ✅ identisch | ✅ identisch |  |
| `cut_dash_odd` | Odd dasharray (5 2 1) repeated per spec. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/svg-dasharray` |
| `cut_dash_scaled` | Dashed path inside scale(2): dash lengths in user space. | ❌ weicht ab | ❌ weicht ab | Java misst Striche nach der Transformation (Dokumenteinheiten), Rust im Nutzerraum des Elements |
| `cut_dash_zero` | dasharray with zero-length dashes (0 2) and all zeros (0 0). | ⛔ Java-Fehler | ✅ identisch (Fix) | behoben durch `fix/svg-dasharray` |
| `cut_dash_curve` | Dashed cubic (Java flattens dashed shapes with flatness >= 1). | ❌ weicht ab | ❌ weicht ab | Java flacht gestrichelte Formen mit Flachheit 1 px ab (DashedShape), Rust 0,2 px |
| `cut_dash_longer_than_path` | Dash longer than the whole path (single dash). | ✅ identisch | ✅ identisch |  |
| `cut_dash_units` | Dasharray with mm units (4mm 1mm). | ❌ weicht ab | ❌ weicht ab | Java ignoriert Einheiten in stroke-dasharray |
| `cut_text` | Text element (fonts differ between Java and Rust; expected to differ). | ❌ weicht ab | ❌ weicht ab | Schriftauflösung: Systemschriften und Glyphenkonturen unterscheiden sich |
| `cut_image_in_cut` | Raster <image> inside a cut job (Java cuts its bounding box, Rust rejects?). | ⛔ Rust lehnt ab | ⛔ Rust lehnt ab | Java schneidet den Rahmen von Rasterbildern, Rust lehnt ab |
| `cut_inkscape_layer` | Inkscape layer group with a transform. | ✅ identisch | ✅ identisch |  |
| `cut_gradient_fill` | Shape filled with a gradient (paint server; cut contour only). | ✅ identisch | ✅ identisch |  |
| `cut_marker_stroke` | Path with markers (markers must not produce contours). | ❌ weicht ab | ❌ weicht ab | Rust schneidet die Marker-Formen mit (usvg), Java ignoriert Marker |
| `cut_nested_svg` | Nested <svg> element with its own viewBox and position. | ❌ weicht ab | ❌ weicht ab | Java ignoriert x/y/width/height/viewBox verschachtelter svg-Elemente |
| `cut_switch_and_a` | <switch> and <a> wrappers around shapes. | ✅ identisch | ✅ identisch |  |

### D Schnittreihenfolge, Verbinden, Parametersätze (ohne Fixes 21/22, mit Fixes 21/22 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `cut_nested` | Inner contours before the outer one, an open line in between. | ✅ identisch | ✅ identisch |  |
| `cut_passes` | Two passes over the same contours. | ✅ identisch | ✅ identisch |  |
| `cut_sets` | Three parameter sets on the same contours, one of them twice. | ✅ identisch | ✅ identisch |  |
| `cut_nested_deep` | Four nesting levels plus siblings and an open line. | ✅ identisch | ✅ identisch |  |
| `cut_identical_twins` | The same rect twice and two passes (ties in the stable sort). | ✅ identisch | ✅ identisch |  |
| `cut_touching_boxes` | Inner rects sharing edges with the outer one (equal bbox extents). | ✅ identisch | ✅ identisch | behoben: Sortierschlüssel ymax, xmax, ymin, xmin |
| `cut_same_bottom_edge` | Several boxes with the same bottom edge but different tops/sides. | ✅ identisch | ✅ identisch | behoben: Sortierschlüssel ymax, xmax, ymin, xmin |
| `cut_open_chain_ring` | Four separate lines forming a rect (joined into a ring) with 0.02 mm gaps. | ✅ identisch | ✅ identisch |  |
| `cut_join_tolerance` | Line ends 0.04 mm (0.79 px) and 0.05 mm (0.98 px) apart: join tolerance 0.9 px. | ✅ identisch | ✅ identisch |  |
| `cut_join_manhattan` | Ends 0.03/0.03 mm apart diagonally (Manhattan 1.18 px > 0.9, Euclid 0.84 px < 0.9). | ✅ identisch | ✅ identisch |  |
| `cut_fork` | Three lines meeting at one point (fork: nothing joined). | ✅ identisch | ✅ identisch |  |
| `cut_chain_reversed` | Chain of lines drawn in alternating directions (join needs inversion). | ✅ identisch | ✅ identisch |  |
| `cut_line_twice_passes` | A single line with two passes (joined there and back). | ✅ identisch | ✅ identisch |  |
| `cut_line_three_passes` | A single line with three passes (odd number of copies to join). | ✅ identisch | ✅ identisch |  |
| `cut_sets_two_diff_speed` | Two parameter sets differing only in speed. | ✅ identisch | ✅ identisch |  |
| `cut_sets_equal_values` | Additional set with the same values as the main one (same Java property). | ✅ identisch | ✅ identisch |  |
| `cut_sets_14_groups` | Fourteen distinct parameter sets (HashMap resize beyond 12 entries). | ✅ identisch | ✅ identisch |  |
| `cut_sets_26_groups` | 26 distinct parameter sets (second HashMap resize; fractional values). | ⛔ Rust lehnt ab | ⛔ Rust lehnt ab | Rust begrenzt auf 16 Parametersätze je Schritt |
| `cut_sets_thickness` | Three sets with material thickness 2.5 (focus in the hash). | ✅ identisch | ✅ identisch |  |
| `cut_sets_thickness_frac` | Thickness 0.1 (binary-inexact float in the focus hash). | ✅ identisch | ✅ identisch |  |
| `cut_sets_open_lines` | Open lines across two sets: joins only within a set. | ✅ identisch | ✅ identisch |  |
| `cut_many_small_parts` | 60 small squares in a grid (ordering sweep across many equal boxes). | ✅ identisch | ✅ identisch |  |

### E Parameter, Namen, Markieren (ohne Fixes 13/13, mit Fixes 13/13 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `cut_speed_min` | Speed 0.1 %, power 0.1 % (minimum values). | ✅ identisch | ✅ identisch |  |
| `cut_speed_max` | Speed 100 %, power 100 %. | ✅ identisch | ✅ identisch |  |
| `cut_fractional_params` | Power 33.3 %, speed 7.25 % (tenth-percent rounding). | ✅ identisch | ✅ identisch |  |
| `cut_params_rounding` | Power 12.35 %, speed 0.15 % (values that truncate differently from rounding). | ✅ identisch | ✅ identisch |  |
| `cut_passes_5` | Five passes over a rect and a line. | ✅ identisch | ✅ identisch |  |
| `cut_name_long` | Project name longer than 15 characters with spaces. | ✅ identisch | ✅ identisch |  |
| `cut_name_umlaut` | Project name with umlauts and ß. | ✅ identisch | ✅ identisch |  |
| `cut_name_emoji` | Project name with an emoji (UTF-16 surrogate pair in Java). | ✅ identisch | ✅ identisch | behoben: Gerätename je UTF-16-Einheit |
| `mark_rect` | Marking: vector part with isCut=false. | ✅ identisch | ✅ identisch |  |
| `mark_circle` | Marking a circle (circle command with isCut=false?). | ✅ identisch | ✅ identisch |  |
| `mark_curves_passes` | Marking curves with two passes. | ✅ identisch | ✅ identisch |  |
| `mark_sets` | Marking with two parameter sets and nested shapes. | ✅ identisch | ✅ identisch |  |
| `mark_speed_max` | Marking at 100 % speed with a small circle (speed limit for circles). | ✅ identisch | ✅ identisch |  |

### F Gravur (Raster) und 3D-Gravur (ohne Fixes 48/57, mit Fixes 48/57 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `engrave_floyd` | Engraving with FloydSteinberg; geometry on whole 500-DPI pixels. | ✅ identisch | ✅ identisch |  |
| `engrave_average` | Engraving with Average; geometry on whole 500-DPI pixels. | ✅ identisch | ✅ identisch |  |
| `engrave_ordered` | Engraving with Ordered; geometry on whole 500-DPI pixels. No pure white: LibLaserCut's matrix holds 256, so Java dots pure white; VisiCutRust deliberately uses 255 there (PROTOCOL.md). | ✅ identisch | ✅ identisch |  |
| `engrave_grid` | Engraving with Grid; geometry on whole 500-DPI pixels. | ✅ identisch | ✅ identisch |  |
| `engrave_halftone` | Engraving with Halftone; geometry on whole 500-DPI pixels. | ✅ identisch | ✅ identisch |  |
| `engrave_brightened` | Engraving with BrightenedHalftone; geometry on whole 500-DPI pixels. | ✅ identisch | ✅ identisch |  |
| `engrave3d_aligned` | 3D engraving: power per pixel from the grey value; pixel-aligned geometry. Only the header is compared. VisiCutRust deliberately differs in the lines (PROTOCOL.md): Java trims 8-bit lines at grey 0 (black) instead of power 0 and shifts 8-bit data by bits, which moves power values between pixels. | ✅ identisch (nur Kopf) | ✅ identisch (nur Kopf) |  |
| `engrave_width_13` | 13 px wide black bar (width not a multiple of 8 bits). | ✅ identisch | ✅ identisch |  |
| `engrave_width_8` | Exactly 8 px and 16 px wide bars. | ✅ identisch | ✅ identisch |  |
| `engrave_one_pixel` | A single black pixel. | ✅ identisch | ✅ identisch |  |
| `engrave_blank_rows` | Two bars with blank rows in between (lines skipped). | ✅ identisch | ✅ identisch |  |
| `engrave_blank_columns` | Two squares side by side with a white gap (leading/trailing white per line). | ✅ identisch | ✅ identisch |  |
| `engrave_wide_run` | 600 px wide black bar (compression runs longer than 255 bytes). | ✅ identisch | ✅ identisch |  |
| `engrave_checker` | 1-px checkerboard (worst case for run-length compression). | ✅ identisch | ✅ identisch |  |
| `engrave_passes2` | FloydSteinberg with two passes. | ✅ identisch | ✅ identisch |  |
| `engrave_sets_speed` | Additional sets at speeds 5, 15, 55 and 95 % (shift table interpolation). | ✅ identisch | ✅ identisch |  |
| `engrave_sets_speed_edges` | Speeds exactly on table entries (10, 20, 90) and 9.9 / 99.9. | ✅ identisch | ✅ identisch |  |
| `engrave_speed_1` | Engraving at 1 % speed (shift at the lower end). | ✅ identisch | ✅ identisch |  |
| `engrave_unidir_topdown` | Unidirectional, top down. | ✅ identisch | ✅ identisch |  |
| `engrave_bidir_bottomup` | Bidirectional, bottom up. | ✅ identisch | ✅ identisch |  |
| `engrave_invert` | Inverted colours (white engraved). | ✅ identisch | ✅ identisch |  |
| `engrave_colorshift_neg` | Colour shift -100. | ✅ identisch | ✅ identisch |  |
| `engrave_colorshift_pos` | Colour shift +100. | ✅ identisch | ✅ identisch |  |
| `engrave_colorshift_255` | Colour shift +255 (everything white?). | ⛔ Rust lehnt ab | ⛔ Rust lehnt ab | Java schreibt einen Auftrag ohne Rasterzeilen, Rust lehnt ab |
| `engrave_grey16_floydsteinberg` | 16 grey steps 0..255 with FloydSteinberg. | ✅ identisch | ✅ identisch |  |
| `engrave_grey16_ordered` | 16 grey steps 0..255 with Ordered. | ✅ identisch | ✅ identisch | Ordered-Matrix 255 statt 256: in LibLaserCut inzwischen ebenso |
| `engrave_grey16_halftone` | 16 grey steps 0..255 with Halftone. | ✅ identisch | ✅ identisch |  |
| `engrave_grey16_average` | 16 grey steps 0..255 with Average. | ✅ identisch | ✅ identisch |  |
| `engrave_grey16_grid` | 16 grey steps 0..255 with Grid. | ✅ identisch | ✅ identisch |  |
| `engrave_grey16_brightenedhalftone` | 16 grey steps 0..255 with BrightenedHalftone. | ✅ identisch | ✅ identisch |  |
| `engrave_rgb` | Pure red, green, blue, yellow, cyan, magenta (luminance conversion). | ✅ identisch | ✅ identisch |  |
| `engrave_fill_opacity` | Rect with fill-opacity 0.5 over white (alpha blending to grey). | ✅ identisch | ✅ identisch |  |
| `engrave_overlap` | Overlapping rects: white painted over black (painter's order). | ✅ identisch | ✅ identisch |  |
| `engrave_stroke_only` | Rect with a 2 px stroke centred on integer edges (no fill). | ✅ identisch | ✅ identisch |  |
| `engrave_rotated_90` | Rect rotated by 90° (still pixel aligned). | ✅ identisch | ✅ identisch |  |
| `engrave_rotated_45` | Rect rotated by 45° (anti-aliased edges; expected to differ). | ❌ weicht ab | ❌ weicht ab | Rasterung schräger oder gebrochener Kanten: Java2D vs. tiny-skia (README) |
| `engrave_circle` | Filled circle (anti-aliased edge; expected to differ). | ❌ weicht ab | ❌ weicht ab | Rasterung schräger oder gebrochener Kanten: Java2D vs. tiny-skia (README) |
| `engrave_png_gradient` | Embedded 8-bit grey PNG gradient at 1:1 pixels. | ✅ identisch | ✅ identisch |  |
| `engrave_png_scaled` | Embedded 40x10 PNG scaled 4x (image interpolation; expected to differ). | ❌ weicht ab | ❌ weicht ab | Bildresampling: Java2D vs. resvg |
| `engrave_png_alpha` | RGBA PNG with an alpha ramp over the white background. | ✅ identisch | ✅ identisch |  |
| `engrave_png_offgrid` | PNG placed at a quarter pixel offset (resampling; expected to differ). | ❌ weicht ab | ❌ weicht ab | Bildresampling: Java2D vs. resvg |
| `engrave_at_origin` | Engraving at the bed origin. | ✅ identisch | ✅ identisch |  |
| `engrave_far_corner` | Engraving near the far bed corner. | ✅ identisch | ✅ identisch | behoben: Overscan am Bettrand wie Java aufgerundet |
| `engrave_white_only` | Only white pixels: nothing to engrave. | ⛔ Rust lehnt ab | ⛔ Rust lehnt ab | Java schreibt einen Auftrag ohne Rasterzeilen, Rust lehnt ab |
| `engrave_bg_quarter_missing` | Dark rect without the quarter-pixel background (area truncation by Helper.toRect). | ✅ identisch | ✅ identisch |  |
| `engrave_bg_offgrid` | Dark rect at 20.5 px (half-pixel edges; raster bounds and coverage). | ❌ weicht ab | ❌ weicht ab | Rasterung schräger oder gebrochener Kanten: Java2D vs. tiny-skia (README) |
| `engrave_threshold_127` | Grey 127 and 128 (threshold rounding), Average dithering. | ✅ identisch | ✅ identisch |  |
| `engrave_passes_sets` | Two passes plus an additional set with two passes. | ✅ identisch | ✅ identisch |  |
| `engrave_speed_frac` | Engrave speed 33.3 % and power 12.5 %. | ✅ identisch | ✅ identisch |  |
| `engrave_tall` | Tall narrow bar: 2 px wide, 200 px high. | ✅ identisch | ✅ identisch |  |
| `engrave_group_opacity` | Group opacity 0.5 over black (group compositing). | ✅ identisch | ✅ identisch |  |
| `engrave_text` | Text in an engraving (font rendering; expected to differ). | ❌ weicht ab | ❌ weicht ab | Rasterung schräger oder gebrochener Kanten: Java2D vs. tiny-skia (README); dazu Schriften |
| `engrave_png_1bit` | 1-bit style PNG: alternating 8 px black/white blocks. | ✅ identisch | ✅ identisch |  |
| `engrave3d_gradient` | 3D engraving of a grey PNG gradient (header only, lines differ by design). | ✅ identisch (nur Kopf) | ✅ identisch (nur Kopf) |  |
| `engrave3d_steps` | 3D engraving of 16 grey steps (header only). | ✅ identisch (nur Kopf) | ✅ identisch (nur Kopf) |  |
| `engrave3d_sets` | 3D engraving with an additional set and two passes (header only). | ✅ identisch (nur Kopf) | ✅ identisch (nur Kopf) |  |
| `engrave3d_full_compare` | 3D engraving of two solid grey rects, full byte compare (documents the known line deviation). | ❌ weicht ab | ❌ weicht ab | Bekannte 3D-Abweichung (PROTOCOL.md): Zeilenbeschnitt bei Grauwert 0 und Bitverschiebung |

### G Importe: Bilder, G-Code, DXF, EPS, PDF (ohne Fixes 22/59, mit Fixes 35/59 identisch)

| Fall | Inhalt | ohne Fixes | mit Fixes | Befund |
| --- | --- | --- | --- | --- |
| `import_png_grey_nodpi` | 8-bit grey PNG without pHYs: both sides 72 DPI. | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_grey_phys72` | Grey PNG with pHYs 72 DPI explicitly. | ❌ weicht ab | ❌ weicht ab | pHYs 2835 px/m ergibt in Rust 72,009 statt 72 DPI (Bild 0,01 % kleiner); dazu 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_grey_phys300` | Grey PNG with pHYs 300 DPI (Rust honours it, Java stays at 72 DPI). | ❌ weicht ab | ❌ weicht ab | Mit `fix/image-resolution` liest Java die 300 DPI ebenfalls: Lage und 53 Zeilen gleich; die Zeilen weichen durch das Resampling 300 → 500 DPI ab (Java bilinear, Rust nearest-neighbour) wie bei den 72-DPI-Bildern |
| `import_png_rgb` | RGB PNG (luminance conversion of colours). | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_rgba` | RGBA PNG with an alpha ramp (background compositing). | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich; dazu Alpha-Verrechnung |
| `import_png_palette` | Indexed (palette) PNG with 16 greys. | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_16bit` | 16-bit grey PNG (conversion to 8 bit). | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich; dazu 16→8 Bit |
| `import_png_1bit` | 1-bit PNG checkerboard of 8-px blocks. | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_odd_size` | Grey PNG of 13 x 7 pixels (non-integer mm size at 72 DPI). | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_white_border` | Grey PNG with a white border (raster bounds from the image, not the content). | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_png_cut` | Cutting a PNG (Java cuts the image frame, Rust rejects). | ⛔ Rust lehnt ab | ⛔ Rust lehnt ab | Java schneidet den Bildrahmen, Rust lehnt Rasterbilder beim Schneiden ab |
| `import_jpeg_grey` | JPEG (grey gradient written by sips, 72 DPI): decoder differences? | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich; dazu JPEG-Decoder (ImageIO vs. image-rs) |
| `import_jpeg_rgb` | RGB JPEG written by sips. | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich; dazu JPEG-Decoder |
| `import_bmp_grey` | BMP written by sips (pixels per metre in the header). | ❌ weicht ab | ❌ weicht ab | 72-DPI-Bild auf 500 DPI skaliert (6,94 px je Bildpixel): Java rendert mit RENDER_QUALITY (bilinear), Rust nearest-neighbour; Lage und Zeilenzahl sind gleich |
| `import_gif_grey` | GIF (Java has no GIF importer). | ⛔ Java-Fehler | ⛔ Java-Fehler | Java hat keinen GIF-Importer |
| `import_gcode_lines` | G00/G01 rectangle and diagonal (Java's long form). | ✅ identisch | ✅ identisch |  |
| `import_gcode_short_form` | G0/G1 short form (Java only understands G00/G01). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/gcode-import` |
| `import_gcode_arc_abs_ij` | G02/G03 arcs with absolute I/J via G90.1 (Java reads I/J absolute anyway). | ✅ identisch | ✅ identisch |  |
| `import_gcode_arc_rel_ij` | G02 arc with relative I/J (standard; Java treats them as absolute). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/gcode-import` |
| `import_gcode_arc_r` | Arc with R word (Java: TODO, draws nothing?). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/gcode-import` |
| `import_gcode_inch` | G20 inch coordinates. | ✅ identisch | ✅ identisch | behoben: Transformationen in f64 statt f32 |
| `import_gcode_relative_g91` | G91 relative moves (Java ignores G91). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/gcode-import` |
| `import_gcode_comments` | Comments in parentheses and after semicolons, lower-case words, feed and spindle. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/gcode-import` |
| `import_gcode_decimal_comma` | Decimal commas and trailing dots in coordinates. | ✅ identisch | ✅ identisch |  |
| `import_gcode_z_moves` | Z moves between XY moves (ignored on both sides?). | ✅ identisch | ✅ identisch |  |
| `import_dxf_lines_circle` | Four LINEs forming a rect and a CIRCLE (no units: mm). | ✅ identisch | ✅ identisch |  |
| `import_dxf_arc` | ARC entities over 90°, 180° and across 0°. | ✅ identisch | ✅ identisch |  |
| `import_dxf_lwpolyline_closed` | Closed LWPOLYLINE. | ✅ identisch | ✅ identisch |  |
| `import_dxf_lwpolyline_open` | Open LWPOLYLINE beside a rect. | ✅ identisch | ✅ identisch |  |
| `import_dxf_lwpolyline_bulge` | LWPOLYLINE with bulges (arc segments, a slot shape). | ✅ identisch | ✅ identisch |  |
| `import_dxf_polyline_vertex` | Old-style POLYLINE/VERTEX/SEQEND, closed. | ✅ identisch | ✅ identisch |  |
| `import_dxf_ellipse` | ELLIPSE full and partial. | ✅ identisch | ✅ identisch |  |
| `import_dxf_spline` | Cubic SPLINE with 6 control points. | ❌ weicht ab | ❌ weicht ab | SPLINE-Näherung: kabeja Polylinie (97 Punkte) vs. Rust Bézier-Abflachung (153 Punkte) |
| `import_dxf_insert_block` | BLOCK inserted twice, the second scaled 2x and rotated 30°. | ❌ weicht ab | ❌ weicht ab | Java (kabeja/svgSalamander) lässt die Linien des Blocks weg und setzt den skalierten, gedrehten INSERT anders |
| `import_dxf_insunits_inch` | $INSUNITS = 1 (inches): Rust converts, kabeja does not. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/dxf-insunits` |
| `import_dxf_insunits_mm` | $INSUNITS = 4 (mm) explicitly. | ✅ identisch | ✅ identisch |  |
| `import_dxf_layers_colors` | Entities on two layers with ACI colours (geometry only). | ❌ weicht ab | ❌ weicht ab | Rust ordnet Objekte nach Ebenen, kabeja nach Dateireihenfolge: der aus Linien verbundene Ring beginnt woanders; Geometrie gleich |
| `import_dxf_negative_coords` | Drawing entirely in negative coordinates (origin shift). | ✅ identisch | ✅ identisch |  |
| `import_dxf_text` | TEXT entity (fonts; expected to differ). | ❌ weicht ab | ❌ weicht ab | TEXT: Schriften |
| `import_eps_lines` | moveto/lineto/closepath stroke rectangle and an open polyline. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-page-background` |
| `import_eps_curves` | curveto and arc (circle) paths. | ❌ weicht ab | ❌ weicht ab | Seitenrahmen seit `fix/eps-page-background` weg, Bounding Box gleich; die Kurvenabflachung setzt in Rust einen Stützpunkt mehr (Ausgabe 14 Bytes länger) |
| `import_eps_fill_only` | Filled shape without stroke. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-page-background` |
| `import_eps_transform` | translate, scale and rotate with gsave/grestore. | ❌ weicht ab | ❌ weicht ab | Seitenrahmen weg; Bounding Box und ein Eckpunkt des gedrehten Rechtecks um eine Einheit (1/4000 Zoll) verschieden, sonst gleich (3 Bytes) |
| `import_eps_procedure_loop` | Procedure definition and a for loop drawing five squares. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-page-background` |
| `import_eps_hires_bbox` | Integer BoundingBox differs from HiResBoundingBox (Java uses the integer one). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-boundingbox` + `fix/eps-page-background` |
| `import_eps_bbox_offset` | BoundingBox not at the origin (drawing shifted). | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-boundingbox` + `fix/eps-page-background` |
| `import_eps_linewidth_zero` | 0 setlinewidth hairline stroke. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-page-background` |
| `import_eps_text` | Text with findfont/show (Rust falls back to Ghostscript; fonts differ). | ❌ weicht ab | ❌ weicht ab | Seitenrahmen weg; Text: Rust über Ghostscript, Java FreeHEP; Schriften |
| `import_eps_dash` | setdash dashed stroke. | ❌ weicht ab | ✅ identisch (Fix) | behoben durch `fix/eps-page-background` |
| `import_pdf_rect_lines` | re and m/l paths stroked (Java: equivalent SVG). | ✅ identisch | ✅ identisch |  |
| `import_pdf_curves` | Bézier curves (c, v, y operators). | ❌ weicht ab | ❌ weicht ab | f32-Transformation (usvg/tiny-skia) statt double: Pixel-Truncation bzw. Flattening kippt vereinzelt um 1 px |
| `import_pdf_cm_transform` | cm matrix with rotation and scale around a nested q/Q. | ✅ identisch | ✅ identisch |  |
| `import_pdf_fill_only` | Filled path without stroke (f operator). | ✅ identisch | ✅ identisch |  |
| `import_pdf_hairline` | Zero-width stroke (0 w): Rust gives 0.1 mm; the SVG uses 0.1 mm too. | ✅ identisch | ✅ identisch |  |
| `import_pdf_mediabox_offset` | MediaBox with a non-zero origin. | ✅ identisch | ✅ identisch |  |
| `import_pdf_flate` | FlateDecode-compressed content stream. | ✅ identisch | ✅ identisch |  |
| `import_pdf_close_subpaths` | Several subpaths, closed with h and implicitly by s. | ✅ identisch | ✅ identisch |  |
| `import_pdf_dash` | Dashed stroke ([6 3] 0 d). | ✅ identisch | ✅ identisch |  |
| `import_pdf_clip` | Clipping path (W n) on a circle-like Bézier. | ❌ weicht ab | ❌ weicht ab | Java ignoriert clipPath (wie cut_clip_rect); Rust wendet den PDF-Clip an |
Reproduktion: `bash scripts/java-parity/generate.sh` (Java 17+, Maven), dann
`cargo test --locked --test java_parity -- --nocapture`; für die Fix-Branches das Jar aus dem
jeweiligen Branch (bzw. einem Merge aller neun) bauen und `LttParity` damit starten.
