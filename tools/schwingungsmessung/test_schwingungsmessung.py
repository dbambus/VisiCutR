#!/usr/bin/env python3
"""Selbsttest mit simulierten Messdaten: python test_schwingungsmessung.py

Prüft das Zerlegen des Binärstroms (adxl_aufnahme.py) und ob die Auswertung
eine bekannte Resonanz wiederfindet (adxl_auswertung.py). Kein Sensor nötig.
"""

import random
import struct
import sys
import tempfile
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import adxl_aufnahme
import adxl_auswertung

G = 9.80665


def frame(samples, ticks, flags=0):
    body = b"".join(b"\0" + struct.pack("<hhh", *s) for s in samples)
    return b"\xa5\x5a" + struct.pack("<BBI", len(samples), flags, ticks) + body


def test_frames():
    rng = random.Random(1)
    rate = 3187.0  # Sensortakt weicht vom Nennwert ab
    stream = b"\x07\x00Muell"
    samples = []
    count = 0
    ticks = (1 << 30) - 20_000  # Überlauf von ticks_us mitten in der Aufnahme
    for _ in range(400):
        n = rng.randint(8, 20)
        batch = [(rng.randint(-400, 400), rng.randint(-400, 400), 256) for _ in range(n)]
        samples += batch
        count += n
        stamp = int(round(ticks + count / rate * 1e6 + rng.uniform(-30, 30))) % (1 << 30)
        stream += frame(batch, stamp, flags=1 if count == 32 else 0)
    parser = adxl_aufnahme.FrameParser()
    at = 0
    while at < len(stream):
        size = rng.randint(1, 300)
        parser.feed(stream[at : at + size])
        at += size
    assert parser.samples == samples, "Messwerte falsch zerlegt"
    assert abs(parser.rate() - rate) < 0.5, parser.rate()
    assert parser.skipped == 7, parser.skipped


def simulate(path):
    """X- und Y-Geraden; X-Rampen regen 31 Hz in Y und 57 Hz in X an."""
    fine = 32_000
    moves = []  # (Achse, Weg mm, Geschwindigkeit mm/s, Beschleunigung mm/s²)
    for _ in range(4):
        moves += [(0, 80, 169, 1.8 * G * 1000), (0, -80, 300, 1.8 * G * 1000)]
    for _ in range(3):
        moves += [(1, 80, 169, 0.24 * G * 1000), (1, -80, 300, 0.24 * G * 1000)]
    command = [np.zeros((int(0.5 * fine), 2))]
    for axis, distance, speed, accel in moves:
        ramp = speed / accel
        cruise = abs(distance) / speed - ramp
        sign = np.sign(distance)
        part = np.zeros((int((2 * ramp + cruise) * fine) + int(0.25 * fine), 2))
        n_ramp = int(ramp * fine)
        n_cruise = int(cruise * fine)
        part[:n_ramp, axis] = sign * accel / 1000
        part[n_ramp + n_cruise : 2 * n_ramp + n_cruise, axis] = -sign * accel / 1000
        command.append(part)
    a_cmd = np.vstack(command)  # m/s²
    modes = [  # (schwingende Achse, auslösende Achse, f, ζ, Kopplung)
        (1, 0, 31.0, 0.04, 0.3),
        (0, 0, 57.0, 0.08, 0.4),
        (1, 1, 74.0, 0.06, 1.0),
    ]
    measured = np.zeros((len(a_cmd), 3))
    measured[:, :2] = a_cmd
    dt = 1 / fine
    for axis, source, f, zeta, coupling in modes:
        w = 2 * np.pi * f
        r = v = 0.0
        out = np.empty(len(a_cmd))
        forcing = a_cmd[:, source]
        for i in range(len(a_cmd)):
            acc = -coupling * forcing[i] - 2 * zeta * w * v - w * w * r
            v += acc * dt
            r += v * dt
            out[i] = acc
        measured[:, axis] += out
    rng = np.random.default_rng(2)
    step = fine // 3200
    data = measured[::step] / G + rng.normal(0, 0.004, (len(measured[::step]), 3))
    data[:, 2] += 1.0
    data = np.round(data / 0.0039) * 0.0039
    rate = 3200
    with open(path, "w", encoding="utf-8") as out:
        out.write(f"# rate_soll={rate} rate_ist={rate} preset=sim\n# notiz=Simulation\n")
        out.write("# ueberlaeufe=0\nt_s,ax_g,ay_g,az_g\n")
        for i, (x, y, z) in enumerate(data):
            out.write(f"{i / rate:.6f},{x:.4f},{y:.4f},{z:.4f}\n")


def test_analysis():
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "sim.csv"
        simulate(path)
        t, acc, meta = adxl_auswertung.load(path)
    assert meta["notiz"] == "Simulation"
    result = adxl_auswertung.analyse(t, acc, 10, 200)
    modes = {round(mode["f"]): mode for mode in result["modes"]}
    assert len(modes) == 3, sorted(modes)

    def row(f, trigger, axis):
        mode = min(result["modes"], key=lambda m: abs(m["f"] - f))
        rows = {(r["trigger"], r["axis"]): r for r in mode["rows"]}
        found = rows[(trigger, axis)]
        assert abs(found["f"] - f) < 0.02 * f, found
        return found

    # Wie auf dem Foto: X-Rampen lassen den Kopf in Y schwingen.
    x_to_y = row(31, 0, 1)
    assert x_to_y["count"] >= 12, x_to_y
    assert 0.034 < x_to_y["zeta"] < 0.046, x_to_y
    # Geschwindigkeitsänderungen der X-Fahrten: 169 bzw. 300 mm/s.
    assert 150 < x_to_y["dv"] < 320, x_to_y
    # Statische Auslenkung κ·a/ω² als Größenordnung des Ausschlags.
    static = 0.3 * 1.8 * G / (2 * np.pi * 31) ** 2 * 1000
    assert 0.2 * static < x_to_y["mm_max"] < 2 * static, (x_to_y, static)
    assert 0.06 < row(57, 0, 0)["zeta"] < 0.10
    assert 0.045 < row(74, 1, 1)["zeta"] < 0.075
    assert adxl_auswertung.strongest(result)["axis"] == 1
    adxl_auswertung.report(path, meta, result, list("XYZ"))


if __name__ == "__main__":
    test_frames()
    print("Binärstrom: in Ordnung")
    test_analysis()
    print("Auswertung: in Ordnung")
