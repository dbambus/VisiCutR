//! Port of VisiCut's tools/cameraserver: answers every HTTP request with a
//! fresh photo taken by an external capture command, e.g.
//!
//!   visicutr-cameraserver -- gphoto2 --capture-image-and-download --stdout
//!   visicutr-cameraserver --output snapshot.jpg -- imagesnap -q snapshot.jpg
//!
//! Set the device's camera URL to http://<host>:8088/visicam.jpg.
//!
//! The server usually listens on the whole network, so every connection has
//! deadlines and size limits (a client that sends or reads slowly cannot
//! block the camera), the capture command is stopped after a while, and
//! `--user` with a password (`--password` or `VISICUTR_CAMERA_PASSWORD`)
//! requires HTTP Basic authentication, which VisiCutR sends when the
//! device has camera user and password.
use base64::Engine as _;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use visicut_core::{i18n::tr, tr};

/// Time for a client to send its request head, to take the answer, and
/// for the capture command.
#[derive(Clone, Copy)]
struct Timing {
    request: Duration,
    response: Duration,
    capture: Duration,
}

const TIMING: Timing = Timing {
    request: Duration::from_secs(10),
    response: Duration::from_secs(30),
    capture: Duration::from_secs(30),
};
const MAX_REQUEST_HEAD: u64 = 16 * 1024;
const MAX_PHOTO: u64 = 64 * 1024 * 1024;

struct Options {
    bind: String,
    port: u16,
    rotate: u16,
    output: Option<String>,
    command: Vec<String>,
    /// Expected `Authorization` header (HTTP Basic), if any.
    authorization: Option<String>,
}

fn usage() -> &'static str {
    tr(
        "Aufruf: visicutr-cameraserver [--bind ADRESSE] [--port 8088] \
        [--rotate 0|90|180|270] [--output DATEI] [--user NAME --password KENNWORT] \
        -- AUFNAHMEBEFEHL [ARGUMENTE …]\n\
        Ohne --output muss der Befehl das Bild (JPEG oder PNG) auf stdout schreiben. \
        Mit --user verlangt der Server eine Anmeldung; das Kennwort kann statt mit \
        --password auch in VISICUTR_CAMERA_PASSWORD stehen.",
    )
}

fn parse(
    args: impl Iterator<Item = String>,
    password_variable: Option<String>,
) -> Result<Options, String> {
    let mut options = Options {
        bind: "0.0.0.0".into(),
        port: 8088,
        rotate: 0,
        output: None,
        command: Vec::new(),
        authorization: None,
    };
    let (mut user, mut password) = (None, password_variable);
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| tr!("{option} benötigt einen Wert", option = arg))
        };
        match arg.as_str() {
            "--bind" => options.bind = value()?,
            "--port" => options.port = value()?.parse().map_err(|_| tr("Ungültiger Port"))?,
            "--rotate" => {
                options.rotate = value()?.parse().map_err(|_| tr("Ungültige Drehung"))?;
                if ![0, 90, 180, 270].contains(&options.rotate) {
                    return Err(tr("Drehung muss 0, 90, 180 oder 270 sein").into());
                }
            }
            "--output" => options.output = Some(value()?),
            "--user" => user = Some(value()?),
            "--password" => password = Some(value()?),
            "--" => options.command = args.by_ref().collect(),
            "-h" | "--help" => return Err(usage().into()),
            other => {
                return Err(format!(
                    "{}\n{}",
                    tr!("Unbekannte Option {option}", option = other),
                    usage()
                ));
            }
        }
    }
    if options.command.is_empty() {
        return Err(usage().into());
    }
    if let Some(user) = user {
        let password = password.filter(|p| !p.is_empty()).ok_or(tr(
            "--user benötigt ein Kennwort (--password oder VISICUTR_CAMERA_PASSWORD)",
        ))?;
        options.authorization = Some(format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
        ));
    }
    Ok(options)
}

/// Reads or writes a stream until `until`; each operation waits at most
/// for the time that is left, so a client that trickles bytes cannot hold
/// the connection longer.
struct Deadline<'a> {
    stream: &'a TcpStream,
    until: Instant,
}

impl Deadline<'_> {
    fn left(&self) -> std::io::Result<Duration> {
        self.until
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::TimedOut))
    }
}

impl Read for Deadline<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.set_read_timeout(Some(self.left()?))?;
        (&mut &*self.stream).read(buf)
    }
}

impl Write for Deadline<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stream.set_write_timeout(Some(self.left()?))?;
        (&mut &*self.stream).write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        (&mut &*self.stream).flush()
    }
}

fn capture(options: &Options, timing: Timing) -> Result<Vec<u8>, String> {
    if let Some(output) = &options.output {
        let _ = std::fs::remove_file(output);
    }
    let mut child = Command::new(&options.command[0])
        .args(&options.command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", options.command[0]))?;
    // Read both pipes on their own threads, limited, so the command never
    // blocks on a full pipe and a huge output cannot fill the memory.
    let collect = |pipe: Option<Box<dyn Read + Send>>, limit: u64| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = (&mut pipe).take(limit + 1).read_to_end(&mut bytes);
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            }
            bytes
        })
    };
    let stdout = collect(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        MAX_PHOTO,
    );
    let stderr = collect(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        4096,
    );
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timing.capture => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(tr!(
                    "Aufnahmebefehl nach {seconds} s abgebrochen",
                    seconds = timing.capture.as_secs_f32()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let (stdout, stderr) = (
        stdout.join().unwrap_or_default(),
        stderr.join().unwrap_or_default(),
    );
    if !status.success() {
        return Err(tr!(
            "Aufnahmebefehl fehlgeschlagen ({status}): {error}",
            status = status,
            error = String::from_utf8_lossy(&stderr).trim()
        ));
    }
    let photo = match &options.output {
        Some(output) => {
            let size = std::fs::metadata(output).map_err(|e| format!("{output}: {e}"))?;
            if size.len() > MAX_PHOTO {
                return Err(tr("Aufnahme ist größer als 64 MB").into());
            }
            std::fs::read(output).map_err(|e| format!("{output}: {e}"))?
        }
        None => stdout,
    };
    if photo.len() as u64 > MAX_PHOTO {
        return Err(tr("Aufnahme ist größer als 64 MB").into());
    }
    Ok(photo)
}

/// Returns the response body and content type, rotating like `mogrify -rotate`.
fn image_response(bytes: Vec<u8>, rotate: u16) -> Result<(Vec<u8>, &'static str), String> {
    let format = image::guess_format(&bytes).map_err(|_| tr("Aufnahme ist kein Bild"))?;
    if rotate == 0 {
        return Ok((bytes, format.to_mime_type()));
    }
    let picture = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rotated = match rotate {
        90 => picture.rotate90(),
        180 => picture.rotate180(),
        _ => picture.rotate270(),
    };
    let mut jpeg = Vec::new();
    rotated
        .to_rgb8()
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageFormat::Jpeg,
        )
        .map_err(|e| e.to_string())?;
    Ok((jpeg, "image/jpeg"))
}

fn handle(
    stream: TcpStream,
    take_photo: impl FnOnce() -> Result<Vec<u8>, String>,
    rotate: u16,
    authorization: Option<&str>,
    timing: Timing,
) -> std::io::Result<()> {
    // Consume the request head; the path does not matter.
    let mut reader = BufReader::new(Deadline {
        stream: &stream,
        until: Instant::now() + timing.request,
    })
    .take(MAX_REQUEST_HEAD);
    let mut line = String::new();
    let mut authorized = authorization.is_none();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let (Some(expected), Some((name, value))) = (authorization, line.split_once(':'))
            && name.trim().eq_ignore_ascii_case("authorization")
        {
            authorized = value.trim() == expected;
        }
    }
    if reader.limit() == 0 {
        return Err(std::io::Error::other(tr("Anfrage ist zu lang")));
    }
    let mut out = Deadline {
        stream: &stream,
        until: Instant::now() + timing.response,
    };
    if !authorized {
        write!(
            out,
            "HTTP/1.0 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"VisiCutR\"\r\n\
             Content-Length: 0\r\nConnection: close\r\n\r\n"
        )?;
        return out.flush();
    }
    match take_photo().and_then(|bytes| image_response(bytes, rotate)) {
        Ok((body, content_type)) => {
            out.until = Instant::now() + timing.response;
            write!(
                out,
                "HTTP/1.0 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
                 Cache-Control: no-store\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            out.write_all(&body)?;
        }
        Err(error) => {
            // Details only on the server's console, not to the network.
            eprintln!("{error}");
            let message = tr("Aufnahme fehlgeschlagen");
            out.until = Instant::now() + timing.response;
            write!(
                out,
                "HTTP/1.0 500 Internal Server Error\r\nContent-Type: text/plain; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{message}",
                message.len()
            )?;
        }
    }
    out.flush()
}

fn main() {
    // Ohne Einstellungen der Oberfläche gilt die Systemsprache.
    visicut_core::i18n::set_language(visicut_core::i18n::Language::system());
    let password = std::env::var("VISICUTR_CAMERA_PASSWORD").ok();
    let options = match parse(std::env::args().skip(1), password) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let listener = match TcpListener::bind((options.bind.as_str(), options.port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("{}:{}: {error}", options.bind, options.port);
            std::process::exit(1);
        }
    };
    println!(
        "{}",
        tr!(
            "Kameraserver läuft. Kamera-URL in VisiCutR: \
             http://<dieser Rechner>:{port}/visicam.jpg",
            port = options.port
        )
    );
    // One request at a time: a camera can only take one photo at once.
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let authorization = options.authorization.as_deref();
                let photo = || capture(&options, TIMING);
                if let Err(error) = handle(stream, photo, options.rotate, authorization, TIMING) {
                    eprintln!("{}", tr!("Anfrage fehlgeschlagen: {error}", error = error));
                }
            }
            Err(error) => eprintln!(
                "{}",
                tr!("Verbindung fehlgeschlagen: {error}", error = error)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::new(width, height)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    const FAST: Timing = Timing {
        request: Duration::from_millis(500),
        response: Duration::from_millis(500),
        capture: Duration::from_millis(300),
    };

    /// Sends `head` and returns the answer and what `handle` returned.
    fn exchange(
        take_photo: impl FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
        authorization: Option<&'static str>,
        head: &[u8],
    ) -> (Vec<u8>, std::io::Result<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, take_photo, 0, authorization, FAST)
        });
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let _ = client.write_all(head);
        let mut response = Vec::new();
        let _ = client.read_to_end(&mut response);
        (response, server.join().unwrap())
    }

    fn request(
        take_photo: impl FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
        rotate: u16,
    ) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, take_photo, rotate, None, TIMING).unwrap();
        });
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .write_all(b"GET /visicam.jpg HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        server.join().unwrap();
        response
    }

    fn body(response: &[u8]) -> &[u8] {
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        &response[split + 4..]
    }

    #[test]
    fn serves_captured_image_unchanged_or_rotated_as_jpeg() {
        let response = request(|| Ok(png(4, 2)), 0);
        assert!(response.starts_with(b"HTTP/1.0 200 OK\r\nContent-Type: image/png"));
        assert_eq!(body(&response), png(4, 2));

        let response = request(|| Ok(png(4, 2)), 90);
        assert!(response.starts_with(b"HTTP/1.0 200 OK\r\nContent-Type: image/jpeg"));
        let rotated = image::load_from_memory(body(&response)).unwrap();
        assert_eq!((rotated.width(), rotated.height()), (2, 4));
    }

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn reports_capture_errors_without_details_and_parses_options() {
        let response = request(|| Err("/dev/video0: Kamera fehlt".into()), 0);
        assert!(response.starts_with(b"HTTP/1.0 500"));
        // Details of the capture command stay on the server.
        assert_eq!(body(&response), "Aufnahme fehlgeschlagen".as_bytes());

        let list = [
            "--port", "9000", "--rotate", "180", "--", "gphoto2", "--stdout",
        ];
        let options = parse(args(&list), None).unwrap();
        assert_eq!((options.port, options.rotate), (9000, 180));
        assert_eq!(options.command, ["gphoto2", "--stdout"]);
        assert!(options.authorization.is_none());
        assert!(parse(args(&["--rotate", "45", "--", "x"]), None).is_err());
        assert!(parse(std::iter::empty(), None).is_err());
        // --user needs a password, from the option or the variable.
        assert!(parse(args(&["--user", "laser", "--", "x"]), None).is_err());
        let options = parse(args(&["--user", "laser", "--", "x"]), Some("geheim".into())).unwrap();
        assert_eq!(
            options.authorization.as_deref(),
            Some("Basic bGFzZXI6Z2VoZWlt")
        );
    }

    #[test]
    fn with_a_user_only_authenticated_requests_get_a_photo() {
        let expected = Some("Basic bGFzZXI6Z2VoZWlt");
        let (response, _) = exchange(|| Ok(png(2, 2)), expected, b"GET / HTTP/1.1\r\n\r\n");
        assert!(response.starts_with(b"HTTP/1.0 401"), "{response:?}");
        let (response, _) = exchange(
            || Ok(png(2, 2)),
            expected,
            b"GET / HTTP/1.1\r\nAuthorization: Basic d3Jvbmc6d3Jvbmc=\r\n\r\n",
        );
        assert!(response.starts_with(b"HTTP/1.0 401"));
        let (response, _) = exchange(
            || Ok(png(2, 2)),
            expected,
            b"GET / HTTP/1.1\r\nauthorization:  Basic bGFzZXI6Z2VoZWlt\r\n\r\n",
        );
        assert!(response.starts_with(b"HTTP/1.0 200"));
    }

    #[test]
    fn long_or_slow_requests_are_dropped() {
        // No end of line: stops at 16 KB instead of growing.
        let (response, result) = exchange(
            || panic!("no photo for an invalid request"),
            None,
            &vec![b'a'; 64 * 1024],
        );
        assert!(response.is_empty() && result.is_err());
        // A client that sends one byte at a time cannot keep the camera
        // busy beyond the request time.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let started = Instant::now();
            let result = handle(stream, || Ok(png(1, 1)), 0, None, FAST);
            (result, started.elapsed())
        });
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        for _ in 0..20 {
            if client.write_all(b"G").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let (result, elapsed) = server.join().unwrap();
        assert!(result.is_err());
        assert!(elapsed < Duration::from_millis(900), "{elapsed:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_hanging_capture_command_is_stopped() {
        let options = parse(args(&["--", "sleep", "10"]), None).unwrap();
        let started = Instant::now();
        let error = capture(&options, FAST).unwrap_err();
        assert!(error.contains("abgebrochen"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
