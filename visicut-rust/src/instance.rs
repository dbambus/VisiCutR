//! Befehlszeile und Einzelinstanz, wie `ApplicationInstanceManager` in
//! Java-VisiCut. Die laufende Oberfläche wartet auf einem Port an 127.0.0.1
//! und legt Port und ein Zufallskennwort im Einstellungsordner ab. Ein zweiter
//! Start (`visicutr datei.svg`, `visicutr --add datei.svg`, die
//! Inkscape-Erweiterung) gibt seine Dateien dorthin weiter und beendet sich;
//! antwortet niemand, startet er selbst die Oberfläche.
//!
//! Protokoll: eine JSON-Zeile `{"token", "add", "files"}`, Antwort `OK`.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Port und Kennwort der laufenden Oberfläche im Einstellungsordner.
const FILE: &str = "instance.json";
/// Längste Anfrage; Pfade sind kurz.
const MAX_REQUEST: u64 = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Dateien aus der Befehlszeile oder von einem zweiten Start.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Als Teile hinzufügen statt das Projekt zu ersetzen (`--add`).
    pub add: bool,
    /// Absolute Pfade; leer holt nur das Fenster nach vorn.
    pub files: Vec<PathBuf>,
}

/// Ausgewertete Befehlszeile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// Beispiel öffnen (`--demo`).
    pub demo: bool,
    /// Bildschirmaufnahme (`--capture`, `--page`): eigene Instanz, nichts
    /// weitergeben und keinen Port belegen.
    pub standalone: bool,
    pub request: Request,
}

impl Launch {
    /// Ob die Dateien an eine laufende Oberfläche gehen sollen.
    pub fn forwards(&self) -> bool {
        !self.demo && !self.standalone
    }
}

/// Hilfe für `--help` und falsche Aufrufe.
pub const USAGE: &str = "Aufruf: visicutr [--add] [DATEI …] | visicutr --demo\n  \
DATEI     Projekt oder Grafik öffnen (mehrere werden als Teile hinzugefügt)\n  \
--add     Grafiken als Teile zum offenen Projekt hinzufügen\n  \
--demo    Beispiel öffnen\n\
Läuft VisiCutR schon, öffnet das dortige Fenster die Dateien.";

/// Wertet die Argumente ohne Programmnamen aus. Relative Pfade gelten ab
/// `cwd`. `Err` enthält die Meldung für die Konsole.
pub fn parse(arguments: impl IntoIterator<Item = String>, cwd: &Path) -> Result<Launch, String> {
    let mut launch = Launch::default();
    let mut arguments = arguments.into_iter();
    let mut only_files = false;
    while let Some(argument) = arguments.next() {
        if only_files || !argument.starts_with('-') {
            launch.request.files.push(cwd.join(argument));
            continue;
        }
        match argument.as_str() {
            "--" => only_files = true,
            "--add" => launch.request.add = true,
            "--demo" => launch.demo = true,
            // Bildschirmaufnahmen (Feature `screenshot`) mit Wert.
            "--capture" | "--page" => {
                launch.standalone = true;
                arguments.next();
            }
            "-h" | "--help" => return Err(USAGE.into()),
            // Ältere macOS-Versionen hängen beim Start aus dem Finder eine
            // Prozessnummer an.
            other if other.starts_with("-psn_") => {}
            other => return Err(format!("Unbekannte Option {other}\n{USAGE}")),
        }
    }
    Ok(launch)
}

#[derive(Serialize, Deserialize)]
struct Address {
    port: u16,
    token: String,
}

#[derive(Serialize, Deserialize)]
struct Message {
    token: String,
    #[serde(flatten)]
    request: Request,
}

/// Gibt `request` an die laufende Oberfläche weiter, deren Adresse in `dir`
/// liegt. `true`, wenn sie die Anfrage angenommen hat.
pub fn forward(dir: &Path, request: &Request) -> bool {
    let Some(address) = std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Address>(&bytes).ok())
    else {
        return false;
    };
    send(&address, request).is_ok()
}

fn send(address: &Address, request: &Request) -> std::io::Result<()> {
    let socket = SocketAddr::from((Ipv4Addr::LOCALHOST, address.port));
    let mut stream = TcpStream::connect_timeout(&socket, CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
    stream.set_write_timeout(Some(REPLY_TIMEOUT))?;
    let message = Message {
        token: address.token.clone(),
        request: request.clone(),
    };
    let mut line = serde_json::to_vec(&message)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    let mut reply = String::new();
    BufReader::new(stream).take(16).read_line(&mut reply)?;
    if reply.trim_end() == "OK" {
        Ok(())
    } else {
        Err(std::io::Error::other("keine Antwort von VisiCutR"))
    }
}

/// Wartet auf Anfragen späterer Starts; beim Beenden entfernt es seine
/// Adresse wieder.
pub struct Listener {
    receiver: Receiver<Request>,
    file: PathBuf,
    token: String,
}

impl Listener {
    /// Öffnet einen freien Port an 127.0.0.1 und legt die Adresse in `dir`
    /// ab. `notify` läuft nach jeder Anfrage (weckt die Oberfläche).
    pub fn start(dir: &Path, notify: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let token = random_token();
        let address = Address {
            port: server.local_addr()?.port(),
            token: token.clone(),
        };
        std::fs::create_dir_all(dir)?;
        let file = dir.join(FILE);
        write_private(&file, &serde_json::to_vec(&address)?)?;
        let (sender, receiver) = std::sync::mpsc::channel();
        let expected = token.clone();
        std::thread::spawn(move || {
            for stream in server.incoming() {
                let Ok(stream) = stream else { continue };
                if let Some(request) = receive(stream, &expected) {
                    if sender.send(request).is_err() {
                        break;
                    }
                    notify();
                }
            }
        });
        Ok(Self {
            receiver,
            file,
            token,
        })
    }

    /// Nächste Anfrage, falls eine wartet.
    pub fn try_recv(&self) -> Option<Request> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Nur die eigene Adresse entfernen, nicht die einer neueren Instanz.
        let own = std::fs::read(&self.file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Address>(&bytes).ok())
            .is_some_and(|address| address.token == self.token);
        if own {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

/// Liest eine Anfrage und bestätigt sie; falsches Kennwort oder Unsinn
/// bleiben unbeantwortet.
fn receive(stream: TcpStream, token: &str) -> Option<Request> {
    stream.set_read_timeout(Some(REPLY_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(REPLY_TIMEOUT)).ok()?;
    let mut reader = BufReader::new(stream.try_clone().ok()?).take(MAX_REQUEST);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let message: Message = serde_json::from_str(&line).ok()?;
    if message.token != token {
        return None;
    }
    (&stream).write_all(b"OK\n").ok()?;
    Some(message.request)
}

/// 128 Bit aus den zufälligen Schlüsseln der Standardbibliothek.
fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut token = String::new();
    for _ in 0..2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        hasher.write_u32(std::process::id());
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
}

/// Schreibt die Adresse nur für den eigenen Benutzer lesbar.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("visicut-instance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_files_add_and_demo() {
        let cwd = Path::new("/arbeit");
        assert_eq!(parse(args(&[]), cwd).unwrap(), Launch::default());
        let launch = parse(args(&["--add", "a.svg", "/tmp/b.svg"]), cwd).unwrap();
        assert!(launch.request.add && launch.forwards());
        assert_eq!(
            launch.request.files,
            [PathBuf::from("/arbeit/a.svg"), PathBuf::from("/tmp/b.svg")]
        );
        // Reihenfolge egal; nach `--` ist alles ein Dateiname.
        let launch = parse(args(&["x.vcr", "-psn_0_123", "--add", "--", "--demo"]), cwd).unwrap();
        assert!(launch.request.add && !launch.demo);
        assert_eq!(launch.request.files.len(), 2);
        let demo = parse(args(&["--demo"]), cwd).unwrap();
        assert!(demo.demo && !demo.forwards());
    }

    #[test]
    fn capture_options_skip_their_value_and_stay_local() {
        let launch = parse(
            args(&["--capture", "bild.png", "--page", "job"]),
            Path::new("/"),
        )
        .unwrap();
        assert!(launch.standalone && !launch.forwards());
        assert!(launch.request.files.is_empty());
    }

    #[test]
    fn rejects_unknown_options_with_usage() {
        let error = parse(args(&["--singleinstanceport=1"]), Path::new("/")).unwrap_err();
        assert!(error.contains("--singleinstanceport") && error.contains("--add"));
        assert_eq!(parse(args(&["--help"]), Path::new("/")).unwrap_err(), USAGE);
    }

    #[test]
    fn forwards_requests_to_the_running_instance() {
        let dir = temporary("forward");
        let request = Request {
            add: true,
            files: vec![PathBuf::from("/tmp/ä b.svg")],
        };
        // Niemand läuft: selbst starten.
        assert!(!forward(&dir, &request));
        let (woken, wake) = std::sync::mpsc::channel();
        let listener = Listener::start(&dir, move || {
            let _ = woken.send(());
        })
        .unwrap();
        assert!(forward(&dir, &request));
        wake.recv_timeout(REPLY_TIMEOUT).unwrap();
        assert_eq!(listener.try_recv(), Some(request));
        assert!(forward(&dir, &Request::default()));
        wake.recv_timeout(REPLY_TIMEOUT).unwrap();
        assert_eq!(listener.try_recv(), Some(Request::default()));
        assert_eq!(listener.try_recv(), None);
        // Beenden entfernt die Adresse; danach startet der nächste selbst.
        drop(listener);
        assert!(!dir.join(FILE).exists());
        assert!(!forward(&dir, &Request::default()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_wrong_tokens_and_stale_addresses() {
        let dir = temporary("token");
        let listener = Listener::start(&dir, || {}).unwrap();
        let file: Address =
            serde_json::from_slice(&std::fs::read(dir.join(FILE)).unwrap()).unwrap();
        let forged = Address {
            port: file.port,
            token: "falsch".into(),
        };
        assert!(send(&forged, &Request::default()).is_err());
        assert_eq!(listener.try_recv(), None);
        // Eine neuere Instanz hat die Adresse überschrieben: nicht löschen.
        let (woken, wake) = std::sync::mpsc::channel();
        let newer = Listener::start(&dir, move || {
            let _ = woken.send(());
        })
        .unwrap();
        drop(listener);
        assert!(dir.join(FILE).exists());
        assert!(forward(&dir, &Request::default()));
        // Die Bestätigung kommt vor der Übergabe an die Oberfläche: warten.
        wake.recv_timeout(REPLY_TIMEOUT).unwrap();
        assert_eq!(newer.try_recv(), Some(Request::default()));
        // Verwaiste Adresse (Absturz): Port ist zu, also selbst starten.
        drop(newer);
        let closed = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let stale = Address {
            port: closed.local_addr().unwrap().port(),
            token: "alt".into(),
        };
        drop(closed);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), serde_json::to_vec(&stale).unwrap()).unwrap();
        assert!(!forward(&dir, &Request::default()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
