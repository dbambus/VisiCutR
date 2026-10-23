#!/usr/bin/env python3
"""Nimmt die Beschleunigung eines ADXL345 am RP2040 (firmware/main.py) auf.

Beispiel:
    python adxl_aufnahme.py --notiz "1-x-geraden 50 %" messungen/x-50.csv

Die Aufnahme läuft, bis Strg+C gedrückt wird oder `--dauer` abgelaufen ist.
Die CSV-Datei enthält Zeit (s) und Beschleunigung X, Y, Z (g) in den Achsen
des Sensors; Kommentarzeilen mit `#` halten Rate, Board und Notiz fest.
"""

import argparse
import datetime
import struct
import sys
import time

MAGIC = b"\xa5\x5a"
HEADER = 8
SLOT = 7
FIFO_SIZE = 32
TICKS_PERIOD = 1 << 30  # time.ticks_us() von MicroPython läuft hier über
SCALE_G = 0.0039  # volle Auflösung: 3,9 mg je Stufe
RP2040_VID = 0x2E8A


class FrameParser:
    """Zerlegt den Binärstrom der Firmware in Messwerte und Zeitmarken."""

    def __init__(self):
        self.buffer = bytearray()
        self.samples = []  # (x, y, z) in Rohwerten
        self.marks = []  # (Index des letzten Werts eines Rahmens, µs)
        self.overflows = 0
        self.skipped = 0
        self._ticks = None
        self._micros = 0

    def feed(self, data):
        self.buffer += data
        buffer = self.buffer
        start = 0
        while True:
            at = buffer.find(MAGIC, start)
            if at < 0:
                start = max(start, len(buffer) - 1)
                break
            self.skipped += at - start
            if len(buffer) - at < HEADER:
                start = at
                break
            n, flags, ticks = struct.unpack_from("<BBI", buffer, at + 2)
            if not 1 <= n <= FIFO_SIZE:
                start = at + 1
                continue
            end = at + HEADER + n * SLOT
            if len(buffer) < end:
                start = at
                break
            self.samples.extend(struct.iter_unpack("<xhhh", buffer[at + HEADER : end]))
            if flags & 1:
                self.overflows += 1
            if self._ticks is not None:
                self._micros += (ticks - self._ticks) % TICKS_PERIOD
            self._ticks = ticks
            self.marks.append((len(self.samples) - 1, self._micros))
            start = end
        del buffer[:start]

    def rate(self):
        """Tatsächliche Abtastrate aus den Zeitmarken (Ausgleichsgerade)."""
        if len(self.marks) < 2:
            return None
        n = len(self.marks)
        mean_i = sum(i for i, _ in self.marks) / n
        mean_t = sum(t for _, t in self.marks) / n
        sxy = sum((i - mean_i) * (t - mean_t) for i, t in self.marks)
        sxx = sum((i - mean_i) ** 2 for i, _ in self.marks)
        if sxx == 0 or sxy <= 0:
            return None
        return 1e6 * sxx / sxy


def find_port():
    from serial.tools import list_ports

    ports = [p for p in list_ports.comports() if p.vid == RP2040_VID]
    if not ports:
        sys.exit("Kein RP2040 gefunden; Port mit --port angeben.")
    if len(ports) > 1:
        names = ", ".join(p.device for p in ports)
        sys.exit(f"Mehrere RP2040 gefunden ({names}); Port mit --port angeben.")
    return ports[0].device


def command(port, text, prefixes, timeout=3.0):
    """Sendet einen Befehl und liefert die erste Antwortzeile mit einem der Präfixe."""
    port.write(text.encode() + b"\n")
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        line = port.readline().decode(errors="replace").strip()
        if line.startswith("ERR"):
            sys.exit(f"Sensor: {line}")
        if line.startswith(prefixes):
            return line
    sys.exit("Keine Antwort vom Board. Läuft firmware/main.py als main.py?")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("datei", nargs="?", help="Ziel-CSV (Standard: messung-<Zeit>.csv)")
    parser.add_argument("--port", help="serielle Schnittstelle, z. B. COM5 oder /dev/ttyACM0")
    parser.add_argument("--rate", type=int, default=3200, choices=(3200, 1600, 800, 400))
    parser.add_argument("--dauer", type=float, help="Aufnahmedauer in Sekunden")
    parser.add_argument("--notiz", default="", help="z. B. Testmuster und Geschwindigkeit")
    parser.add_argument("--pruefen", action="store_true", help="nur Board und Sensor prüfen")
    args = parser.parse_args()

    import serial

    name = args.port or find_port()
    stamp = datetime.datetime.now()
    path = args.datei or stamp.strftime("messung-%Y%m%d-%H%M%S.csv")
    with serial.Serial(name, 115200, timeout=0.2) as port:
        # Eine laufende Messung beenden und Reste verwerfen.
        port.write(b"X\n")
        time.sleep(0.3)
        port.reset_input_buffer()
        info = command(port, "I", ("INFO",))
        print(f"{name}: {info}")
        if args.pruefen:
            return
        ok = command(port, f"S{args.rate}", ("OK",))
        preset = ok.split("preset=")[-1]
        frames = FrameParser()
        started = time.monotonic()
        shown = started
        print("Aufnahme läuft, Ende mit Strg+C.")
        try:
            while args.dauer is None or time.monotonic() - started < args.dauer:
                frames.feed(port.read(max(1, port.in_waiting)))
                now = time.monotonic()
                if now - shown >= 1.0:
                    shown = now
                    rate = frames.rate()
                    rate_text = f"{rate:.0f} Hz" if rate else "–"
                    print(
                        f"\r{now - started:6.1f} s  {len(frames.samples):8d} Werte  {rate_text}"
                        f"  Überläufe {frames.overflows}",
                        end="",
                        flush=True,
                    )
        except KeyboardInterrupt:
            pass
        finally:
            port.write(b"X\n")
        print()

    rate = frames.rate()
    if not frames.samples or rate is None:
        sys.exit("Keine Messwerte empfangen.")
    with open(path, "w", encoding="utf-8") as out:
        out.write(f"# ADXL345-Aufnahme {stamp.isoformat(timespec='seconds')}\n")
        out.write(f"# rate_soll={args.rate} rate_ist={rate:.3f} preset={preset}\n")
        out.write(f"# notiz={args.notiz}\n")
        out.write(f"# ueberlaeufe={frames.overflows}\n")
        out.write("t_s,ax_g,ay_g,az_g\n")
        for i, (x, y, z) in enumerate(frames.samples):
            out.write(f"{i / rate:.6f},{x * SCALE_G:.4f},{y * SCALE_G:.4f},{z * SCALE_G:.4f}\n")
    print(f"{len(frames.samples)} Werte mit {rate:.1f} Hz in {path} gespeichert.")
    if frames.overflows:
        print(
            f"Achtung: {frames.overflows}-mal war der FIFO voll, einzelne Werte können fehlen."
            " Kleinere Rate (--rate 1600) verwenden."
        )
    if frames.skipped:
        print(f"Hinweis: {frames.skipped} Bytes ohne Rahmen übersprungen.")


if __name__ == "__main__":
    main()
