# Text, Formen, Schilder, Kopien und Nesting

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

[← Übersicht](README.md)
