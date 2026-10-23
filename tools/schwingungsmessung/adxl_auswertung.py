#!/usr/bin/env python3
"""Wertet ADXL-Aufnahmen aus: Eigenfrequenz, Dämpfung und Ausschlag.

Beispiel:
    python adxl_auswertung.py messungen/*.csv
    python adxl_auswertung.py --achsen x=-y,y=x,z=z messungen/x-50.csv

Liest CSV-Dateien von adxl_aufnahme.py (g) oder Klipper-Rohdaten
(`accel_*.csv`, mm/s²). Für jede Datei:

1. Spektrum (Welch) je Achse und die stärksten Spitzen.
2. Ausschwingvorgänge: Nach jeder Rampe wird dort, wo der Kopf mit
   konstanter Geschwindigkeit fährt oder steht, eine gedämpfte
   Sinusschwingung angepasst. Daraus folgen Frequenz, Dämpfungsgrad ζ und
   Ausschlag (Beschleunigung / ω²).
3. Auslöser: die Achse mit der größten Geschwindigkeitsänderung kurz davor.
4. Hinweise für Rampen, die nach ganzen Schwingungsdauern enden.

Mit matplotlib entsteht zusätzlich je Datei ein Bild `<Datei>.png`.
"""

import argparse
import math
import sys
from pathlib import Path

import numpy as np

G = 9.80665
NOMINAL_CUT_SPEED = 338.67  # mm/s bei 100 % Schneiden (iLaser 4000)
CURVE_RAMP = 0.1 * G * 1000  # mm/s², feste Rampe verbundener Kurven
STOP_RAMP = 0.24 * G * 1000  # mm/s², vyacc für Start/Stopp-Vektoren (Annahme)
AXES = "XYZ"


# --- Einlesen ---------------------------------------------------------------


def load(path):
    """Zeit (s), Beschleunigung (N×3, m/s², Sensorachsen) und Metadaten."""
    meta = {}
    lines = []
    scale = G
    with open(path, encoding="utf-8") as file:
        for line in file:
            line = line.strip()
            if not line:
                continue
            if line.startswith("#"):
                text = line[1:].strip()
                if "accel_x" in text:
                    scale = 0.001  # Klipper: mm/s²
                if text.startswith("notiz="):
                    meta["notiz"] = text[len("notiz=") :]
                    continue
                for part in text.split():
                    if "=" in part:
                        key, value = part.split("=", 1)
                        meta[key] = value
                continue
            if not (line[0].isdigit() or line[0] in "-+."):
                if "accel_x" in line:
                    scale = 0.001
                continue
            lines.append(line)
    if len(lines) < 100:
        raise ValueError("zu wenige Messwerte")
    data = np.loadtxt(lines, delimiter=",", ndmin=2)
    return data[:, 0] - data[0, 0], data[:, 1:4] * scale, meta


def axis_matrix(text):
    """`x=-y,y=x,z=z`: Maschinenachse = ±Sensorachse."""
    matrix = np.eye(3)
    if not text:
        return matrix
    matrix[:] = 0
    seen = set()
    for part in text.split(","):
        machine, _, sensor = part.partition("=")
        machine, sensor = machine.strip().lower(), sensor.strip().lower()
        sign = -1.0 if sensor.startswith("-") else 1.0
        sensor = sensor.lstrip("+-")
        if machine not in "xyz" or sensor not in ("x", "y", "z") or len(machine) != 1:
            raise ValueError(f"ungültige Achsenzuordnung: {part}")
        matrix["xyz".index(machine), "xyz".index(sensor)] = sign
        seen.add(machine)
    if seen != {"x", "y", "z"}:
        raise ValueError("Achsenzuordnung braucht x, y und z")
    return matrix


# --- Signalverarbeitung -----------------------------------------------------


def fft_filter(x, fs, gain):
    """Filtert spaltenweise im Frequenzbereich; `gain(f)` ist reell."""
    n = len(x)
    size = 1 << (2 * n - 1).bit_length()
    spectrum = np.fft.rfft(x, size, axis=0)
    weights = gain(np.fft.rfftfreq(size, 1 / fs))
    if x.ndim == 2:
        weights = weights[:, None]
    return np.fft.irfft(spectrum * weights, size, axis=0)[:n]


def ramp(f, low, high):
    """0 unter `low`, 1 über `high`, dazwischen Kosinusübergang."""
    u = np.clip((f - low) / (high - low), 0, 1)
    return 0.5 - 0.5 * np.cos(np.pi * u)


def highpass(fc):
    return lambda f: ramp(f, fc / 2, fc)


def lowpass(fc):
    return lambda f: 1 - ramp(f, fc, 2 * fc)


def bandpass(low, high):
    return lambda f: ramp(f, low * 0.7, low) * (1 - ramp(f, high, high * 1.4))


def triangle_mean(x, period):
    """Gleitendes Dreiecksmittel über 2·`period` Werte, spaltenweise."""
    box = np.ones(period) / period
    kernel = np.convolve(box, box)
    return np.column_stack([np.convolve(x[:, k], kernel, mode="same") for k in range(x.shape[1])])


def envelope(x):
    """Betrag des analytischen Signals, spaltenweise."""
    n = len(x)
    spectrum = np.fft.fft(x, axis=0)
    h = np.zeros(n)
    h[0] = 1
    if n % 2 == 0:
        h[n // 2] = 1
        h[1 : n // 2] = 2
    else:
        h[1 : (n + 1) // 2] = 2
    if x.ndim == 2:
        h = h[:, None]
    return np.abs(np.fft.ifft(spectrum * h, axis=0))


def welch(x, fs, seconds=1.0):
    """Leistungsdichte je Spalte (Hann, 50 % Überlappung)."""
    size = min(1 << round(math.log2(fs * seconds)), 1 << (len(x).bit_length() - 1))
    window = np.hanning(size)
    step = size // 2
    total = 0
    count = 0
    for start in range(0, len(x) - size + 1, step):
        segment = x[start : start + size]
        segment = segment - segment.mean(axis=0)
        total = total + np.abs(np.fft.rfft(segment * window[:, None], axis=0)) ** 2
        count += 1
    psd = total / count / (fs * (window**2).sum())
    psd[1:-1] *= 2
    return np.fft.rfftfreq(size, 1 / fs), psd


def peaks(freqs, power, fmin, fmax, count=3):
    """Stärkste Spitzen: (Frequenz, Höhe, ζ aus der Halbwertsbreite)."""
    band = (freqs >= fmin) & (freqs <= fmax)
    floor = np.median(power[band])
    found = []
    for i in np.flatnonzero(band)[1:-1]:
        if power[i] > power[i - 1] and power[i] >= power[i + 1] and power[i] > 5 * floor:
            found.append(i)
    found.sort(key=lambda i: -power[i])
    resolution = freqs[1] - freqs[0]
    result = []
    for i in found:
        # Nebenspitzen an den Flanken einer stärkeren Spitze überspringen.
        if any(abs(freqs[i] - f) < max(3 * resolution, 0.15 * f) for f, _, _ in result):
            continue
        # Parabel durch die logarithmierten Nachbarn.
        a, b, c = np.log(power[i - 1 : i + 2])
        shift = 0.5 * (a - c) / (a - 2 * b + c) if a - 2 * b + c != 0 else 0.0
        f0 = freqs[i] + shift * resolution
        half = power[i] / 2
        lo = i
        while lo > 0 and power[lo] > half:
            lo -= 1
        hi = i
        while hi < len(power) - 1 and power[hi] > half:
            hi += 1
        f1 = np.interp(half, [power[lo], power[lo + 1]], [freqs[lo], freqs[lo + 1]])
        f2 = np.interp(half, [power[hi], power[hi - 1]], [freqs[hi], freqs[hi - 1]])
        result.append((f0, power[i], (f2 - f1) / (2 * f0)))
        if len(result) == count:
            break
    return result


def fit_damped(t, y, f_guess):
    """Passt y ≈ e^(−σt)·(A·cos ωt + B·sin ωt) + C an.

    Liefert (f gedämpft, σ, Amplitude bei t=0, Bestimmtheitsmaß) oder None.
    """
    y = y - y.mean()
    total = float(y @ y)
    if total == 0:
        return None
    span = (0.8 * f_guess, 1.2 * f_guess)
    sigma_span = (0.0, 2 * np.pi * f_guess * 0.3)
    best = None
    for _ in range(3):
        freqs = np.linspace(*span, 25)
        sigmas = np.linspace(*sigma_span, 25)
        decay = np.exp(-np.outer(sigmas, t))  # S×n
        for f in freqs:
            w = 2 * np.pi * f
            c = decay * np.cos(w * t)
            s = decay * np.sin(w * t)
            ones = np.ones_like(c)
            columns = (c, s, ones)
            gram = np.empty((len(sigmas), 3, 3))
            rhs = np.empty((len(sigmas), 3))
            for i, u in enumerate(columns):
                rhs[:, i] = u @ y
                for j, v in enumerate(columns):
                    gram[:, i, j] = np.einsum("ij,ij->i", u, v)
            try:
                solution = np.linalg.solve(gram, rhs[..., None])[..., 0]
            except np.linalg.LinAlgError:
                continue
            residual = total - np.einsum("ij,ij->i", rhs, solution)
            k = int(np.argmin(residual))
            if best is None or residual[k] < best[0]:
                best = (residual[k], f, sigmas[k], solution[k])
        _, f, sigma, _ = best
        df = (span[1] - span[0]) / 6
        ds = (sigma_span[1] - sigma_span[0]) / 6
        span = (f - df, f + df)
        sigma_span = (max(0.0, sigma - ds), sigma + ds)
    residual, f, sigma, (a, b, _) = best
    return f, sigma, math.hypot(a, b), 1 - residual / total


# --- Auswertung -------------------------------------------------------------


def analyse(t, acc, fmin, fmax):
    fs = (len(t) - 1) / (t[-1] - t[0])
    fmax = min(fmax, 0.4 * fs)
    acc = acc - np.median(acc, axis=0)
    freqs, psd = welch(acc, fs)
    # Kandidaten im Spektrum des Rucks (× f²): Die Rampen der Steuerung sind
    # dort flach, Resonanzen ragen heraus.
    jerk = psd * (freqs**2)[:, None]
    spectrum = [peaks(freqs, jerk[:, k], fmin, fmax) for k in range(3)]
    guesses = []
    for power in (jerk[:, 0] + jerk[:, 1], jerk[:, 0], jerk[:, 1]):
        for guess, _, _ in peaks(freqs, power, fmin, fmax, 4):
            if all(abs(guess - other) > 0.1 * other for other in guesses):
                guesses.append(guess)
    found = []  # (Frequenz, Vorgang, Achse, Anpassung) über alle Kandidaten
    if guesses:
        # Die Rampen der Steuerung ohne die Schwingungen: ein Dreiecksmittel
        # über zwei Dauern der tiefsten Resonanz löscht diese aus und dämpft
        # alle höheren stark, ohne lange nachzulaufen.
        slow = triangle_mean(acc, int(round(fs / min(guesses))))
    for guess in guesses:
        for event in ringing_events(t, acc, fs, guess, slow):
            for k, fit in enumerate(event["fits"]):
                # Am Rand des Suchbereichs liegt die Resonanz woanders.
                if fit and abs(fit["f"] / guess - 1) < 0.18 and fit["zeta"] < 0.25:
                    found.append((fit["f"], event, k, fit))
    # Nach der angepassten Frequenz zu Resonanzen gruppieren.
    found.sort(key=lambda item: item[0])
    clusters = []
    for item in found:
        if clusters and item[0] - clusters[-1][-1][0] < 0.04 * item[0]:
            clusters[-1].append(item)
        else:
            clusters.append([item])
    modes = []
    for cluster in clusters:
        # Derselbe Vorgang kann von mehreren Kandidaten gefunden werden.
        unique = {}
        for f, event, k, fit in cluster:
            key = (k, round(event["t"] * f / 2))
            if key not in unique or fit["r2"] > unique[key][2]["r2"]:
                unique[key] = (event, k, fit)
        items = sorted(unique.values(), key=lambda item: item[0]["t"])
        if len(items) < 3:
            continue
        modes.append(
            {
                "f": float(np.median([fit["f"] for _, _, fit in items])),
                "items": items,
                "events": len({id(event) for event, _, _ in items}),
                "rows": summary(items),
            }
        )
    return {
        "fs": fs,
        "duration": t[-1] - t[0],
        "fmax": fmax,
        "freqs": freqs,
        "psd": psd,
        "spectrum": spectrum,
        "modes": modes,
    }


def ringing_events(t, acc, fs, f0, slow):
    """Ausschwingvorgänge nahe `f0` mit angepasster Schwingung je Achse.

    `slow` ist die Beschleunigung ohne Schwingungen (Rampen der Steuerung).
    """
    period = int(round(fs / f0))
    ringing = fft_filter(acc, fs, bandpass(0.6 * f0, 1.6 * f0))
    banded = fft_filter(acc, fs, bandpass(0.8 * f0, 1.25 * f0))
    env = envelope(banded)
    env_all = np.sqrt((env**2).sum(axis=1))
    forcing = np.sqrt((slow**2).sum(axis=1))
    noise = np.percentile(env_all, 20)
    noise_axis = np.percentile(env, 20, axis=0)
    forcing_limit = max(5 * np.median(forcing), 0.02 * G)

    inner = env_all[1:-1]
    local = (inner > 6 * noise) & (inner >= env_all[:-2]) & (inner > env_all[2:])
    candidates = sorted(np.flatnonzero(local) + 1, key=lambda i: -env_all[i])
    chosen = []
    for i in candidates:
        if all(abs(i - j) > 2 * period for j in chosen):
            chosen.append(i)
    chosen.sort()

    events = []
    limit = int(0.6 * fs)
    for p in chosen:
        start = p + period
        while start < len(t) and forcing[start] > forcing_limit and start < p + 8 * period:
            start += 1
        if start >= len(t) or forcing[start] > forcing_limit:
            continue
        end = start
        lowest = env_all[start]
        while end < min(len(t) - 1, start + limit):
            end += 1
            lowest = min(lowest, env_all[end])
            if env_all[end] < 3 * noise:
                break
            if env_all[end] > 1.3 * lowest + noise or forcing[end] > forcing_limit:
                break
        if end - start < 3 * period:
            continue
        lead = slice(max(0, start - int(0.3 * fs)), start)
        dv = slow[lead].sum(axis=0) / fs * 1000  # mm/s
        trigger = int(np.argmax(np.abs(dv[:2])))
        if abs(dv[trigger]) < 10:
            trigger = None
        window = slice(start, end)
        tt = t[window] - t[start]
        fits = []
        for k in range(3):
            if env[start, k] < 4 * noise_axis[k]:
                fits.append(None)
                continue
            fit = fit_damped(tt, ringing[window, k], f0)
            if fit is None or fit[3] < 0.7 or fit[1] <= 0:
                fits.append(None)
                continue
            f, sigma, amplitude, r2 = fit
            wd = 2 * np.pi * f
            wn = math.hypot(wd, sigma)
            fits.append(
                {
                    "f": wn / (2 * np.pi),
                    "zeta": sigma / wn,
                    "acc_g": amplitude / G,
                    "mm": amplitude / wn**2 * 1000,
                    "r2": r2,
                }
            )
        if any(fits):
            events.append(
                {"t": t[start], "end": t[end], "trigger": trigger, "dv": dv, "fits": fits}
            )
    return events


def summary(items):
    """Gruppiert (Vorgang, Achse, Anpassung) nach auslösender und schwingender Achse."""
    groups = {}
    for event, k, fit in items:
        groups.setdefault((event["trigger"], k), []).append((event, fit))
    rows = []
    for (trigger, k), members in sorted(groups.items(), key=lambda kv: (kv[0][0] is None, kv[0])):
        fits = [fit for _, fit in members]
        rows.append(
            {
                "trigger": trigger,
                "axis": k,
                "count": len(members),
                "f": float(np.median([fit["f"] for fit in fits])),
                "f_spread": float(np.std([fit["f"] for fit in fits])),
                "zeta": float(np.median([fit["zeta"] for fit in fits])),
                "mm": float(np.median([fit["mm"] for fit in fits])),
                "mm_max": float(max(fit["mm"] for fit in fits)),
                "dv": float(np.median([abs(e["dv"][trigger]) for e, _ in members]))
                if trigger is not None
                else None,
            }
        )
    return rows


def advice(f, zeta):
    period = 1 / f
    lines = [
        f"Schwingungsdauer T = {period * 1000:.1f} ms (f = {f:.1f} Hz, ζ = {zeta:.3f}).",
        "Eine Rampe, die genau n·T dauert, regt kaum Restschwingung an.",
        "Verbundene Kurven (feste Rampe 0,1 g): Geschwindigkeitssprung je Knoten",
    ]
    for n in (1, 2, 3):
        dv = CURVE_RAMP * n * period
        lines.append(f"  n = {n}: Δv = {dv:6.1f} mm/s = {dv / NOMINAL_CUT_SPEED * 100:5.1f} %")
    lines.append(
        "Start/Stopp-Vektoren, falls der Controller mit vyacc 0,24 g rampt"
        " (Annahme, mit Δv-Spalte prüfen):"
    )
    n = 1
    while True:
        v = STOP_RAMP * n * period
        if v > NOMINAL_CUT_SPEED or n > 6:
            break
        lines.append(f"  n = {n}: v = {v:6.1f} mm/s = {v / NOMINAL_CUT_SPEED * 100:5.1f} %")
        n += 1
    if n == 1:
        lines.append("  keine Geschwindigkeit bis 100 % erreicht eine volle Schwingungsdauer")
    if zeta > 0:
        lines.append(f"Abklingen auf 10 %: {math.log(10) / (zeta * 2 * np.pi * f) * 1000:.0f} ms.")
    return lines


def strongest(result):
    """Zeile mit dem größten Ausschlag in X/Y über alle Resonanzen."""
    rows = [row for mode in result["modes"] for row in mode["rows"] if row["axis"] < 2]
    return max(rows, key=lambda row: row["mm_max"], default=None)


def report(path, meta, result, names):
    print(f"== {path}")
    if meta.get("notiz"):
        print(f"   Notiz: {meta['notiz']}")
    print(f"   Abtastrate {result['fs']:.1f} Hz, Dauer {result['duration']:.1f} s")
    if meta.get("ueberlaeufe", "0") != "0":
        print(f"   Achtung: {meta['ueberlaeufe']} FIFO-Überläufe bei der Aufnahme")
    print("   Stärkste Spitzen im Spektrum (Kandidaten):")
    for k in range(3):
        text = ", ".join(f"{f:.1f} Hz" for f, _, _ in result["spectrum"][k]) or "–"
        print(f"     {names[k]}: {text}")
    if not result["modes"]:
        print("   Keine Ausschwingvorgänge in X/Y gefunden.")
        return
    for mode in result["modes"]:
        print(f"   Resonanz bei {mode['f']:.1f} Hz, {mode['events']} Ausschwingvorgänge:")
        print("     Auslöser  schwingt  Anzahl   f (Hz)        ζ      Ausschlag (mm)   |Δv| (mm/s)")
        for row in mode["rows"]:
            trigger = names[row["trigger"]] if row["trigger"] is not None else "?"
            dv = f"{row['dv']:8.0f}" if row["dv"] is not None else "       –"
            print(
                f"     {trigger:8}  {names[row['axis']]:8}  {row['count']:6d}"
                f"   {row['f']:5.1f} ±{row['f_spread']:4.1f}  {row['zeta']:6.3f}"
                f"   {row['mm']:6.3f} (max {row['mm_max']:.3f})  {dv}"
            )
    row = strongest(result)
    if row:
        print("   Stärkste Schwingung in X/Y:")
        for line in advice(row["f"], row["zeta"]):
            print(f"     {line}")


def plot(path, t, acc, result, names):
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        return None
    fig, (top, middle, bottom) = plt.subplots(3, 1, figsize=(12, 10))
    acc = acc - np.median(acc, axis=0)
    for k in range(3):
        top.plot(t, acc[:, k] / G, lw=0.5, label=names[k])
    best = None
    for mode in result["modes"]:
        for event, k, fit in mode["items"]:
            top.axvspan(event["t"], event["end"], color="0.85", zorder=0)
            if k < 2 and (best is None or fit["mm"] > best[2]["mm"]):
                best = (event, k, fit)
    top.set_xlabel("Zeit (s)")
    top.set_ylabel("Beschleunigung (g)")
    top.set_title(f"{Path(path).name}: grau = ausgewertete Ausschwingvorgänge")
    top.legend(loc="upper right")
    band = (result["freqs"] > 1) & (
        result["freqs"] < min(0.45 * result["fs"], 1.5 * result["fmax"])
    )
    for k in range(3):
        middle.semilogy(result["freqs"][band], result["psd"][band, k], lw=0.8, label=names[k])
    for mode in result["modes"]:
        middle.axvline(mode["f"], color="k", ls="--", lw=0.8)
    middle.set_xlabel("Frequenz (Hz); gestrichelt = gefundene Resonanzen")
    middle.set_ylabel("Leistungsdichte ((m/s²)²/Hz)")
    middle.legend(loc="upper right")
    if best:
        event, k, fit = best
        window = (t >= event["t"]) & (t <= event["end"])
        f0 = fit["f"]
        ringing = fft_filter(acc, result["fs"], bandpass(0.6 * f0, 1.6 * f0))[window, k]
        tt = t[window] - event["t"]
        sigma = fit["zeta"] * 2 * np.pi * f0
        hull = fit["acc_g"] * np.exp(-sigma * tt)
        bottom.plot(tt * 1000, ringing / G, lw=0.8, label=f"gemessen {names[k]} (gefiltert)")
        bottom.plot(tt * 1000, hull, "k--", lw=0.8, label="angepasste Hüllkurve")
        bottom.plot(tt * 1000, -hull, "k--", lw=0.8)
        bottom.set_title(
            f"stärkster Vorgang bei {event['t']:.2f} s: {f0:.1f} Hz, ζ = {fit['zeta']:.3f},"
            f" {fit['mm']:.3f} mm"
        )
        bottom.set_xlabel("Zeit ab Fensterbeginn (ms)")
        bottom.set_ylabel("g")
        bottom.legend(loc="upper right")
    fig.tight_layout()
    target = Path(path).with_suffix(".png")
    fig.savefig(target, dpi=110)
    plt.close(fig)
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("dateien", nargs="+", type=Path)
    parser.add_argument(
        "--achsen",
        help="Sensor- auf Maschinenachsen, z. B. x=-y,y=x,z=z (Standard: gleich)",
    )
    parser.add_argument("--fmin", type=float, default=10.0, help="untere Frequenz (Hz)")
    parser.add_argument("--fmax", type=float, default=200.0, help="obere Frequenz (Hz)")
    parser.add_argument("--keine-bilder", action="store_true")
    args = parser.parse_args()
    try:
        matrix = axis_matrix(args.achsen)
    except ValueError as error:
        sys.exit(str(error))
    names = list(AXES)
    overview = []
    for path in args.dateien:
        try:
            t, acc, meta = load(path)
        except (OSError, ValueError) as error:
            print(f"== {path}: {error}")
            continue
        acc = acc @ matrix.T
        result = analyse(t, acc, args.fmin, args.fmax)
        report(path, meta, result, names)
        if not args.keine_bilder:
            image = plot(path, t, acc, result, names)
            if image:
                print(f"   Bild: {image}")
        print()
        overview.append((path, meta.get("notiz", ""), result))
    if len(overview) > 1:
        print("== Übersicht: Schwingung in X/Y je Datei und Resonanz")
        print("   Datei                         Auslöser→Achse    f (Hz)       ζ   max. mm  Notiz")
        for path, note, result in overview:
            for mode in result["modes"]:
                for row in mode["rows"]:
                    if row["axis"] >= 2:
                        continue
                    trigger = names[row["trigger"]] if row["trigger"] is not None else "?"
                    print(
                        f"   {path.name[:28]:28}  {trigger}→{names[row['axis']]:13}"
                        f"  {row['f']:6.1f}  {row['zeta']:6.3f}  {row['mm_max']:7.3f}  {note}"
                    )


if __name__ == "__main__":
    main()
