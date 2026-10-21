//! Dateien, die das Betriebssystem übergibt: unter Windows und Linux als
//! Startargument, unter macOS als Apple-Event „Dokumente öffnen“, das winit
//! nicht weiterreicht. Das Ereignis kommt beim Start vor dem ersten Bild und
//! bei laufender App; die Pfade warten hier, bis die Oberfläche sie abholt.

use eframe::egui;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Was der Start laut Kommandozeile tun soll.
#[derive(Debug, PartialEq, Eq)]
pub enum Startup {
    Demo,
    Open(PathBuf),
}

/// Schalter der Bildschirmfoto-Aufnahme, deren Wert keine Datei ist.
const OPTIONS_WITH_VALUE: [&str; 2] = ["--capture", "--page"];

/// Erste Datei oder `--demo` aus den Argumenten (ohne Programmnamen).
pub fn startup(arguments: &[String]) -> Option<Startup> {
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--demo" => return Some(Startup::Demo),
            option if OPTIONS_WITH_VALUE.contains(&option) => {
                rest.next();
            }
            // Ältere macOS-Versionen hängen `-psn_…` an.
            option if option.starts_with('-') => {}
            path => return Some(Startup::Open(PathBuf::from(path))),
        }
    }
    None
}

static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REPAINT: OnceLock<egui::Context> = OnceLock::new();

/// Neu übergebene Dateien wecken ab jetzt die Oberfläche.
pub fn watch(ctx: &egui::Context) {
    let _ = REPAINT.set(ctx.clone());
}

/// Seit dem letzten Aufruf übergebene Dateien.
pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()))
}

// Nur das macOS-Ereignis liefert Dateien nachträglich.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn deliver(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend(paths);
    if let Some(ctx) = REPAINT.get() {
        ctx.request_repaint();
    }
}

/// Fängt die „Öffnen“-Ereignisse ab; vor `eframe::run_native` aufrufen.
#[cfg(target_os = "macos")]
pub fn install() {
    apple::install();
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}

#[cfg(target_os = "macos")]
mod apple {
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{AllocAnyThread, define_class, msg_send, sel};
    use objc2_foundation::{
        NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSString,
    };

    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "VisiCutROpenDocuments"]
        struct Handler;

        impl Handler {
            /// AppKit setzt seine eigene Behandlung kurz vorher; danach
            /// überschreibt sie dieser Handler, wie Apple es vorsieht.
            #[unsafe(method(applicationWillFinishLaunching:))]
            fn will_finish_launching(&self, _notification: &NSNotification) {
                let manager = NSAppleEventManager::sharedAppleEventManager();
                let _: () = unsafe {
                    msg_send![
                        &manager,
                        setEventHandler: self,
                        andSelector: sel!(openDocuments:withReplyEvent:),
                        forEventClass: CORE_EVENT_CLASS,
                        andEventID: OPEN_DOCUMENTS
                    ]
                };
            }

            #[unsafe(method(openDocuments:withReplyEvent:))]
            fn open_documents(
                &self,
                event: &NSAppleEventDescriptor,
                _reply: &NSAppleEventDescriptor,
            ) {
                let files: Option<Retained<NSAppleEventDescriptor>> =
                    unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
                let Some(files) = files else {
                    return;
                };
                // Meist eine Liste, bei einer Datei auch der Eintrag selbst.
                let items: Vec<_> = match files.numberOfItems() {
                    0 => vec![files],
                    n => (1..=n).filter_map(|i| files.descriptorAtIndex(i)).collect(),
                };
                super::deliver(
                    items
                        .iter()
                        .filter_map(|item| item.fileURLValue()?.to_file_path())
                        .collect(),
                );
            }
        }
    );

    pub fn install() {
        let handler: Retained<Handler> = unsafe { msg_send![Handler::alloc(), init] };
        let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &handler,
                sel!(applicationWillFinishLaunching:),
                Some(&name),
                None,
            );
        }
        // Mitteilungszentrale und Apple-Event-Manager halten keine starke
        // Referenz; der Handler lebt bis zum Programmende.
        std::mem::forget(handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn startup_opens_the_first_file_argument() {
        assert_eq!(startup(&args(&[])), None);
        assert_eq!(
            startup(&args(&["C:\\Users\\Lab\\Schild.svg"])),
            Some(Startup::Open(PathBuf::from("C:\\Users\\Lab\\Schild.svg")))
        );
        assert_eq!(
            startup(&args(&["/home/lab/mit Leerzeichen.vcr", "zweite.svg"])),
            Some(Startup::Open(PathBuf::from(
                "/home/lab/mit Leerzeichen.vcr"
            )))
        );
        assert_eq!(
            startup(&args(&["-psn_0_12345", "/Users/lab/Teil.dxf"])),
            Some(Startup::Open(PathBuf::from("/Users/lab/Teil.dxf")))
        );
        assert_eq!(startup(&args(&["--demo"])), Some(Startup::Demo));
        // Werte der Aufnahme-Schalter sind keine Dateien.
        assert_eq!(
            startup(&args(&["--capture", "bild.png", "--page", "general"])),
            None
        );
        assert_eq!(
            startup(&args(&["--capture", "bild.png", "projekt.vcr"])),
            Some(Startup::Open(PathBuf::from("projekt.vcr")))
        );
    }

    #[test]
    fn delivered_files_wait_until_taken() {
        deliver(vec![PathBuf::from("a.svg"), PathBuf::from("b.plf")]);
        deliver(Vec::new());
        assert_eq!(take(), vec![PathBuf::from("a.svg"), PathBuf::from("b.plf")]);
        assert!(take().is_empty());
    }
}
