//! Inkscape-Erweiterung „Add to VisiCutR“ / „Open in VisiCutR“ (Ordner
//! `resources/inkscape`). VisiCutR installiert sie wie Java-VisiCut in das
//! Erweiterungsverzeichnis von Inkscape 1.x und merkt dort seinen eigenen
//! Programmpfad, damit das Skript VisiCutR findet.

use std::path::{Path, PathBuf};

/// Unterordner im Erweiterungsverzeichnis.
pub const FOLDER: &str = "visicutr";
/// Programmpfad für das Skript (siehe `visicutr_export.py`).
const PATH_FILE: &str = "visicutr-path.txt";

/// Dateien der Erweiterung, eingebettet ins Programm.
pub const FILES: [(&str, &str); 3] = [
    (
        "visicutr_add.inx",
        include_str!("../resources/inkscape/visicutr_add.inx"),
    ),
    (
        "visicutr_open.inx",
        include_str!("../resources/inkscape/visicutr_open.inx"),
    ),
    (
        "visicutr_export.py",
        include_str!("../resources/inkscape/visicutr_export.py"),
    ),
];

/// Benutzer-Erweiterungsverzeichnis von Inkscape 1.x.
pub fn extensions_dir() -> Option<PathBuf> {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        Some(
            var("HOME")?.join(
                "Library/Application Support/org.inkscape.Inkscape/config/inkscape/extensions",
            ),
        )
    } else if cfg!(windows) {
        Some(var("APPDATA")?.join("inkscape").join("extensions"))
    } else {
        let config = var("XDG_CONFIG_HOME").or_else(|| Some(var("HOME")?.join(".config")))?;
        Some(config.join("inkscape").join("extensions"))
    }
}

/// Stand der Erweiterung in `extensions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Missing,
    /// Andere Fassung oder anderer Programmpfad: neu installieren.
    Outdated,
    Current,
}

pub fn status(extensions: &Path, program: &Path) -> Status {
    let dir = extensions.join(FOLDER);
    if !dir.join(FILES[2].0).is_file() {
        return Status::Missing;
    }
    let same = |name: &str, content: &[u8]| {
        std::fs::read(dir.join(name)).is_ok_and(|bytes| bytes == content)
    };
    let current = FILES
        .iter()
        .all(|(name, content)| same(name, content.as_bytes()))
        && same(PATH_FILE, program.to_string_lossy().as_bytes());
    if current {
        Status::Current
    } else {
        Status::Outdated
    }
}

/// Schreibt die Erweiterung nach `extensions/visicutr` und merkt `program`.
/// Liefert den Zielordner.
pub fn install(extensions: &Path, program: &Path) -> Result<PathBuf, String> {
    let dir = extensions.join(FOLDER);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path_text = program.to_string_lossy();
    for (name, content) in FILES
        .iter()
        .copied()
        .chain([(PATH_FILE, path_text.as_ref())])
    {
        let path = dir.join(name);
        std::fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join(FILES[2].0);
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("visicut-inkscape-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn installs_files_and_program_path() {
        let extensions = temporary("install");
        let program = Path::new("/Programme/VisiCutR.app/Contents/MacOS/visicutr");
        assert_eq!(status(&extensions, program), Status::Missing);
        let dir = install(&extensions, program).unwrap();
        assert_eq!(dir, extensions.join(FOLDER));
        assert_eq!(status(&extensions, program), Status::Current);
        assert_eq!(
            std::fs::read_to_string(dir.join(PATH_FILE)).unwrap(),
            program.to_string_lossy()
        );
        // Verschobenes Programm: neu installieren.
        assert_eq!(
            status(&extensions, Path::new("/anderswo/visicutr")),
            Status::Outdated
        );
        std::fs::write(dir.join("visicutr_add.inx"), "alt").unwrap();
        assert_eq!(status(&extensions, program), Status::Outdated);
        install(&extensions, program).unwrap();
        assert_eq!(status(&extensions, program), Status::Current);
        let _ = std::fs::remove_dir_all(&extensions);
    }

    #[test]
    fn inx_files_call_the_script_with_add_or_replace() {
        for (name, content, add) in [
            (FILES[0].0, FILES[0].1, "true"),
            (FILES[1].0, FILES[1].1, "false"),
        ] {
            let document = roxmltree::Document::parse(content).unwrap();
            let param = document
                .descendants()
                .find(|n| n.attribute("name") == Some("add"))
                .unwrap_or_else(|| panic!("{name}: Parameter add"));
            assert_eq!(param.text(), Some(add), "{name}");
            let command = document
                .descendants()
                .find(|n| n.has_tag_name("command"))
                .unwrap();
            assert_eq!(command.text(), Some(FILES[2].0));
            assert_eq!(command.attribute("location"), Some("inx"));
        }
    }

    /// Prüft das Skript ohne Inkscape, sofern Python vorhanden ist.
    #[test]
    fn script_parses_arguments_and_finds_visicutr() {
        let dir = temporary("script");
        let program = dir.join("bin").join("visicutr");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, "").unwrap();
        let installed = install(&dir.join("ext"), &program).unwrap();
        let svg = dir.join("ink_ext_XXXX.svg");
        std::fs::write(&svg, r#"<svg sodipodi:docname="Schild &amp; Tür.svg"/>"#).unwrap();
        let check = format!(
            r#"
import sys
sys.path.insert(0, {installed:?})
import visicutr_export as v
add, ids, source = v.parse_arguments(["--add=false", "--id=a", "--selected-nodes=", "--id=b", "z.svg"])
assert (add, ids, source) == (False, ["a", "b"], "z.svg"), (add, ids, source)
assert v.parse_arguments(["--add=true", "z.svg"])[0]
found = v.find_visicutr({installed:?}, {{"PATH": ""}}, "linux")
assert found == {program:?}, found
assert v.find_visicutr({empty:?}, {{"VISICUTR_BIN": {program:?}, "PATH": ""}}, "darwin") == {program:?}
assert v.find_visicutr({empty:?}, {{"PATH": ""}}, "win32") is None
name = v.document_name({svg:?})
assert name == "Schild  Tur.svg", name
cmd = v.inkscape_command("inkscape", (1, 3), "x.svg", ["a", "b"])
assert "select-by-id:a,b" in cmd[-1] and "export-id-only:true" in cmd[-1], cmd
assert "select-all:all" in v.inkscape_command("inkscape", (1, 2), "x.svg", [])[-1]
assert v.inkscape_command("inkscape", (1, 0), "x.svg", [])[2] == "--batch-process"
"#,
            installed = installed.to_string_lossy(),
            program = program.to_string_lossy(),
            empty = dir.to_string_lossy(),
            svg = svg.to_string_lossy(),
        );
        // Unter Windows kann `python3` ein Platzhalter des Stores sein.
        let python = std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !python {
            eprintln!("python3 fehlt, Skripttest übersprungen");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let output = std::process::Command::new("python3")
            .args(["-I", "-B", "-c", &check])
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
