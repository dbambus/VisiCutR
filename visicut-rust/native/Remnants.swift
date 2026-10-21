import AppKit
import SwiftUI

/// Ein erkanntes Reststück in Bett-Millimetern (y nach unten), wie
/// `remnant::Remnant` im Rust-Kern.
struct Remnant: Codable, Equatable {
    let outline: [[Double]]
    let holes: [[[Double]]]
    let area_mm2: Double
    var contours: [[[Double]]] { [outline] + holes }
}

/// Antwort von `detect_remnants`; `bounds` je Reststück `[x, y, w, h]` in mm.
struct RemnantDetection: Decodable {
    let remnants: [Remnant]
    let bounds: [[Double]]
    let threshold: Int
    let has_reference: Bool
}

struct RemnantReference: Decodable { let has_reference: Bool }

/// Woran die Erkennung Material vom Bett unterscheidet (`method` der Brücke).
enum RemnantMethod: String, CaseIterable, Identifiable {
    case reference, lighter, darker
    var id: String { rawValue }
    var title: String {
        switch self {
        case .reference: return tr("Unterschied zum Foto des leeren Betts")
        case .lighter: return tr("Material heller als das Bett")
        case .darker: return tr("Material dunkler als das Bett")
        }
    }
}

/// Umrissfarbe der Reststücke (keine Bearbeitungsfarbe).
let remnantColor = NSColor(calibratedRed: 0, green: 0.75, blue: 0.35, alpha: 1)

extension AppModel {
    /// Öffnet „Reststücke erkennen“ und blendet das Kamerabild ein.
    func openRemnants() {
        guard !device.camera_url.isEmpty, !busy else { return }
        if !showCamera { toggleCamera() }
        showRemnants = true
    }

    /// Fügt die Umrisse als Schneid-Teil an ihrer Stelle ein (widerrufbar).
    func insertRemnants(_ remnants: [Remnant]) -> Bool {
        guard !busy, let values = try? RustCore.json(remnants) else { return false }
        guard compose("insert_remnants", values: ["remnants": values]) else { return false }
        // Schneid-Parameter aus dem Materialprofil statt der Werte des bisherigen Teils.
        if preset != nil {
            applyPreset()
            status = tr("Reststücke als Teil eingefügt · Bearbeiten › Widerrufen macht es rückgängig")
        } else {
            status = tr("Reststücke als Teil eingefügt; kein Schneid-Profil für dieses Material: Leistung und Geschwindigkeit prüfen")
        }
        return true
    }

    /// Zeichnet die Umrisse auf das Arbeitsbett (`bed` in Punkten, `scale` Punkte je mm).
    func drawRemnants(in bed: NSRect, scale: Double) {
        guard !remnants.isEmpty else { return }
        let path = NSBezierPath()
        for remnant in remnants {
            for contour in remnant.contours {
                for (i, p) in contour.enumerated() {
                    let point = NSPoint(x: bed.minX + p[0] * scale, y: bed.minY + p[1] * scale)
                    if i == 0 { path.move(to: point) } else { path.line(to: point) }
                }
                path.close()
            }
        }
        path.lineJoinStyle = .round
        path.lineWidth = 3.5; NSColor(calibratedWhite: 0, alpha: 0.6).setStroke(); path.stroke()
        path.lineWidth = 1.8; remnantColor.setStroke(); path.stroke()
    }
}

struct RemnantSheet: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var method = RemnantMethod.lighter
    @State private var autoThreshold = true
    @State private var threshold = 128.0
    @State private var smoothing = 3.0
    @State private var margin = 2.0
    @State private var minArea = 4.0
    @State private var hasReference = false
    @State private var result: RemnantDetection?
    @State private var loading = false
    @State private var pending = false
    @State private var problem: String?

    private var found: Bool { !(result?.remnants.isEmpty ?? true) }
    private var bedSize: CGSize { CGSize(width: model.project.bed_width_mm, height: model.project.bed_height_mm) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(tr("Reststücke erkennen")).font(.title2).bold()
                Spacer()
                if model.cameraLoading {
                    ProgressView().controlSize(.small)
                    Text(tr("Kamerabild wird geladen …")).foregroundStyle(.secondary)
                }
                Button(tr("Kamerabild aktualisieren")) { model.refreshCamera() }.disabled(model.cameraLoading)
            }
            HStack(alignment: .top, spacing: 20) {
                picture
                controls.frame(width: 320)
            }
            Divider()
            HStack {
                if loading { ProgressView().controlSize(.small) }
                Spacer()
                Button(tr("Verwerfen")) {
                    model.remnants = []
                    dismiss()
                }.keyboardShortcut(.cancelAction)
                Button(tr("Als Teil einfügen")) {
                    guard let remnants = result?.remnants else { return }
                    if model.insertRemnants(remnants) { dismiss() }
                }
                .help(tr("Fügt die Umrisse als Schneid-Teil an ihrer Stelle ein, z. B. zum Zuschneiden. Löschen, wenn es nicht gelasert werden soll."))
                .disabled(!found)
                Button(tr("Auf dem Bett anzeigen")) {
                    model.remnants = result?.remnants ?? []
                    dismiss()
                }
                .help(tr("Umrisse bleiben nach dem Schließen sichtbar, bis sie verworfen werden."))
                .keyboardShortcut(.defaultAction).disabled(!found)
            }
        }
        .padding(24).frame(width: 980, height: 520)
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear {
            if queryReference() { method = .reference }
            refresh()
        }
        .onChange(of: method) { refresh() }
        .onChange(of: autoThreshold) { refresh() }
        .onChange(of: threshold) { if !autoThreshold { refresh() } }
        .onChange(of: smoothing) { refresh() }
        .onChange(of: margin) { refresh() }
        .onChange(of: minArea) { refresh() }
        .onChange(of: model.cameraPNG) { refresh() }
    }

    /// Kamerabild mit den erkannten Umrissen, im Seitenverhältnis des Betts.
    private var picture: some View {
        VStack(spacing: 6) {
            ZStack {
                Color(nsColor: .controlBackgroundColor)
                if let camera = model.cameraImage {
                    Image(nsImage: camera).resizable().interpolation(.high)
                } else {
                    Text(tr("Noch kein Kamerabild")).foregroundStyle(.secondary)
                }
                Canvas { context, size in
                    guard let remnants = result?.remnants, bedSize.width > 0, bedSize.height > 0 else { return }
                    let sx = size.width / bedSize.width, sy = size.height / bedSize.height
                    var path = Path()
                    for remnant in remnants {
                        for contour in remnant.contours {
                            path.addLines(contour.map { CGPoint(x: $0[0] * sx, y: $0[1] * sy) })
                            path.closeSubpath()
                        }
                    }
                    context.stroke(path, with: .color(.black.opacity(0.6)), lineWidth: 3.5)
                    context.stroke(path, with: .color(Color(nsColor: remnantColor)), lineWidth: 1.8)
                    for (remnant, bounds) in zip(remnants, result?.bounds ?? []) where bounds.count == 4 {
                        let center = CGPoint(x: (bounds[0] + bounds[2] / 2) * sx, y: (bounds[1] + bounds[3] / 2) * sy)
                        context.draw(Text("\(Int((remnant.area_mm2 / 100).rounded())) cm²").font(.caption).bold()
                            .foregroundColor(Color(nsColor: remnantColor)), at: center)
                    }
                }
            }
            .aspectRatio(bedSize.width / max(bedSize.height, 1), contentMode: .fit)
            .frame(width: 600)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.3), lineWidth: 1))
            Text(tr("Die Umrisse stammen aus dem Kamerabild und sind nur so genau wie dessen Kalibrierung."))
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(tr("Material erkennen")).font(.headline)
            Picker(tr("Material erkennen"), selection: $method) {
                ForEach(RemnantMethod.allCases) { method in
                    Text(method.title).tag(method).disabled(method == .reference && !hasReference)
                }
            }
            .pickerStyle(.radioGroup).labelsHidden()
            HStack {
                Button(tr("Leeres Bett als Referenz speichern")) { saveReference() }
                    .disabled(model.cameraPNG == nil)
                    .help(tr("Bett vorher leer räumen. Die Erkennung vergleicht danach jedes Kamerabild mit diesem Foto und findet auch Material in der Farbe des Betts."))
                if hasReference { Button(tr("Referenz löschen")) { deleteReference() } }
            }
            Divider()
            Toggle(tr("Schwellwert automatisch"), isOn: $autoThreshold)
            LabeledContent(tr("Schwellwert")) {
                HStack(spacing: 6) {
                    Slider(value: $threshold, in: 0...255, step: 1) { Text(tr("Schwellwert")) }
                        .labelsHidden().frame(width: 130)
                    Text(String(Int(threshold.rounded()))).monospacedDigit().frame(width: 32, alignment: .trailing)
                }
            }.disabled(autoThreshold)
            NumberField(tr("Glättung"), value: $smoothing, unit: "mm")
                .help(tr("Gleicht Wabe, Lamellen und Kratzer aus; größere Werte runden Ecken ab."))
            NumberField(tr("Sicherheitsabstand"), value: $margin, unit: "mm")
                .help(tr("Die Umrisse liegen um diesen Abstand innerhalb der erkannten Kante."))
            NumberField(tr("Mindestfläche"), value: $minArea, unit: "cm²")
                .help(tr("Kleinere Reststücke werden ignoriert."))
            Divider()
            status.font(.callout)
            Spacer(minLength: 0)
        }
    }

    @ViewBuilder private var status: some View {
        if let problem {
            Label(problem, systemImage: "exclamationmark.triangle").foregroundStyle(.orange)
                .fixedSize(horizontal: false, vertical: true)
        } else if let result, let largest = result.bounds.first, largest.count == 4 {
            Text(tr("{count} Reststück(e) · größtes etwa {width} × {height} mm",
                    ["count": result.remnants.count, "width": Int(largest[2].rounded()), "height": Int(largest[3].rounded())]))
                .monospacedDigit()
        } else if result != nil {
            Text(tr("Kein Reststück gefunden; Einstellungen anpassen")).foregroundStyle(.orange)
        } else if loading {
            Text(tr("Erkennung läuft …")).foregroundStyle(.secondary)
        }
    }

    private var values: [String: Any] {
        [
            "device_name": model.device.name,
            "method": method.rawValue,
            "threshold": autoThreshold ? NSNull() as Any : Int(threshold.rounded()) as Any,
            "smoothing_mm": max(smoothing, 0),
            "margin_mm": max(margin, 0),
            "min_area_mm2": max(minArea, 0) * 100,
        ]
    }

    /// Schreibt das aktuelle Kamerabild für die Brücke in eine temporäre Datei.
    private func cameraFile() throws -> URL? {
        guard let png = model.cameraPNG else { return nil }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("visicut-remnants-camera.png")
        try png.write(to: url)
        return url
    }

    /// Rechnet im Hintergrund. Änderungen während einer laufenden Erkennung
    /// werden danach einmal mit den dann aktuellen Einstellungen nachgeholt.
    private func refresh() {
        if loading { pending = true; return }
        let file: URL
        do {
            guard let url = try cameraFile() else { result = nil; return }
            file = url
        } catch { problem = error.localizedDescription; return }
        loading = true
        pending = false
        let values = self.values, project = model.project
        DispatchQueue.global(qos: .userInitiated).async {
            let outcome = Result { try RustCore.decode("detect_remnants", project: project, path: file.path, values: values) as RemnantDetection }
            DispatchQueue.main.async {
                loading = false
                switch outcome {
                case .success(let detection):
                    result = detection
                    hasReference = detection.has_reference
                    if autoThreshold { threshold = Double(detection.threshold) }
                    problem = nil
                case .failure(let error):
                    result = nil
                    problem = error.localizedDescription
                }
                if pending { refresh() }
            }
        }
    }

    /// Fragt, ob für den Lasercutter ein Referenzbild gespeichert ist.
    private func queryReference() -> Bool {
        let response: RemnantReference? = try? RustCore.decode("remnant_reference", values: ["device_name": model.device.name])
        hasReference = response?.has_reference ?? false
        return hasReference
    }

    private func saveReference() {
        do {
            guard let file = try cameraFile() else { return }
            let response: RemnantReference = try RustCore.decode("remnant_reference", path: file.path, values: ["device_name": model.device.name])
            hasReference = response.has_reference
            method = .reference
            model.status = tr("Referenzbild des leeren Betts gespeichert")
        } catch {
            problem = tr("Referenzbild nicht gespeichert: {error}", ["error": error.localizedDescription])
        }
    }

    private func deleteReference() {
        do {
            let response: RemnantReference = try RustCore.decode("remnant_reference", values: ["device_name": model.device.name, "delete": true])
            hasReference = response.has_reference
            if method == .reference { method = .lighter }
        } catch { problem = error.localizedDescription }
    }
}

/// Reststück-Erkennung über die Brücke mit einem erzeugten Kamerabild; das
/// Einfügen wird widerrufen, das Projekt bleibt unverändert.
@MainActor
func testRemnants(_ model: AppModel) throws {
    let file = FileManager.default.temporaryDirectory.appendingPathComponent("visicut-remnants-\(UUID().uuidString).png")
    let pixels = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 500, pixelsHigh: 300, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    guard let bytes = pixels.bitmapData else { throw CoreError("Testbild ohne Pixel") }
    for y in 0..<300 {
        for x in 0..<500 {
            // Helles Reststück (100/50 mm, 300 × 200 mm) auf dunklem Bett, 0,5 px/mm.
            let piece = (50..<200).contains(x) && (25..<125).contains(y)
            let offset = y * pixels.bytesPerRow + x * 4
            bytes[offset] = piece ? 210 : 35; bytes[offset + 1] = piece ? 180 : 35
            bytes[offset + 2] = piece ? 130 : 35; bytes[offset + 3] = 255
        }
    }
    try pixels.representation(using: .png, properties: [:])!.write(to: file)
    defer { try? FileManager.default.removeItem(at: file) }
    var bed = model.project
    bed.bed_width_mm = 1000; bed.bed_height_mm = 600
    let detection: RemnantDetection = try RustCore.decode("detect_remnants", project: bed, path: file.path,
        values: ["device_name": "Reststück-Test", "method": "lighter", "threshold": NSNull(),
                 "smoothing_mm": 4.0, "margin_mm": 0.0, "min_area_mm2": 400.0])
    guard detection.remnants.count == 1, let bounds = detection.bounds.first, abs(bounds[0] - 100) < 3, abs(bounds[2] - 300) < 5
    else { throw CoreError("Reststück falsch erkannt: \(detection.bounds)") }
    func partCount() -> Int { model.project.parts.count + (model.project.hasArtwork ? 1 : 0) }
    let parts = partCount()
    guard model.insertRemnants(detection.remnants), partCount() == parts + 1,
          model.project.operation == .cut,
          // Das Teil umfasst nur das Reststück und liegt an dessen Stelle.
          abs(model.project.x_mm - bounds[0]) < 0.01, abs(model.project.y_mm - bounds[1]) < 0.01,
          abs(model.project.width_mm - bounds[2]) < 0.01, abs(model.project.height_mm - bounds[3]) < 0.01
    else { throw CoreError("Reststücke nicht als Teil eingefügt: \(model.project.x_mm), \(model.project.y_mm), \(model.project.width_mm) × \(model.project.height_mm)") }
    model.undo()
    guard partCount() == parts else { throw CoreError("Einfügen der Reststücke nicht widerrufbar") }
}
