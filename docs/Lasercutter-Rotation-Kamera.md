# Lasercutter, Rotationseinheit und Kamera

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
  ruft die FabLab-Einstellungen eines FabLabs aus VisiCuts Liste von dessen
  Einstellungs-Server ab; ein Dialog fragt, was übernommen wird: die LTT-iLaser-4000-Geräte (ersetzen gleichnamige), die
  Materialien mit ihren Laserwerten (ersetzen die Materialbibliothek; ein
  Laserprofil mit mehreren Einträgen wird zu Durchgängen und weiteren
  Parametersätzen, die ein Schritt mit diesem Material übernimmt) und
  die Zuordnungen als Vorlagen. Wie in Java-VisiCut prüft VisiCutR danach auf
  Wunsch einmal am Tag beim Start, ob das FabLab eine neue Fassung
  veröffentlicht hat, und bietet sie zum Übernehmen an. **Exportieren**
  schreibt alle Geräte als JSON.
- Die Geräteliste liegt unter `~/Library/Application Support/VisiCutR`
  (Windows `%APPDATA%\VisiCutR`, Linux `~/.config/visicutr`);
  `VISICUTR_CONFIG_DIR` setzt einen anderen Ordner.
- **Maschine**: Zeilenversatz-Tabelle für die bidirektionale Gravur
  (10 … 100 %, 1/4000 Zoll), Kurvenbeschleunigung (LibLaserCut 2000 mm/s²
  oder experimentell „Firmware 0,1 g“) und Verweilzeit am Konturstart für die
  Zeitschätzung. VisiCut-Gerätedateien liefern Tabelle und Beschleunigung mit.
- **Zeilenversatz kalibrieren**: Testmuster öffnen und gravieren, je
  Geschwindigkeit den Block mit den geradesten Kanten wählen und in die
  Tabelle übernehmen (siehe [LTT-Protokoll](LTT-Protokoll.md)).
- **Rotationseinheit**: Bei Geräten mit Rotationseinheit lässt sie sich je Projekt mit
  Werkstückdurchmesser einschalten (siehe [LTT-Protokoll](LTT-Protokoll.md)).
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
  Das Passwort liegt in der Geräteliste (`devices.json`), die unter macOS und
  Linux nur der eigene Benutzer lesen darf (Rechte 0600), unter Windows im
  privaten Profilordner; Exporte lassen es immer weg. **Aktualisierung (ms)** (VisiCut
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
- **Kameraserver**: `visicutr-cameraserver` ersetzt Java-VisiCuts `tools/cameraserver` und
  beantwortet jede HTTP-Anfrage mit einem frischen Foto eines
  Aufnahmebefehls, z. B.
  `visicutr-cameraserver -- gphoto2 --capture-image-and-download --stdout` oder
  `visicutr-cameraserver --output snap.jpg -- imagesnap -q snap.jpg`
  (`--port`, `--rotate 90`). Er liegt in `VisiCutR.app/Contents/MacOS`
  bzw. neben der Windows-/Linux-Programmdatei. Er lauscht im ganzen Netz;
  mit `--user NAME` und einem Kennwort (`--password` oder die
  Umgebungsvariable `VISICUTR_CAMERA_PASSWORD`) verlangt er eine Anmeldung,
  die VisiCutR mit Kamera-Benutzer und -Kennwort des Geräts sendet. Jede
  Verbindung hat Fristen (Anfrage 10 s, Antwort 30 s), der Aufnahmebefehl
  wird nach 30 s beendet, Fehlerdetails stehen nur in der Konsole des
  Servers.

[← Übersicht](README.md)
