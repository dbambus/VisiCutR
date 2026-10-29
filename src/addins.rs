//! Skript für Adobe Illustrator (CS5.1 oder neuer, `resources/illustrator`)
//! und Makro für CorelDRAW (2017 oder neuer, `resources/coreldraw`). Beide
//! schicken die Zeichnung wie die Inkscape-Erweiterung an VisiCutR. Anders
//! als bei Inkscape gibt es keinen Benutzerordner, den die Programme von
//! selbst lesen: VisiCutR speichert die Dateien, wohin der Benutzer möchte,
//! und merkt sich bei jedem Start seinen Programmpfad im Einstellungsordner,
//! wo beide nachsehen.

use std::path::Path;

/// Programmpfad für Skript und Makro, im Einstellungsordner.
pub const PATH_FILE: &str = "visicutr-path.txt";

/// Illustrator-Skript (ExtendScript): Name und Inhalt.
pub const ILLUSTRATOR: (&str, &str) = (
    "VisiCutR.jsx",
    include_str!("../resources/illustrator/VisiCutR.jsx"),
);

/// CorelDRAW-Makro (VBA-Modul zum Importieren): Name und Inhalt. Der
/// Makro-Editor braucht CRLF; `.gitattributes` hält die Datei so.
pub const CORELDRAW: (&str, &str) = (
    "VisiCutR.bas",
    include_str!("../resources/coreldraw/VisiCutR.bas"),
);

/// Merkt `program` in `dir/visicutr-path.txt`; schreibt nur bei Änderung.
pub fn remember_program(dir: &Path, program: &Path) -> std::io::Result<()> {
    let path = dir.join(PATH_FILE);
    let text = program.to_string_lossy();
    if std::fs::read(&path).is_ok_and(|bytes| bytes == text.as_bytes()) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{Listener, Request};
    use std::io::{BufRead, BufReader, Write};
    use std::path::PathBuf;

    fn temporary(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("visicut-addins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn remembers_the_program_path() {
        let dir = temporary("path");
        let program = Path::new("C:\\Programme\\VisiCutR Ü\\visicutr.exe");
        remember_program(&dir, program).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(PATH_FILE)).unwrap(),
            program.to_string_lossy()
        );
        remember_program(&dir, Path::new("/anderswo/visicutr")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(PATH_FILE)).unwrap(),
            "/anderswo/visicutr"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Illustrator liest Skripte ohne BOM, der Makro-Editor Module in der
    /// Systemcodierung und nur mit CRLF.
    #[test]
    fn scripts_are_ascii_and_the_macro_has_crlf() {
        for (name, content) in [ILLUSTRATOR, CORELDRAW] {
            assert!(content.is_ascii(), "{name}: nur ASCII");
            assert!(content.contains(PATH_FILE), "{name}");
            assert!(content.contains("--update") || content.contains("\"update\""));
        }
        let lines: Vec<&str> = CORELDRAW.1.split('\n').collect();
        assert!(
            lines[..lines.len() - 1].iter().all(|l| l.ends_with('\r')),
            "{}: CRLF",
            CORELDRAW.0
        );
        assert!(
            CORELDRAW
                .1
                .starts_with("Attribute VB_Name = \"VisiCutR\"\r\n")
        );
        for name in ["Public Sub AddToVisiCutR()", "Public Sub OpenInVisiCutR()"] {
            assert!(CORELDRAW.1.contains(name), "{name}");
        }
        assert!(ILLUSTRATOR.1.starts_with("#target illustrator"));
    }

    /// Prüft die reinen Funktionen des Illustrator-Skripts und seine Anfrage
    /// an eine echte Instanz, sofern Node.js vorhanden ist.
    #[test]
    fn illustrator_script_reaches_the_running_instance() {
        let node = std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !node {
            eprintln!("node fehlt, Skripttest übersprungen");
            return;
        }
        let dir = temporary("illustrator");
        let listener = Listener::start(&dir, || {}).unwrap();
        let address = std::fs::read_to_string(dir.join("instance.json")).unwrap();
        let svg = "C:\\Users\\Jürgen\\Temp\\visicutr-1\\Schild \"A\".svg";
        // `#target` ist eine Anweisung für ExtendScript, kein JavaScript.
        let script: String = ILLUSTRATOR
            .1
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let check = format!(
            r#"{script}
var assert = require("assert");
var v = VisiCutR;
assert.strictEqual(v.documentName("Tür & Schild.ai"), "Tur  Schild");
assert.strictEqual(v.documentName("Größe.v2.ai"), "Grosse.v2");
assert.strictEqual(v.documentName("...ai"), "Zeichnung");
assert.strictEqual(v.documentName("Unbenannt-1"), "Unbenannt-1");
assert.strictEqual(v.launchTarget("/Applications/VisiCutR.app/Contents/MacOS/visicutr"), "/Applications/VisiCutR.app");
assert.strictEqual(v.launchTarget("C:\\VisiCutR\\visicutr.exe"), "C:\\VisiCutR\\visicutr.exe");
assert.strictEqual(v.parseAddress("kaputt"), null);
var address = v.parseAddress({address:?});
process.stdout.write(v.requestLine(address.token, true, true, [{svg:?}]));
"#
        );
        let output = std::process::Command::new("node")
            .args(["-e", &check])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let line = String::from_utf8(output.stdout).unwrap();
        assert!(line.is_ascii(), "{line}");
        let port: serde_json::Value = serde_json::from_str(&address).unwrap();
        let mut stream =
            std::net::TcpStream::connect(("127.0.0.1", port["port"].as_u64().unwrap() as u16))
                .unwrap();
        stream.write_all(format!("{line}\n").as_bytes()).unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        assert_eq!(reply, "OK\n");
        let request = (0..50)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                listener.try_recv()
            })
            .unwrap();
        assert_eq!(
            request,
            Request {
                add: true,
                update: true,
                files: vec![PathBuf::from(svg)],
                ..Request::default()
            }
        );
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
