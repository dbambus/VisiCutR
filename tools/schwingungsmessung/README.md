# Schwingungsmessung am Laserkopf

Werkzeuge, um mit einem Beschleunigungssensor ADXL345 zu messen, wie der
Laserkopf des iLaser 4000 nach Rampen nachschwingt. Anlass: Beim Lasern eines
Kreuzes hat die waagerechte Linie an Anfang und Ende Haken quer zur
Fahrtrichtung, die senkrechte nicht. Vermutet wird, dass das Portal nach
X-Rampen in Y schwingt.

Aus der Messung folgen Eigenfrequenz, Dämpfung und Ausschlag je Resonanz und
welche Achse sie auslöst. Mit der Schwingungsdauer T lassen sich Rampen
planen, die nach ganzen Vielfachen von T enden und deshalb kaum
Restschwingung hinterlassen.

| Datei | Zweck |
| --- | --- |
| `firmware/main.py` | MicroPython-Programm für den RP2040: liest den ADXL345 mit bis zu 3200 Hz und sendet die Werte über USB |
| `adxl_aufnahme.py` | Aufnahme am Laptop, schreibt eine CSV-Datei |
| `adxl_auswertung.py` | Auswertung einer oder mehrerer Aufnahmen, mit Bild |
| `testmuster/*.svg` | Testmuster für VisiCutR |
| `test_schwingungsmessung.py` | Selbsttest mit simulierten Daten, ohne Sensor |

## Hardware

Der ADXL345 hat selbst kein USB; zwischen Sensor und Laptop sitzt immer ein
RP2040. Beide Varianten verwenden dieselbe Firmware, die das Board beim Start
selbst erkennt.

**A: USB-C-Sensorboard mit RP2040** (BIGTREETECH ADXL345 V2, FYSETC Portable
Input Shaper und ähnliche). Diese Boards laufen ab Werk meist mit
Klipper-Firmware und müssen einmal auf MicroPython umgestellt werden:

1. BOOT-Taste gedrückt halten und das Board per USB-C anstecken; es erscheint
   das Laufwerk `RPI-RP2`.
2. MicroPython für den Raspberry Pi Pico
   (<https://micropython.org/download/RPI_PICO/>, Datei `.uf2`) auf das
   Laufwerk ziehen. Das Board startet neu.
3. `firmware/main.py` als `main.py` auf das Board kopieren, mit
   [Thonny](https://thonny.org) (Datei → Speichern unter → Raspberry Pi Pico)
   oder `mpremote cp firmware/main.py :main.py`.

Zurück zu Klipper: wie in Schritt 1, dann die Klipper-`.uf2` des Boards
aufspielen.

**B: Raspberry Pi Pico mit ADXL345-Platine**, Verdrahtung wie in der
Klipper-Dokumentation:

| ADXL345 | Pico |
| --- | --- |
| 3V3 / VCC | 3V3 OUT (Pin 36) |
| GND | GND (Pin 38) |
| CS | GP1 (Pin 2) |
| SDO | GP0 (Pin 1) |
| SDA | GP3 (Pin 5) |
| SCL | GP2 (Pin 4) |

MicroPython und `main.py` wie bei A, Schritte 1–3.

Andere Boards: in `firmware/main.py` unter `PRESETS` die Pins aus der
Klipper-Konfiguration des Boards ergänzen. Antwortet der Sensor nicht,
meldet die Aufnahme `ADXL345 nicht gefunden`.

### Montage

- Den Sensor **starr** am Laserkopf befestigen, möglichst nah an der Düse:
  verschrauben oder mit Sekundenkleber auf einen kleinen Winkel. Klebepads
  und Kabelbinder dämpfen die Schwingung weg.
- Die Achsen des Sensors möglichst parallel zu den Maschinenachsen
  ausrichten und die Zuordnung notieren, z. B. „Sensor-X zeigt nach
  Maschinen-−Y“ (Aufdruck auf der Platine).
- Das USB-Kabel so führen und zugentlasten, dass es den ganzen Verfahrweg
  mitmacht, nicht in den Strahlengang hängt und nirgends einhakt.
- **Keine Sicherheitsabschaltung überbrücken.** Lässt sich das Kabel bei
  geschlossener Haube nicht ohne Weiteres herausführen, das Vorgehen mit den
  Betreuern des Lasers klären.

## Software

Python 3.9 oder neuer, dazu

```sh
pip install pyserial numpy matplotlib
```

`matplotlib` ist nur für die Bilder nötig. Board und Sensor prüfen:

```sh
python adxl_aufnahme.py --pruefen
# z. B. „COM5: INFO ADXL345 preset=btt“
```

Den Port findet das Skript selbst, wenn genau ein RP2040 angeschlossen ist,
sonst `--port COM5` bzw. `--port /dev/ttyACM0` angeben.

## Messreihe

Die Testmuster sind 100 × 100 mm groß, alle Linien 80 mm lang:

| Muster | Inhalt | Befehle am Gerät |
| --- | --- | --- |
| `1-x-geraden.svg` | 5 Geraden in X | Start/Stopp-Vektor (`PR`) |
| `2-y-geraden.svg` | 5 Geraden in Y | Start/Stopp-Vektor (`PR`) |
| `3-x-kurve.svg` | 5 Geraden in X mit Zwischenpunkten alle 10 mm | verbundene Kurve (`PJ … PE … PF`) mit geplanter Rampe |
| `4-kreuze.svg` | 3 Kreuze 10 × 10 mm wie auf dem Foto | Start/Stopp-Vektor (`PR`) |

Jedes Muster in VisiCutR als **Schneiden** auf Papier oder Pappe laden, mit
sehr kleiner Leistung, damit nur eine Spur entsteht. Dann für 20 %, 50 % und
100 % Geschwindigkeit je eine Aufnahme:

1. Aufnahme starten, Name nach Muster und Geschwindigkeit:
   ```sh
   python adxl_aufnahme.py --notiz "1-x-geraden 50 %" messungen/1-x-50.csv
   ```
2. Den Auftrag in VisiCutR an den Laser senden und abarbeiten lassen.
3. Zwei Sekunden nach dem Ende **Strg+C** drücken; die Datei wird
   geschrieben.

Ergänzungen, die viel verraten:

- `3-x-kurve.svg` ein zweites Mal mit der Geräteeinstellung
  **„Firmware 0,1 g“** (Kurvenbeschleunigung 981 mm/s²) statt 2000 mm/s².
- `1-x-geraden.svg` einmal in der Mitte und einmal in einer Ecke der
  Arbeitsfläche: Schwingt das Portal überall gleich?
- Das Papier aufbewahren und neben die Auswertung legen: Der ADXL misst den
  Kopf, das Brandbild zeigt den Strahl auf dem Material.

Steigen in der Aufnahme die `Überläufe`, die Rate auf `--rate 1600`
senken.

## Auswertung

```sh
python adxl_auswertung.py messungen/*.csv
# Sensorachsen anders als Maschinenachsen montiert:
python adxl_auswertung.py --achsen x=-y,y=x,z=z messungen/*.csv
```

`--achsen` gibt für jede Maschinenachse an, welche Sensorachse ihr mit
welchem Vorzeichen entspricht. Für jede Datei erscheinen:

- **Resonanz bei … Hz**: eine Eigenschwingung, gefunden über die
  Ausschwingvorgänge nach Rampen. Für jede Kombination aus **Auslöser**
  (Achse, die gerade beschleunigt oder gebremst hat) und schwingender Achse
  stehen Anzahl, Frequenz, Dämpfungsgrad **ζ** und **Ausschlag** in mm
  (Beschleunigung / ω², eine Näherung). Die Vermutung vom Foto wäre eine Zeile
  „X → Y“ mit deutlichem Ausschlag.
- **|Δv|**: die Geschwindigkeitsänderung vor dem Vorgang. Sie sollte zur
  eingestellten Geschwindigkeit passen (50 % ≈ 169 mm/s). Passt sie zu einer
  anderen Achse als erwartet, stimmt die Achsenzuordnung nicht.
- **Stärkste Schwingung**: Schwingungsdauer T, Abklingzeit, und welche
  Geschwindigkeitssprünge bzw. Geschwindigkeiten Rampen von genau n·T Dauer
  ergeben. Für Start/Stopp-Vektoren ist eine Rampe mit `vyacc` 0,24 g
  angenommen. Das ist zu prüfen, z. B. ob bei diesen Geschwindigkeiten die
  Haken tatsächlich kleiner werden.
- Ein Bild `<Datei>.png`: Zeitverlauf mit den ausgewerteten Abschnitten,
  Spektrum mit den gefundenen Resonanzen und der stärkste Ausschwingvorgang
  mit angepasster Hüllkurve.

Bei mehreren Dateien folgt eine Übersicht aller Resonanzen je Datei.
Klipper-Rohdaten (`accel_*.csv` von `ACCELEROMETER_MEASURE`) werden
ebenfalls gelesen.

## Grenzen

- Der Sensor misst den Kopf, nicht die Lage des Strahls auf dem Werkstück.
  Spiegel, Linse und Bett können zusätzlich schwingen.
- Der ADXL345 erfasst bei 3200 Hz Frequenzen bis etwa 1600 Hz, die
  Auswertung sucht zwischen 10 und 200 Hz (`--fmin`, `--fmax`).
- Der Takt des Sensors weicht vom Nennwert ab. Die Aufnahme bestimmt die
  tatsächliche Rate aus den Zeitstempeln des RP2040, damit die Frequenzen
  stimmen.

## Selbsttest

```sh
python test_schwingungsmessung.py
```

Simuliert X- und Y-Fahrten mit drei bekannten Resonanzen und prüft, ob die
Auswertung Frequenz, Dämpfung, Auslöser und Ausschlag wiederfindet. Außerdem
prüft er das Zerlegen des USB-Datenstroms. Ein Sensor ist dafür nicht nötig.
