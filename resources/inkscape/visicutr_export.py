#!/usr/bin/env python3
"""Inkscape-Erweiterung "Add to VisiCutR" / "Open in VisiCutR".

Speichert die Zeichnung (oder nur die Auswahl) als SVG, wandelt Texte und
Klone mit Inkscape in Pfade um und übergibt die Datei an VisiCutR
(`visicutr [--add] -- datei.svg`). Läuft VisiCutR schon, öffnet das dortige
Fenster die Datei; sonst startet VisiCutR.

Nach dem Vorbild von tools/inkscape_extension/visicut_export.py aus
Java-VisiCut (Thomas Oster, Max Gaukler). Teil von VisiCutR,
LGPL-3.0-or-later.
"""

import os
import random
import re
import shutil
import string
import subprocess
import sys
import tempfile
import unicodedata

# Installiert VisiCutR die Erweiterung, steht hier sein Programmpfad.
PATH_FILE = "visicutr-path.txt"


def parse_arguments(argv):
    """Inkscape ruft `skript --add=true --id=a --id=b … zeichnung.svg` auf.

    Liefert (hinzufügen, ausgewählte IDs, Eingabedatei)."""
    add = True
    ids = []
    source = None
    for argument in argv:
        if argument.startswith("--id="):
            ids.append(argument[len("--id="):])
        elif argument.startswith("--add="):
            add = argument[len("--add="):].lower() == "true"
        elif argument.startswith("-"):
            # Weitere Optionen von Inkscape (z. B. --selected-nodes).
            pass
        else:
            source = argument
    return add, ids, source


def find_visicutr(script_dir=None, environ=None, system=None):
    """Programmpfad von VisiCutR oder None.

    Reihenfolge: Umgebungsvariable VISICUTR_BIN, der beim Installieren
    gemerkte Pfad, übliche Orte je Plattform, PATH."""
    script_dir = script_dir or os.path.dirname(os.path.abspath(__file__))
    environ = os.environ if environ is None else environ
    system = system or sys.platform
    candidates = []
    if environ.get("VISICUTR_BIN"):
        candidates.append(environ["VISICUTR_BIN"])
    try:
        with open(os.path.join(script_dir, PATH_FILE), encoding="utf-8") as stored:
            candidates.append(stored.read().strip())
    except OSError:
        pass
    home = os.path.expanduser("~")
    if system == "darwin":
        bundle = os.path.join("VisiCutR.app", "Contents", "MacOS", "visicutr")
        candidates += [
            os.path.join("/Applications", bundle),
            os.path.join(home, "Applications", bundle),
        ]
        names = ["visicutr"]
    elif system.startswith("win"):
        for base in ("LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"):
            if environ.get(base):
                sub = "Programs" if base == "LOCALAPPDATA" else ""
                candidates.append(os.path.join(environ[base], sub, "VisiCutR", "visicutr.exe"))
        names = ["visicutr.exe", "visicutr"]
    else:
        candidates += [
            os.path.join(home, ".local", "bin", "visicutr"),
            "/opt/visicutr/visicutr",
            "/usr/local/bin/visicutr",
            "/usr/bin/visicutr",
        ]
        names = ["visicutr"]
    for candidate in candidates:
        if candidate and os.path.isfile(candidate):
            return candidate
    for name in names:
        found = shutil.which(name, path=environ.get("PATH"))
        if found:
            return found
    return None


def find_inkscape():
    """Inkscape selbst; Inkscape 1.x setzt INKSCAPE_COMMAND für Erweiterungen."""
    command = os.environ.get("INKSCAPE_COMMAND")
    if command and (os.path.isfile(command) or shutil.which(command)):
        return command
    found = shutil.which("inkscape")
    if found:
        return found
    for candidate in (
        "/Applications/Inkscape.app/Contents/MacOS/inkscape",
        os.path.join(os.environ.get("ProgramFiles", ""), "Inkscape", "bin", "inkscape.exe"),
    ):
        if os.path.isfile(candidate):
            return candidate
    return None


def document_name(svg_path):
    """Dateiname für VisiCutR: Inkscape speichert unter einem Zufallsnamen,
    der eigentliche steht in sodipodi:docname."""
    name = None
    try:
        with open(svg_path, encoding="utf-8", errors="replace") as svg:
            match = re.search(r'sodipodi:docname="([^"]*)"', svg.read())
        if match:
            name = match.group(1)
            for entity, char in (("&lt;", "<"), ("&gt;", ">"), ("&quot;", '"'), ("&amp;", "&")):
                name = name.replace(entity, char)
    except OSError:
        pass
    if not name:
        name = os.path.basename(svg_path)
        if name.startswith("ink_ext_"):
            name = "Zeichnung"
    if name.lower().endswith(".svg"):
        name = name[:-4]
    name = unicodedata.normalize("NFKD", name).encode("ASCII", "ignore").decode("ASCII")
    name = "".join(c for c in name if c.isalnum() or c in "._- ").strip(" .")
    return (name or "Zeichnung") + ".svg"


def inkscape_version(inkscape):
    output = subprocess.check_output(
        [inkscape, "--version"], stderr=subprocess.DEVNULL, universal_newlines=True
    )
    match = re.search(r"Inkscape (\d+)\.(\d+)", output)
    return (int(match.group(1)), int(match.group(2))) if match else (1, 2)


def inkscape_command(inkscape, version, path, ids):
    """Inkscape-Aufruf, der nur die Auswahl behält und Texte, Klone und Formen
    in Pfade umwandelt (Ablauf wie in Java-VisiCut)."""
    if version >= (1, 2):
        actions = ["unlock-all"]
        actions.append("select-by-id:" + ",".join(ids) if ids else "select-all:all")
        actions += ["clone-unlink", "object-to-path"]
        if ids:
            group = "VISICUTR-" + "".join(random.sample(string.ascii_lowercase, 20))
            actions += [
                "selection-group",
                "object-set-attribute:id," + group,
                "export-id-only:true",
                "export-id:" + group,
            ]
        actions.append("export-area-page")
        return [inkscape, path, "--export-overwrite", "--actions=" + ";".join(actions)]
    # Inkscape 1.0 und 1.1 kennen nur Verben.
    command = [inkscape, path, "--batch-process"]
    verbs = ["ObjectToPath", "UnlockAllInAllLayers"]
    if ids:
        command.append("--select=" + ",".join(ids))
    else:
        verbs.append("EditSelectAllInAllLayers")
    verbs += [
        "UnhideAllInAllLayers", "EditInvertInAllLayers", "EditDelete",
        "EditSelectAllInAllLayers", "EditUnlinkClone", "ObjectToPath", "FileSave",
    ]
    return command + ["--verb=" + ";".join(verbs)]


def prepare(source, ids, target):
    """Schreibt die bereinigte Zeichnung nach `target`."""
    # Inkscape 1.3 startet sonst nicht aus einer Erweiterung heraus.
    os.environ["SELF_CALL"] = "true"
    work = os.path.join(os.path.dirname(target), "arbeit.svg")
    shutil.copyfile(source, work)
    inkscape = find_inkscape()
    if inkscape is None:
        sys.stderr.write("Inkscape nicht gefunden: VisiCutR erhält die ganze Zeichnung ohne Umwandlung.\n")
    else:
        try:
            command = inkscape_command(inkscape, inkscape_version(inkscape), work, ids)
            subprocess.check_output(command, stderr=subprocess.STDOUT, universal_newlines=True)
        except (OSError, subprocess.CalledProcessError) as error:
            sys.stderr.write(
                "Inkscape konnte die Zeichnung nicht vorbereiten; VisiCutR zeigt sie "
                "womöglich unvollständig.\n" + str(getattr(error, "output", error)) + "\n"
            )
    os.replace(work, target)


def launch(visicutr, add, path):
    """Startet VisiCutR losgelöst von Inkscape. Läuft es schon, gibt der neue
    Aufruf die Datei weiter und endet sofort; Fehler meldet Inkscape."""
    # --update: Ist die Zeichnung schon ein Teil, ersetzt VisiCutR nur dessen
    # Grafik; Lage und Zuordnung bleiben.
    command = [visicutr, "--update"] + (["--add"] if add else []) + ["--", path]
    # Fehlerausgabe in eine Datei: eine Pipe bräche nach dem Ende dieses
    # Skripts und die Oberfläche könnte nicht mehr schreiben.
    log_path = os.path.join(os.path.dirname(path), "visicutr.log")
    with open(log_path, "wb") as log:
        options = {
            "stdin": subprocess.DEVNULL,
            "stdout": log,
            "stderr": log,
            "close_fds": True,
            "cwd": os.path.dirname(path),
        }
        if os.name == "nt":
            # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
            options["creationflags"] = 0x00000008 | 0x00000200
        else:
            options["start_new_session"] = True
        process = subprocess.Popen(command, **options)
    try:
        code = process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        # Die Oberfläche läuft.
        return 0
    if code != 0:
        with open(log_path, encoding="utf-8", errors="replace") as log:
            sys.stderr.write("VisiCutR ({}) meldet:\n{}".format(visicutr, log.read()))
    return code


def main(argv):
    add, ids, source = parse_arguments(argv)
    if not source:
        sys.stderr.write("Keine Zeichnung von Inkscape erhalten.\n")
        return 1
    visicutr = find_visicutr()
    if visicutr is None:
        sys.stderr.write(
            "VisiCutR nicht gefunden. Bitte die Erweiterung aus VisiCutR heraus neu "
            "installieren (Einstellungen → Allgemein) oder VISICUTR_BIN setzen.\n"
        )
        return 1
    # Bleibt liegen: VisiCutR liest die Datei beim Öffnen und erneut beim Neuladen.
    target = os.path.join(tempfile.mkdtemp(prefix="visicutr-"), document_name(source))
    prepare(source, ids, target)
    return launch(visicutr, add, target)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
