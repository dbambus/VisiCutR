import AppKit
import SwiftUI

/// Neues Release von VisiCutRust (visicut_core::update::Release).
struct UpdateRelease: Decodable { let version: String; let url: String; let notes: String }
struct UpdateResponse: Decodable { let release: UpdateRelease?; let current: String }

/// Update-Prüfung: alle 14 Tage beim Start, abschaltbar unter Einstellungen → Allgemein.
@MainActor
enum UpdateCheck {
    static let interval: TimeInterval = 14 * 24 * 60 * 60
    private static let disabledKey = "updateCheckDisabled"
    private static let lastKey = "lastUpdateCheck"

    static var enabled: Bool {
        get { !UserDefaults.standard.bool(forKey: disabledKey) }
        set { UserDefaults.standard.set(!newValue, forKey: disabledKey) }
    }
    static var lastCheck: Date? {
        let seconds = UserDefaults.standard.double(forKey: lastKey)
        return seconds > 0 ? Date(timeIntervalSince1970: seconds) : nil
    }
    /// Nie geprüft, 14 Tage vorbei oder die Uhr lief rückwärts.
    static var due: Bool {
        guard let last = lastCheck else { return true }
        let age = Date().timeIntervalSince(last)
        return age < 0 || age >= interval
    }

    /// Fragt im Hintergrund bei GitHub. Von Hand gestartet meldet sie auch
    /// „aktuell“ und Fehler; automatisch nur ein neues Release.
    static func run(manual: Bool, model: AppModel) {
        if manual { model.status = tr("Suche nach Updates …") }
        DispatchQueue.global(qos: .utility).async {
            let result = Result { try RustCore.decode("check_update") as UpdateResponse }
            DispatchQueue.main.async { MainActor.assumeIsolated {
                switch result {
                case .success(let response):
                    UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: lastKey)
                    if let release = response.release {
                        announce(release, current: response.current)
                    } else if manual {
                        model.status = tr("VisiCutRust {version} ist aktuell", ["version": response.current])
                        let alert = NSAlert()
                        alert.messageText = tr("VisiCutRust {version} ist aktuell", ["version": response.current])
                        alert.runModal()
                    }
                case .failure(let error):
                    if manual { model.error = error.localizedDescription } else { model.status = error.localizedDescription }
                }
            } }
        }
    }

    private static func announce(_ release: UpdateRelease, current: String) {
        let alert = NSAlert()
        alert.messageText = tr("Update verfügbar")
        let notes = release.notes.count > 600 ? String(release.notes.prefix(600)) + " …" : release.notes
        alert.informativeText = tr("VisiCutRust {version} ist verfügbar (installiert: {current}).",
                                   ["version": release.version, "current": current])
            + (notes.isEmpty ? "" : "\n\n" + notes)
        alert.addButton(withTitle: tr("Download-Seite öffnen"))
        alert.addButton(withTitle: tr("Später"))
        if alert.runModal() == .alertFirstButtonReturn, let url = URL(string: release.url) {
            NSWorkspace.shared.open(url)
        }
    }
}

/// Reiter „Allgemein“ im Einstellungsfenster.
struct GeneralSettings: View {
    @ObservedObject var model: AppModel
    @State private var enabled = UpdateCheck.enabled
    @State private var lastCheck = UpdateCheck.lastCheck

    var body: some View {
        Form {
            Section(tr("Updates")) {
                Toggle(tr("Alle 14 Tage nach Updates suchen"), isOn: $enabled)
                    .onChange(of: enabled) { _, value in UpdateCheck.enabled = value }
                    .help(tr("Fragt bei GitHub nach einem neuen Release von VisiCutRust; es wird nichts automatisch installiert."))
                LabeledContent(tr("Installiert"), value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–")
                LabeledContent(tr("Zuletzt geprüft"), value: lastCheck.map { $0.formatted(date: .abbreviated, time: .shortened) } ?? tr("noch nie"))
                Button(tr("Jetzt nach Updates suchen")) {
                    UpdateCheck.run(manual: true, model: model)
                    DispatchQueue.main.asyncAfter(deadline: .now() + 3) { lastCheck = UpdateCheck.lastCheck }
                }
            }
            Section(tr("Lasercutter")) {
                HStack {
                    Button(tr("Importieren …")) { _ = model.importDevices() }
                    Menu(tr("Herunterladen")) {
                        ForEach(model.labs, id: \.url) { lab in
                            Button(lab.name) { _ = model.downloadDevices(lab) }
                        }
                    }.fixedSize()
                    // Exportiert die gesicherten Geräte, nicht den Entwurf im Reiter „Lasercutter“.
                    Button(tr("Exportieren …")) { model.exportDevices(model.devices.devices) }
                }
            }
        }
        .formStyle(.grouped)
        .frame(minWidth: 760, minHeight: 520)
    }
}
