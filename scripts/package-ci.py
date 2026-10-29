#!/usr/bin/env python3
"""Package host-built CI artifacts. Run from any working directory."""
import argparse
import json
import shutil
import subprocess
import tarfile
import zipfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=[
        "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
    ])
    target = parser.parse_args().target
    project = Path(__file__).resolve().parent.parent
    dist = project / "dist"
    dist.mkdir(exist_ok=True)
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
    ], cwd=project))
    version = metadata["packages"][0]["version"]
    name = f"VisiCutR-{version}-{target}"

    if target.endswith("apple-darwin"):
        app = dist / "VisiCutR.app"
        if not (app / "Contents/MacOS/visicutr").is_file():
            parser.error("Build the application bundle with bundle-macos.sh first")
        archive = dist / f"{name}.zip"
        subprocess.run([
            "ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(archive),
        ], check=True)
    else:
        stage = dist / name
        if stage.exists():
            shutil.rmtree(stage)
        stage.mkdir(exist_ok=True)
        suffix = ".exe" if "windows" in target else ""
        binary = project / "target/release" / f"visicutr{suffix}"
        if not binary.is_file():
            parser.error("Build the portable application with cargo build --release first")
        shutil.copy2(binary, stage / f"visicutr{suffix}")
        server = project / "target/release" / f"visicutr-cameraserver{suffix}"
        if not server.is_file():
            parser.error("Build all binaries with cargo build --release --bins first")
        shutil.copy2(server, stage / server.name)
        for source in [project / "README.md", project / "COPYING",
                       project / "COPYING.LESSER"]:
            shutil.copy2(source, stage / source.name)
        # Die Anleitung (Wiki-Seiten ohne die Prüfberichte).
        (stage / "docs").mkdir()
        for page in sorted((project / "docs").glob("*.md")):
            shutil.copy2(page, stage / "docs" / page.name)
        # Inkscape-Erweiterung zum Installieren von Hand; VisiCutR installiert
        # sie auch selbst (Einstellungen > Allgemein).
        shutil.copytree(project / "resources/inkscape", stage / "inkscape-extension",
                        ignore=shutil.ignore_patterns("__pycache__"))
        if "windows" in target:
            # Illustrator-Skript und CorelDRAW-Makro; VisiCutR speichert sie
            # auch selbst (Einstellungen > Software-Add-ins). Byteweise
            # kopieren: das Makro braucht CRLF.
            for source, folder in [
                (project / "resources/illustrator/VisiCutR.jsx", "illustrator-script"),
                (project / "resources/coreldraw/VisiCutR.bas", "coreldraw-macro"),
            ]:
                (stage / folder).mkdir()
                shutil.copy2(source, stage / folder / source.name)
        if "linux" in target:
            # Anwendungsmenü und Dateizuordnungen für ~/.local/share.
            linux = project / "resources/linux"
            shutil.copy2(linux / "install-desktop.sh", stage / "install-desktop.sh")
            desktop = stage / "desktop"
            desktop.mkdir()
            for source in [linux / "visicutr.desktop", linux / "visicutr-mime.xml",
                           project / "resources/icon/visicutr.svg"]:
                shutil.copy2(source, desktop / source.name)
            shutil.copy2(project / "resources/icon/icon.png", desktop / "visicutr.png")
            associations = (
                "Run ./install-desktop.sh once to add VisiCutR to the application menu\n"
                "and open .vcr, .plf and .ls files by double-click (SVG, PDF, DXF and EPS\n"
                "under \"Open with\"); ./install-desktop.sh --uninstall removes it again.\n"
            )
        elif "windows" in target:
            associations = (
                "illustrator-script/ holds a script for Adobe Illustrator (CS5.1 or later)\n"
                "and coreldraw-macro/ a VBA module for CorelDRAW (2017 or later) that send\n"
                "the drawing to VisiCutR; see docs/Illustrator-und-CorelDRAW.md.\n"
                "Settings > General > File types associates .vcr, .plf and .ls files with\n"
                "this visicutr.exe for the current user (SVG, PDF, DXF and EPS under\n"
                "\"Open with\"); the same page removes the association again.\n"
            )
        (stage / "START-HERE.txt").write_text(
            "VisiCutR - egui interface (the same on Windows, Linux and macOS)\n\n"
            f"Start visicutr{suffix}. No Java is required.\n"
            "Features: object assignment, material\n"
            "library, text/shape/sign tools, copies, nesting, job preview with timeline,\n"
            "LTT iLaser 4000 devices, rotary jobs and the calibrated camera background.\n"
            "inkscape-extension/ holds the Inkscape extension \"Add to VisiCutR\";\n"
            "VisiCutR installs it under Settings > Software add-ins.\n"
            f"visicutr-cameraserver{suffix} serves photos from a capture command.\n"
            "Linux requires a graphical X11/Wayland session, OpenGL/EGL, libxkbcommon,\n"
            "and a desktop portal for file dialogs. Built on Ubuntu 24.04 (glibc 2.39).\n"
            "See README.md and docs/ for features, docs/LTT-Protokoll.md for hardware status.\n"
            + associations,
            encoding="utf-8",
        )
        if "windows" in target:
            archive = dist / f"{name}.zip"
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for path in sorted(stage.rglob("*")):
                    output.write(path, f"{name}/{path.relative_to(stage).as_posix()}")
        else:
            archive = dist / f"{name}.tar.gz"
            with tarfile.open(archive, "w:gz") as output:
                output.add(stage, arcname=name)
    print(archive)


if __name__ == "__main__":
    main()
