import AppKit
import SwiftUI
import UniformTypeIdentifiers
import Combine

@_silgen_name("visicut_execute")
private func rustExecute(_ request: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
@_silgen_name("visicut_free")
private func rustFree(_ response: UnsafeMutablePointer<CChar>)

enum RustCore {
    static func call(_ action: String, project: Project? = nil, path: String? = nil, values: [String: Any] = [:]) throws -> Data {
        var request = values
        request["action"] = action
        if let project {
            request["project"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(project))
        }
        if let path { request["path"] = path }
        let json = String(decoding: try JSONSerialization.data(withJSONObject: request), as: UTF8.self)
        let data: Data = try json.withCString { pointer in
            guard let response = rustExecute(pointer) else { throw CoreError(tr("Rust hat keine Antwort geliefert")) }
            defer { rustFree(response) }
            return Data(String(cString: response).utf8)
        }
        guard let response = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw CoreError(tr("Ungültige Antwort des Rust-Kerns"))
        }
        guard response["ok"] as? Bool == true else { throw CoreError(response["error"] as? String ?? tr("Verarbeitung fehlgeschlagen")) }
        return try JSONSerialization.data(withJSONObject: response["result"] ?? [:])
    }

    static func decode<T: Decodable>(_ action: String, project: Project? = nil, path: String? = nil, values: [String: Any] = [:]) throws -> T {
        try JSONDecoder().decode(T.self, from: call(action, project: project, path: path, values: values))
    }

    static func json<T: Encodable>(_ value: T) throws -> Any {
        try JSONSerialization.jsonObject(with: JSONEncoder().encode(value), options: .fragmentsAllowed)
    }
}

/// Sprache der Oberfläche. Der deutsche Text ist der Schlüssel; die Tabellen
/// liefert der Rust-Kern (resources/i18n, dieselben wie unter Windows/Linux).
/// `tr(_:)` übersetzt, `tr(_:_:)` füllt zusätzlich benannte Platzhalter wie `{name}`.
enum Localization {
    struct Language: Decodable, Identifiable { let code: String; let name: String; var id: String { code } }
    private struct Response: Decodable { let language: String; let table: [String: String]; let languages: [Language] }
    private(set) static var table: [String: String] = [:]
    private(set) static var current = "de"
    private(set) static var languages: [Language] = []
    /// Gewählte Sprache; nil folgt der Systemsprache.
    static var chosen: String? { UserDefaults.standard.string(forKey: "language") }

    static func choose(_ code: String?) {
        if let code { UserDefaults.standard.set(code, forKey: "language") }
        else { UserDefaults.standard.removeObject(forKey: "language") }
        load()
    }

    /// Lädt die Tabelle; der UI-Test läuft immer auf Deutsch.
    static func load() {
        let code = CommandLine.arguments.contains("--ui-test") ? "de" : chosen
        guard let response: Response = try? RustCore.decode("set_language", values: code.map { ["code": $0] } ?? [:]) else { return }
        table = response.table
        current = response.language
        languages = response.languages
    }
}

func tr(_ german: String) -> String { Localization.table[german] ?? german }

func tr(_ german: String, _ values: [String: Any]) -> String {
    var text = tr(german)
    for (name, value) in values { text = text.replacingOccurrences(of: "{\(name)}", with: "\(value)") }
    return text
}

struct CoreError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

enum Operation: String, Codable, CaseIterable, Identifiable {
    case cut = "Cut", engrave = "Engrave", engrave3d = "Engrave3d", mark = "Mark"
    var id: String { rawValue }
    var title: String {
        switch self { case .cut: return tr("Schneiden"); case .engrave: return tr("Gravieren"); case .engrave3d: return tr("3D-Gravur"); case .mark: return tr("Markieren") }
    }
    var symbol: String {
        switch self { case .cut: return "scissors"; case .engrave: return "square.stack.3d.up"; case .engrave3d: return "cube"; case .mark: return "pencil.tip" }
    }
    var color: Color {
        switch self { case .cut: return .red; case .engrave: return .blue; case .engrave3d: return .orange; case .mark: return .purple }
    }
    var isRaster: Bool { self == .engrave || self == .engrave3d }
}

struct Project: Codable, Equatable {
    var format_version: Int
    var name: String
    var svg: String
    var bed_width_mm: Double
    var bed_height_mm: Double
    var x_mm: Double
    var y_mm: Double
    var width_mm: Double
    var height_mm: Double
    var material: String
    var thickness_mm: Double
    var operation: Operation
    var power_percent: Double
    var speed_percent: Double
    var passes: Int
    var hostname: String
    var port: Int
    var steps: [JobStep]
    var rotary_axis: Bool
    var rotary_diameter_mm: Double
    var raster: RasterSettings
    var ignore_filters: [[Filter]]
    /// Rust serialisiert das Feld immer und setzt beim Laden älterer Dateien den Standard.
    var cut_order: CutOrder
    var remove_duplicate_lines: Bool
    /// Vom aktiven Lasercutter übernommen (Zeilenversatz, Kurvenbeschleunigung, Verweilzeit).
    var machine: MachineSettings
    /// Name des aktiven Teils. Die Felder oben (SVG, Lage, Größe, Verfahren,
    /// Parameter, Schritte, Raster, Ignorierregeln) gehören immer zum aktiven Teil.
    var part_name: String = ""
    /// Index des aktiven Teils in der Liste aller Teile.
    var active_part: Int = 0
    /// Die übrigen Teile in Listenreihenfolge, ohne das aktive.
    var parts: [ProjectPart] = []

    var hasArtwork: Bool { !svg.isEmpty }
    var fitsBed: Bool {
        func fits(_ x: Double, _ y: Double, _ width: Double, _ height: Double) -> Bool {
            x.isFinite && y.isFinite && width.isFinite && height.isFinite &&
            x >= 0 && y >= 0 && width > 0 && height > 0 &&
            x + width <= bed_width_mm + 0.001 && y + height <= bed_height_mm + 0.001
        }
        return fits(x_mm, y_mm, width_mm, height_mm) &&
            parts.allSatisfy { fits($0.x_mm, $0.y_mm, $0.width_mm, $0.height_mm) }
    }
    /// Namen aller Teile in Listenreihenfolge (leer ohne Motiv).
    var partNames: [String] {
        guard hasArtwork else { return [] }
        var names = parts.map(\.name)
        names.insert(part_name, at: min(max(0, active_part), names.count))
        return names
    }
    /// Die nicht aktiven Teile mit ihrem Index in der Liste aller Teile.
    var inactiveParts: [(index: Int, part: ProjectPart)] {
        parts.enumerated().map { entry -> (index: Int, part: ProjectPart) in
            (index: entry.offset < active_part ? entry.offset : entry.offset + 1, part: entry.element)
        }
    }
}

/// Ein weiteres Teil (eine importierte Datei) mit eigener Lage und
/// Zuordnung (visicut_core::project::Part).
struct ProjectPart: Codable, Equatable {
    var name: String
    var svg: String
    var x_mm: Double
    var y_mm: Double
    var width_mm: Double
    var height_mm: Double
    var operation: Operation
    var power_percent: Double
    var speed_percent: Double
    var passes: Int
    var steps: [JobStep]
    var raster: RasterSettings
    var ignore_filters: [[Filter]]
}

/// Antwort von `part_previews`: Vorschaubilder aller Teile.
struct PartPreviewList: Decodable {
    struct Entry: Decodable {
        let index: Int
        let name: String
        let x_mm: Double
        let y_mm: Double
        let width_mm: Double
        let height_mm: Double
        let active: Bool
        let preview: Preview
    }
    let parts: [Entry]
}

/// Vorschaubild eines Teils zum Zeichnen auf dem Arbeitsbett.
struct PartImage {
    let index: Int
    let name: String
    /// Lage und Größe in Bett-mm.
    let rect: CGRect
    let active: Bool
    let image: NSImage?
}

/// Zuletzt geöffnete oder gesicherte Dateien (neueste zuerst), wie unter Windows/Linux.
enum RecentFiles {
    private static let key = "recentFiles"
    private static let limit = 10
    /// Der UI-Test verändert die Liste der Benutzerin nicht.
    private static var enabled: Bool { !CommandLine.arguments.contains("--ui-test") }
    static var paths: [String] { UserDefaults.standard.stringArray(forKey: key) ?? [] }
    static func add(_ url: URL) {
        guard enabled else { return }
        let path = url.path
        var list = paths.filter { $0 != path }
        list.insert(path, at: 0)
        UserDefaults.standard.set(Array(list.prefix(limit)), forKey: key)
    }
    static func remove(_ path: String) {
        guard enabled else { return }
        UserDefaults.standard.set(paths.filter { $0 != path }, forKey: key)
    }
    static func clear() {
        guard enabled else { return }
        UserDefaults.standard.removeObject(forKey: key)
    }
}

/// Zwischenspeicher für ungesicherte Projekte (Rust `session`): Statt nach dem
/// Sichern zu fragen, wird der Stand abgelegt und unter „Zuletzt geöffnet“
/// geführt. Der UI-Test legt nichts im Einstellungsordner ab.
enum SessionCache {
    static var enabled: Bool { !CommandLine.arguments.contains("--ui-test") }
    /// Anzahl der abgelegten Stände.
    static var count: Int { ((try? RustCore.decode("cached_states") as PathsResponse)?.paths.count) ?? 0 }
}

/// Reihenfolge der Vektorpfade (visicut_core::project::CutOrder).
enum CutOrder: String, Codable, CaseIterable, Identifiable {
    // Standard zuerst (Reihenfolge im Menü Job).
    case shortestTravel = "ShortestTravel", visiCut = "VisiCut"
    // VisiCuts weitere Reihenfolgen (im Menü unter einem Trennstrich).
    case file = "File", nearest = "Nearest", smallestFirst = "SmallestFirst"
    var id: String { rawValue }
    var title: String {
        switch self {
        case .visiCut: return tr("Klassisch")
        case .shortestTravel: return tr("Kürzeste Leerfahrten")
        case .file: return tr("Wie in der Datei")
        case .nearest: return tr("Nächster Pfad")
        case .smallestFirst: return tr("Kleinste zuerst")
        }
    }
    var help: String {
        switch self {
        case .visiCut: return tr("Innere Konturen zuerst, Reihenfolge wie VisiCut.")
        case .shortestTravel: return tr("Innere Konturen zuerst, dann jeweils der nächstgelegene Pfad ab dem Referenzpunkt oben rechts.")
        case .file: return tr("Pfade in der Reihenfolge der Zeichnung, ohne Sortierung (VisiCut „Datei“).")
        case .nearest: return tr("Ab dem ersten Pfad jeweils der nächstgelegene, ohne Rücksicht auf innere Konturen (VisiCut „Nächster“); Teile können vor ihren Löchern herausfallen.")
        case .smallestFirst: return tr("Kleinste Pfade zuerst, gemessen an der Fläche des Umrissrechtecks (VisiCut „Kleinste zuerst“).")
        }
    }
}

struct Material: Codable, Identifiable, Equatable {
    var id: String
    var name: String
    var profiles: [MaterialProfile]
    var thicknesses: [Double] { Array(Set(profiles.map(\.thickness_mm))).sorted() }
}
struct MaterialProfile: Codable, Equatable {
    var thickness_mm: Double
    var operation: Operation
    var power_percent: Double
    var speed_percent: Double
    var source: String?
}
struct SVGObject: Decodable, Identifiable { let id: Int; let label: String }
struct JobStep: Codable, Equatable {
    var operation: Operation
    var objects: [Int]
    var power_percent: Double
    var speed_percent: Double
    var passes: Int
    var filters: [Filter]?
    var rest: Bool
    var raster: RasterSettings
    var additional: [ParameterSet]
    /// Zusätzlicher Zeilenversatz in Pixeln (nur im Zeilenversatz-Testmuster).
    var engrave_shift_px: Int? = nil
    /// Schneiden/Markieren: nur der Rand der gemeinsamen Fläche (VisiCut „Kontur“).
    var outline: Bool? = nil
}
struct PreparedStep: Decodable {
    let name: String
    let operation: Operation
    let description: String
    let estimated_seconds: Double
    let power_percent: Double
    let speed_percent: Double
    let passes: Int
    let parameter_sets: Int
}
struct ProjectResponse: Decodable { let project: Project; let preview: Preview?; let objects: [SVGObject]?; let warnings: [String]?; let cached: Bool? }
struct PathResponse: Decodable { let path: String? }
struct OutlineResponse: Decodable { let contours: [[[Double]]] }
struct OutlinesResponse: Decodable { let outlines: [[[[Double]]]] }

/// Objekt unter einem Punkt (mm relativ zur Motivecke), wie Rust `object_at`:
/// zuerst eine nahe Linie (oberstes Objekt zuerst), sonst die kleinste
/// geschlossene Form, die den Punkt enthält.
func objectAt(_ outlines: [[[CGPoint]]], _ point: CGPoint, tolerance: Double) -> Int? {
    func near(_ a: CGPoint, _ b: CGPoint) -> Bool {
        let dx = b.x - a.x, dy = b.y - a.y, length = dx * dx + dy * dy
        let t = length > 0 ? min(1, max(0, ((point.x - a.x) * dx + (point.y - a.y) * dy) / length)) : 0
        return hypot(a.x + t * dx - point.x, a.y + t * dy - point.y) <= tolerance
    }
    for index in outlines.indices.reversed() {
        for contour in outlines[index] where contour.count > 1 {
            for i in 1..<contour.count where near(contour[i - 1], contour[i]) { return index }
        }
    }
    var best: (area: Double, index: Int)?
    for (index, outline) in outlines.enumerated() {
        for contour in outline where contour.count > 3 && contour.first == contour.last {
            var inside = false, area = 0.0
            for i in 1..<contour.count {
                let a = contour[i - 1], b = contour[i]
                if (a.y > point.y) != (b.y > point.y) && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x { inside.toggle() }
                area += a.x * b.y - b.x * a.y
            }
            if inside && (best == nil || abs(area) / 2 < best!.area) { best = (abs(area) / 2, index) }
        }
    }
    return best?.index
}
struct PathsResponse: Decodable { let paths: [String] }
struct WarningsResponse: Decodable { let warnings: [String] }
struct Preview: Decodable { let png: [UInt8] }
struct OutputJob: Decodable {
    let name: String
    let operation: Operation
    let bytes: [UInt8]
}
struct TransmissionResponse: Decodable { let sent: [String] }
struct PreparedJob: Decodable {
    let jobs: [OutputJob]
    var byteCount: Int { jobs.reduce(0) { $0 + $1.bytes.count } }
    let description: String
    let estimated_seconds: Double
    let preview_png: [UInt8]
    let steps: [PreparedStep]
    let timeline: LaserTimeline
    let warnings: [String]
}

func duration(_ seconds: Double) -> String {
    let total = max(1, Int(ceil(seconds)))
    if total >= 3600 { return "\(total / 3600) h \((total % 3600) / 60) min" }
    if total >= 60 { return "\(total / 60) min \(total % 60) s" }
    return "\(total) s"
}

@MainActor
final class AppModel: ObservableObject {
    @Published var project: Project { didSet {
        if !replacing { dirty = true }
        if oldValue.svg != project.svg || oldValue.steps != project.steps || oldValue.ignore_filters != project.ignore_filters {
            scheduleMappingRefresh()
        }
        if oldValue.parts != project.parts || oldValue.active_part != project.active_part {
            refreshPartImages()
        }
        if oldValue.svg != project.svg || oldValue.width_mm != project.width_mm || oldValue.height_mm != project.height_mm {
            outlineCache = [:]
            objectOutlines = nil
            if hoveredObject != nil { hoverObject(nil) }
        }
        // Farbige Bearbeitung auf der Arbeitsfläche: nur bei Inhalt oder
        // Zuordnung neu; reines Verschieben eines Teils verschiebt das Bild.
        if oldValue.svg != project.svg || oldValue.steps != project.steps || oldValue.ignore_filters != project.ignore_filters
            || oldValue.operation != project.operation || oldValue.parts != project.parts || oldValue.active_part != project.active_part
            || oldValue.width_mm != project.width_mm || oldValue.height_mm != project.height_mm
            || (!project.parts.isEmpty && (oldValue.x_mm != project.x_mm || oldValue.y_mm != project.y_mm)) {
            scheduleProcessingRefresh()
        }
        if preparedJob != nil { status = tr("Auftrag geändert. Vorschau und Zeit neu berechnen.") }
        preparedJob = nil
        jobImage = nil
        showJobPreview = false
        playback.seek(0, duration: 0)
    } }
    let playback = JobPlayback()
    @Published var objects: [SVGObject] = []
    @Published var preparedJob: PreparedJob?
    @Published var jobImage: NSImage?
    /// Objekte in den Farben ihrer Bearbeitung (Rust `processing_preview`).
    @Published var processingImage: NSImage?
    /// Alle Verfahren in einen LTT-Auftrag (eine Datei) statt einen je
    /// Verfahren; App-Einstellung, wird in der Auftragsvorschau umgeschaltet.
    @Published var singleJob = UserDefaults.standard.bool(forKey: "singleJob") {
        didSet {
            guard singleJob != oldValue else { return }
            if SessionCache.enabled { UserDefaults.standard.set(singleJob, forKey: "singleJob") }
            // Offene Vorschau neu berechnen; die bisherige bleibt bis dahin stehen.
            if preparedJob != nil { previewJob(refresh: true) }
        }
    }
    /// Umriss des Objekts unter der Maus in der Objektliste, in mm relativ zur
    /// Motivecke (Rust `object_outline`); Umrisse werden je Objekt gemerkt.
    @Published var hoverOutline: [[CGPoint]]?
    @Published private(set) var hoveredObject: Int?
    /// Die Maus steht auf der Arbeitsfläche (die Liste scrollt dann mit).
    private(set) var hoverFromCanvas = false
    /// Umrisse aller Objekte für den Treffertest auf der Arbeitsfläche.
    private(set) var objectOutlines: [[[CGPoint]]]?
    private var outlinesLoading = false
    private var outlineCache: [Int: [[CGPoint]]] = [:]
    private var processingGeneration = 0
    @Published var showJobPreview = false
    @Published var showProcessing = true
    @Published var image: NSImage?
    @Published var dirty = false
    @Published var busy = false
    @Published var status = tr("SVG importieren, um einen Job vorzubereiten.")
    @Published var error: String?
    @Published var keepProportions = true
    /// Bezugspunkt für X/Y und Größenänderungen (0, 0,5 oder 1 je Achse).
    @Published var referenceX = 0.0
    @Published var referenceY = 0.0
    /// Stand vor der letzten Werkzeug- oder Löschaktion (ein Schritt).
    @Published var undoState: UndoState?
    weak var undoManager: UndoManager?
    @Published var zoom = 1.0
    /// Verschiebung des Arbeitsbetts in Punkten (Trackpad-Scrollen beim Zoomen).
    @Published var pan = CGSize.zero
    func zoomBy(_ factor: Double) { zoom = min(8, max(0.5, zoom * factor)); if zoom <= 1 { pan = .zero } }
    func fitZoom() { zoom = 1; pan = .zero }
    @Published var showGrid = true
    /// Vorschaubilder aller Teile; nur bei mehreren Teilen gefüllt (sonst genügt `image`).
    @Published var partImages: [PartImage] = []
    private var partImageGeneration = 0
    @Published var devices: DeviceStore { didSet {
        // Nullpunkt unten links: Bezugspunkt standardmäßig unten links.
        let wasBottomLeft = oldValue.devices.indices.contains(oldValue.selected) && oldValue.devices[oldValue.selected].origin_bottom_left
        if wasBottomLeft != originBottomLeft { referenceY = originBottomLeft ? 1 : 0 }
    } }
    @Published var showCamera = false
    @Published var cameraImage: NSImage?
    @Published var cameraLoading = false
    /// Laufende Kamera-Aktualisierung: Fehler in Folge und Zähler gegen doppelte Schleifen.
    var cameraErrors = 0
    var cameraGeneration = 0
    let labs: [LabSetting]
    @Published var materials: [Material]
    @Published var materialsCustom = false
    @Published var mapping: MappingInfo?
    @Published var mappingError: String?
    @Published var vectorizeSource: BitmapSource?
    @Published var composeTool: ComposeTool?
    var materialSource = ""
    var mappingScheduled = false
    /// Ort des Projekts; ein neuer Ort (auch nil) beendet die Bindung an einen Zwischenstand.
    var projectURL: URL? { didSet { cachedURL = nil } }
    /// Zwischenstand, aus dem das Projekt geöffnet wurde; Sichern löscht ihn.
    private(set) var cachedURL: URL?
    private var replacing = true

    init() {
        do {
            let initial: ProjectResponse = try RustCore.decode("default")
            let list: DeviceList = try RustCore.decode("devices")
            devices = CameraKeychain.filled(list.store)
            labs = list.labs
            project = initial.project
            let catalog: MaterialCatalog = try RustCore.decode("materials")
            materials = catalog.materials
            materialSource = catalog.source
            materialsCustom = catalog.custom
            project.material = ""
            error = list.error.map { tr("Geräteliste nicht lesbar, FAU-Standard wird verwendet: {error}", ["error": $0]) }
                ?? catalog.error.map { tr("Materialbibliothek nicht lesbar, FAU-Bibliothek wird verwendet: {error}", ["error": $0]) }
        } catch {
            fatalError("Rust-Kern konnte nicht initialisiert werden: \(error)")
        }
        applyDevice()
        if originBottomLeft { referenceY = 1 }
        replacing = false
        if CommandLine.arguments.contains("--demo") || CommandLine.arguments.contains("--ui-test") {
            demo()
        } else if let path = CommandLine.arguments.dropFirst().first, !path.hasPrefix("-") {
            open(URL(fileURLWithPath: path))
        } else if SessionCache.enabled, let resume = try? RustCore.decode("take_resume") as PathResponse,
                  let path = resume.path {
            open(URL(fileURLWithPath: path))
        }
    }

    /// Materialprofile des Lasercutters.
    var activeMaterials: [Material] { materials }
    var selectedMaterial: Material? { activeMaterials.first { $0.name == project.material } }
    var materialSelection: String {
        project.material.isEmpty ? "" : selectedMaterial?.name ?? "__custom"
    }
    var preset: MaterialProfile? {
        selectedMaterial?.profiles.first { abs($0.thickness_mm - project.thickness_mm) < 0.0001 && $0.operation == project.operation && $0.power_percent > 0 && $0.speed_percent > 0 }
    }
    var sendEnabled: Bool { project.hasArtwork && project.fitsBed && !busy }

    func selectMaterial(_ selection: String) {
        if selection == "__custom" {
            project.material = tr("Eigenes Material")
        } else {
            project.material = selection
            if let material = selectedMaterial, !material.thicknesses.contains(project.thickness_mm),
               let first = material.thicknesses.first {
                project.thickness_mm = first
            }
            // Das Materialprofil ist der Standard.
            applyPreset()
        }
    }
    func selectThickness(_ thickness: Double) {
        project.thickness_mm = thickness
        applyPreset()
    }
    /// Materialprofil für ein Verfahren bei der aktuellen Stärke.
    func stepPreset(_ operation: Operation) -> MaterialProfile? {
        selectedMaterial?.profiles.first {
            abs($0.thickness_mm - project.thickness_mm) < 0.0001 && $0.operation == operation && $0.power_percent > 0 && $0.speed_percent > 0
        }
    }

    func applyPreset() {
        if !project.steps.isEmpty {
            for index in project.steps.indices { applyStepPreset(index) }
            status = tr("Verfügbare FAU-Profile für alle Verfahren übernommen.")
            return
        }
        guard let preset else { return }
        project.power_percent = preset.power_percent
        project.speed_percent = preset.speed_percent
        project.passes = 1
        status = tr("FAU-Profil für {material}, {thickness} mm, {operation} übernommen.",
                    ["material": project.material, "thickness": format(project.thickness_mm), "operation": project.operation.title])
    }

    func changeWidth(_ width: Double) {
        guard width.isFinite, width > 0 else { return }
        let anchor = referencePoint
        if keepProportions { project.height_mm *= width / project.width_mm }
        project.width_mm = width
        keepReference(anchor)
    }
    func changeHeight(_ height: Double) {
        guard height.isFinite, height > 0 else { return }
        let anchor = referencePoint
        if keepProportions { project.width_mm *= height / project.height_mm }
        project.height_mm = height
        keepReference(anchor)
    }
    /// Lage des Bezugspunkts auf dem Arbeitsbett.
    var referencePoint: (Double, Double) {
        (project.x_mm + project.width_mm * referenceX, project.y_mm + project.height_mm * referenceY)
    }
    /// Größenänderungen lassen den Bezugspunkt stehen.
    private func keepReference(_ anchor: (Double, Double)) {
        project.x_mm = anchor.0 - project.width_mm * referenceX
        project.y_mm = anchor.1 - project.height_mm * referenceY
    }
    func center() {
        project.x_mm = max(0, (project.bed_width_mm - project.width_mm) / 2)
        project.y_mm = max(0, (project.bed_height_mm - project.height_mm) / 2)
    }
    /// Der Lasercutter misst Y vom unteren Bettrand aus (nur Anzeige; der Auftrag bleibt gleich).
    var originBottomLeft: Bool { device.origin_bottom_left }
    /// Y des Bezugspunkts wie angezeigt: bei Nullpunkt unten links vom unteren Bettrand aus.
    var displayY: Double {
        let y = referencePoint.1
        return originBottomLeft ? project.bed_height_mm - y : y
    }
    func setDisplayY(_ value: Double) {
        guard value.isFinite else { return }
        let y = originBottomLeft ? project.bed_height_mm - value : value
        project.y_mm = y - project.height_mm * referenceY
    }

    // MARK: Teile

    /// Lädt die Vorschaubilder der Teile im Hintergrund (nur bei mehreren Teilen).
    /// Hebt das Objekt unter der Maus in der Objektliste auf der Arbeitsfläche hervor.
    func hoverObject(_ index: Int?, fromCanvas: Bool = false) {
        hoverFromCanvas = fromCanvas && index != nil
        if hoveredObject != index { hoveredObject = index }
        guard let index else { hoverOutline = nil; return }
        if let all = objectOutlines, all.indices.contains(index) { hoverOutline = all[index]; return }
        if let cached = outlineCache[index] { hoverOutline = cached; return }
        let snapshot = project
        DispatchQueue.global(qos: .userInitiated).async {
            let response = try? RustCore.decode("object_outline", project: snapshot, values: ["index": index]) as OutlineResponse
            DispatchQueue.main.async {
                let outline = response?.contours.map { $0.map { CGPoint(x: $0[0], y: $0[1]) } } ?? []
                // Nur für den Stand, aus dem der Umriss stammt.
                guard snapshot.svg == self.project.svg, snapshot.width_mm == self.project.width_mm,
                      snapshot.height_mm == self.project.height_mm else { return }
                self.outlineCache[index] = outline
                if self.hoveredObject == index { self.hoverOutline = outline }
            }
        }
    }

    /// Holt die Umrisse aller Objekte einmal im Hintergrund (für den Treffertest).
    func ensureObjectOutlines() {
        guard objectOutlines == nil, !outlinesLoading, project.hasArtwork else { return }
        outlinesLoading = true
        let snapshot = project
        DispatchQueue.global(qos: .userInitiated).async {
            let response = try? RustCore.decode("object_outlines", project: snapshot) as OutlinesResponse
            DispatchQueue.main.async {
                self.outlinesLoading = false
                guard snapshot.svg == self.project.svg, snapshot.width_mm == self.project.width_mm,
                      snapshot.height_mm == self.project.height_mm else { return }
                self.objectOutlines = response?.outlines.map { $0.map { $0.map { CGPoint(x: $0[0], y: $0[1]) } } } ?? []
            }
        }
    }

    /// Rechnet die farbige Bearbeitung kurz nach der letzten Änderung im
    /// Hintergrund neu; ältere Ergebnisse werden verworfen.
    func scheduleProcessingRefresh() {
        processingGeneration += 1
        let generation = processingGeneration
        guard project.hasArtwork else { processingImage = nil; return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
            guard generation == self.processingGeneration else { return }
            let snapshot = self.project
            DispatchQueue.global(qos: .userInitiated).async {
                let preview = try? RustCore.decode("processing_preview", project: snapshot) as Preview
                DispatchQueue.main.async {
                    guard generation == self.processingGeneration else { return }
                    self.processingImage = preview.flatMap { NSImage(data: Data($0.png)) }
                }
            }
        }
    }

    func refreshPartImages() {
        partImageGeneration += 1
        let generation = partImageGeneration
        guard project.hasArtwork, !project.parts.isEmpty else {
            if !partImages.isEmpty { partImages = [] }
            return
        }
        let snapshot = project
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try RustCore.decode("part_previews", project: snapshot) as PartPreviewList }
            DispatchQueue.main.async {
                guard generation == self.partImageGeneration, case .success(let list) = result else { return }
                self.partImages = list.parts.map { entry in
                    PartImage(index: entry.index, name: entry.name,
                              rect: CGRect(x: entry.x_mm, y: entry.y_mm, width: entry.width_mm, height: entry.height_mm),
                              active: entry.active, image: NSImage(data: Data(entry.preview.png)))
                }
            }
        }
    }

    /// Anzeigename eines Teils (Dateiname oder „Teil 2“).
    func partTitle(_ index: Int) -> String {
        let names = project.partNames
        guard names.indices.contains(index), !names[index].isEmpty else { return tr("Teil {number}", ["number": index + 1]) }
        return names[index]
    }

    /// Macht ein anderes Teil aktiv. Kein Widerrufschritt: Die Auswahl ändert keinen Inhalt.
    func selectPart(_ index: Int) {
        guard !busy, project.hasArtwork, index != project.active_part, index >= 0, index <= project.parts.count else { return }
        do {
            let response: ProjectResponse = try RustCore.decode("select_part", project: project, values: ["index": index])
            accept(response, dirty: dirty)
            refreshMapping()
            status = tr("Teil „{name}“ gewählt", ["name": partTitle(project.active_part)])
            if let warnings = response.warnings, !warnings.isEmpty { error = warnings.joined(separator: "\n") }
        } catch { self.error = error.localizedDescription }
    }

    func duplicatePart() {
        guard project.hasArtwork, !busy else { return }
        if compose("duplicate_part", values: [:]) { status = tr("Teil dupliziert · ⌘Z widerruft") }
    }

    static let graphicExtensions = ["svg", "psvg", "dxf", "eps", "ps", "pdf", "png", "jpg", "jpeg", "bmp", "gif", "nc", "gcode", "plf", "ls"]

    /// Weitere Grafiken als eigene Teile hinzufügen (wie Ablegen auf dem Fenster).
    func chooseAdditionalFiles() {
        guard !busy else { return }
        let panel = NSOpenPanel()
        panel.title = tr("Grafiken als Teile hinzufügen")
        panel.allowedContentTypes = Self.graphicExtensions.compactMap { UTType(filenameExtension: $0) }
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK, !panel.urls.isEmpty else { return }
        dropFiles(panel.urls)
    }

    /// Öffnet eine Datei aus „Zuletzt geöffnet“; fehlende Dateien verschwinden aus der Liste.
    func openRecent(_ path: String) {
        guard !busy else { return }
        guard FileManager.default.fileExists(atPath: path) else {
            RecentFiles.remove(path)
            error = tr("Datei nicht gefunden: {path}", ["path": path])
            return
        }
        open(URL(fileURLWithPath: path))
    }

    func accept(_ response: ProjectResponse, dirty: Bool) {
        replacing = true
        project = response.project
        if project.material == "Material wählen" { project.material = "" }
        applyDevice()
        image = response.preview.flatMap { NSImage(data: Data($0.png)) }
        objects = response.objects ?? []
        replacing = false
        self.dirty = dirty
    }

    func demo() {
        guard canDiscard() else { return }
        do {
            let response: ProjectResponse = try RustCore.decode("demo", project: project)
            accept(response, dirty: true)
            projectURL = nil
            status = tr("Beispiel geöffnet. Material und Bearbeitung auswählen.")
        } catch { self.error = error.localizedDescription }
    }

    func newProject() {
        guard canDiscard() else { return }
        do {
            let response: ProjectResponse = try RustCore.decode("default")
            accept(response, dirty: false)
            projectURL = nil
            status = tr("Neues Projekt. Datei öffnen, um zu beginnen.")
        } catch { self.error = error.localizedDescription }
    }

    func chooseFile() {
        guard !busy else { return }
        let panel = NSOpenPanel()
        panel.title = tr("Grafik oder VisiCutRust-Projekt öffnen")
        panel.allowedContentTypes = (Self.graphicExtensions + ["vcr"]).compactMap { UTType(filenameExtension: $0) } + [.json]
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url { open(url) }
    }

    func open(_ url: URL) {
        guard !busy, canDiscard() else { return }
        do {
            let isSVG = !["vcr", "json"].contains(url.pathExtension.lowercased())
            let response: ProjectResponse = try RustCore.decode(isSVG ? "import" : "load", project: project, path: url.path)
            let cached = response.cached == true
            accept(response, dirty: isSVG || cached)
            projectURL = isSVG || cached ? nil : url
            if cached { cachedURL = url }
            RecentFiles.add(url)
            status = isSVG ? tr("{file} importiert · Originalmaße übernommen", ["file": url.lastPathComponent])
                : cached ? tr("Ungesicherter Stand geöffnet. Zum Behalten sichern.") : tr("Projekt geöffnet")
            if let warnings = response.warnings, !warnings.isEmpty { self.error = warnings.joined(separator: "\n") }
        } catch { self.error = error.localizedDescription }
    }

    @discardableResult
    func save(asCopy: Bool = false) -> Bool {
        do {
            _ = try RustCore.call("validate_document", project: project)
            var url = asCopy ? nil : projectURL
            if url == nil {
                let panel = NSSavePanel()
                panel.title = tr("VisiCutRust-Projekt sichern")
                panel.allowedContentTypes = [UTType(filenameExtension: "vcr") ?? .json]
                panel.nameFieldStringValue = project.name + ".vcr"
                guard panel.runModal() == .OK else { return false }
                url = panel.url
            }
            guard let url else { return false }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try encoder.encode(project).write(to: url, options: .atomic)
            if let cache = cachedURL {
                _ = try? RustCore.call("discard_cached", path: cache.path)
                RecentFiles.remove(cache.path)
            }
            projectURL = url
            RecentFiles.add(url)
            dirty = false
            status = tr("Projekt gespeichert.")
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    /// Schreibt das Projekt als PLF für Java-VisiCut; die Hinweise nennen,
    /// was das Format nicht enthält.
    func exportPlf() {
        guard project.hasArtwork, !busy else { return }
        let panel = NSSavePanel()
        panel.title = tr("Als VisiCut-Projekt (PLF) exportieren")
        panel.allowedContentTypes = [UTType(filenameExtension: "plf") ?? .zip]
        panel.nameFieldStringValue = project.name + ".plf"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let result: WarningsResponse = try RustCore.decode("export_plf", project: project, path: url.path)
            status = tr("Als VisiCut-Projekt exportiert")
            if !result.warnings.isEmpty { error = result.warnings.joined(separator: "\n") }
        } catch { self.error = error.localizedDescription }
    }

    /// Löscht alle Zwischenstände und ihre Einträge unter „Zuletzt geöffnet“.
    /// Das geöffnete Projekt bleibt im Speicher, verliert aber die Bindung.
    func clearCache() {
        do {
            let removed: PathsResponse = try RustCore.decode("clear_cache")
            for path in removed.paths { RecentFiles.remove(path) }
            cachedURL = nil
            status = tr("Zwischenspeicher geleert ({count} Stände)", ["count": removed.paths.count])
        } catch {
            self.error = tr("Zwischenspeicher nicht geleert: {error}", ["error": error.localizedDescription])
        }
    }

    /// Keine Rückfrage: Ungesicherte Änderungen kommen in den Zwischenspeicher
    /// und unter „Zuletzt geöffnet“; mit `resume` öffnen sie beim nächsten Start
    /// wieder. Nur wenn das Ablegen scheitert, wird wie früher gefragt.
    func canDiscard(resume: Bool = false) -> Bool {
        guard !busy else { return false }
        guard dirty else { return true }
        guard SessionCache.enabled else { return true }
        do {
            var values: [String: Any] = ["resume": resume]
            if let cachedURL { values["previous"] = cachedURL.path }
            let stashed: PathResponse = try RustCore.decode("stash", project: project, values: values)
            if let path = stashed.path { RecentFiles.add(URL(fileURLWithPath: path)) }
            return true
        } catch {
            self.error = tr("Zwischenspeichern fehlgeschlagen: {error}", ["error": error.localizedDescription])
        }
        let alert = NSAlert()
        alert.messageText = tr("Änderungen an „{name}“ sichern?", ["name": project.name])
        alert.informativeText = tr("Nicht gesicherte Änderungen gehen verloren.")
        alert.addButton(withTitle: tr("Sichern"))
        alert.addButton(withTitle: tr("Abbrechen"))
        alert.addButton(withTitle: tr("Nicht sichern"))
        switch alert.runModal() {
        case .alertFirstButtonReturn: return save()
        case .alertThirdButtonReturn: return true
        default: return false
        }
    }

    /// Ohne gewähltes Material fehlen die Laserparameter: das sagen statt einer Leistungsgrenze.
    private func requireMaterial() -> Bool {
        guard project.material.trimmingCharacters(in: .whitespaces).isEmpty else { return true }
        error = tr("Kein Material zum Lasern gewählt: unter „Material“ ein Material wählen (oder „Eigenes Material …“ mit eigener Leistung).")
        return false
    }

    func export() {
        guard sendEnabled, requireMaterial() else { return }
        do {
            let prepared: PreparedJob = try preparedJob ?? RustCore.decode("prepare", project: project, values: ["single_job": singleJob])
            guard let first = prepared.jobs.first else { return }
            if prepared.jobs.count == 1 {
                let panel = NSSavePanel()
                panel.title = tr("LTT-Auftrag exportieren")
                panel.allowedContentTypes = [UTType(filenameExtension: "ltt") ?? .data]
                panel.nameFieldStringValue = first.name + ".ltt"
                guard panel.runModal() == .OK, let url = panel.url else { return }
                try Data(first.bytes).write(to: url, options: .atomic)
            } else {
                let panel = NSOpenPanel()
                panel.title = tr("Ordner für {count} getrennte LTT-Aufträge wählen", ["count": prepared.jobs.count])
                panel.canChooseDirectories = true
                panel.canChooseFiles = false
                panel.canCreateDirectories = true
                panel.prompt = tr("Exportieren")
                guard panel.runModal() == .OK, let folder = panel.url else { return }
                let existing = prepared.jobs.filter {
                    FileManager.default.fileExists(atPath: folder.appendingPathComponent($0.name + ".ltt").path)
                }
                if !existing.isEmpty {
                    let alert = NSAlert()
                    alert.messageText = tr("Vorhandene LTT-Dateien ersetzen?")
                    alert.informativeText = existing.map { $0.name + ".ltt" }.joined(separator: "\n")
                    alert.addButton(withTitle: tr("Ersetzen")); alert.addButton(withTitle: tr("Abbrechen"))
                    guard alert.runModal() == .alertFirstButtonReturn else { return }
                }
                for job in prepared.jobs {
                    try Data(job.bytes).write(to: folder.appendingPathComponent(job.name + ".ltt"), options: .atomic)
                }
            }
            status = tr("Exportiert: {files}", ["files": prepared.jobs.map(\.name).joined(separator: ", ")])
        } catch { self.error = error.localizedDescription }
    }

    func selectOperation(_ operation: Operation) {
        guard operation != project.operation else { return }
        project.operation = operation
        if preset != nil {
            applyPreset()
        } else if operation == .mark {
            project.power_percent = 0
            project.speed_percent = 100
            project.passes = 1
            status = tr("Markieren: eigene Leistung einstellen oder ein passendes Profil übernehmen.")
        }
    }

    func defaultStep(_ operation: Operation) -> JobStep {
        let preset = selectedMaterial?.profiles.first {
            abs($0.thickness_mm - project.thickness_mm) < 0.0001 && $0.operation == operation && $0.power_percent > 0
        }
        var step = JobStep(operation: operation, objects: [],
            power_percent: preset?.power_percent ?? (operation == .mark && project.operation != .mark ? 0 : project.power_percent),
            speed_percent: preset?.speed_percent ?? (operation == .mark && project.operation != .mark ? 100 : project.speed_percent),
            passes: project.passes)
        if operation.isRaster && project.operation.isRaster { step.raster = project.raster }
        return step
    }

    func setIndividual(_ enabled: Bool) {
        if !enabled { project.steps = []; return }
        project.steps = [Operation.engrave, .mark, .cut].map { operation in
            var step = defaultStep(operation)
            step.objects = operation == project.operation ? objects.map(\.id) : []
            return step
        }
    }

    func assignment(_ object: Int) -> String {
        project.steps.first { $0.objects.contains(object) }?.operation.rawValue ?? "Ignore"
    }

    func assign(_ object: Int, to operation: String) {
        var steps = project.steps
        if let target = Operation(rawValue: operation), !steps.contains(where: { $0.operation == target }) {
            steps.append(defaultStep(target))
        }
        for index in steps.indices {
            steps[index].objects.removeAll { $0 == object }
            if steps[index].operation.rawValue == operation { steps[index].objects.append(object) }
        }
        project.steps = steps
    }

    func applyStepPreset(_ index: Int) {
        guard let preset = selectedMaterial?.profiles.first(where: {
            abs($0.thickness_mm - project.thickness_mm) < 0.0001 && $0.operation == project.steps[index].operation && $0.power_percent > 0
        }) else { return }
        project.steps[index].power_percent = preset.power_percent
        project.steps[index].speed_percent = preset.speed_percent
        project.steps[index].passes = 1
    }

    /// Berechnet Auftragsvorschau und Zeit; mit `refresh` neu, wobei die
    /// bisherige Vorschau bis zum Ergebnis sichtbar bleibt (kein Neuaufbau).
    func previewJob(refresh: Bool = false) {
        guard sendEnabled, requireMaterial() else { return }
        if preparedJob != nil && !refresh { showJobPreview = true; return }
        busy = true
        status = tr("Auftragsvorschau und Bearbeitungszeit werden berechnet …")
        let snapshot = project
        let single = singleJob
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try RustCore.decode("prepare", project: snapshot, values: ["single_job": single]) as PreparedJob }
            DispatchQueue.main.async {
                self.busy = false
                guard self.project == snapshot else {
                    self.status = tr("Auftrag geändert. Vorschau und Zeit neu berechnen.")
                    return
                }
                // Inzwischen erneut umgeschaltet: das spätere Ergebnis zählt.
                guard self.singleJob == single else { return }
                switch result {
                case .success(let prepared):
                    self.playback.seek(0, duration: prepared.estimated_seconds)
                    self.preparedJob = prepared
                    self.jobImage = NSImage(data: Data(prepared.preview_png))
                    self.showJobPreview = true
                    self.status = tr("Geschätzte Bearbeitungszeit: ca. {time}", ["time": duration(prepared.estimated_seconds)])
                case .failure(let error):
                    self.error = error.localizedDescription
                    self.status = tr("Auftrag konnte nicht vorbereitet werden.")
                }
            }
        }
    }

    func send() { previewJob() }

    func sendReviewedJob() {
        guard sendEnabled, let prepared = preparedJob else { return }
        let snapshot = project
        let alert = NSAlert()
        let count: [String: Any] = ["count": prepared.jobs.count]
        alert.messageText = prepared.jobs.count == 1
            ? tr("{count} Auftrag an den Lasercutter senden?", count)
            : tr("{count} Aufträge an den Lasercutter senden?", count)
        let jobs = tr("Aufträge: {jobs} · ca. {time}",
                      ["jobs": prepared.jobs.map(\.name).joined(separator: " → "), "time": duration(prepared.estimated_seconds)])
        let target = tr("Lasercutter: {name} ({host}:{port})",
                        ["name": device.name, "host": snapshot.hostname, "port": String(snapshot.port)])
        alert.informativeText = ([jobs + "\n" + target] + prepared.warnings).joined(separator: "\n\n")
        alert.addButton(withTitle: tr("Senden"))
        alert.addButton(withTitle: tr("Abbrechen"))
        guard alert.runModal() == .alertFirstButtonReturn, snapshot == project else { return }
        showJobPreview = false
        busy = true
        status = tr("Job wird an den Lasercutter übertragen …")
        let single = singleJob
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try RustCore.decode("transmit", project: snapshot, values: ["single_job": single]) as TransmissionResponse }
            DispatchQueue.main.async {
                self.busy = false
                switch result {
                case .success(let response):
                    self.status = tr("Übertragen: {jobs}. Aufträge am Gerät prüfen und einzeln starten.", ["jobs": response.sent.joined(separator: ", ")])
                    self.showJobSentText(response.sent)
                case .failure(let error): self.error = error.localizedDescription; self.status = tr("Übertragung fehlgeschlagen. Gerätestatus prüfen.")
                }
            }
        }
    }

}

func format(_ value: Double) -> String {
    value.formatted(.number.precision(.fractionLength(0...2)))
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, NSToolbarDelegate, NSMenuItemValidation, NSToolbarItemValidation, NSMenuDelegate {
    var model: AppModel!
    var workspaceWindow: NSWindow?
    var settingsWindow: NSWindow?
    var materialsWindow: NSWindow?
    /// Untermenü „Zuletzt geöffnet“ (wird bei jedem Aufklappen neu gefüllt).
    private var recentMenu: NSMenu?
    /// Untermenü „Ablage → Lasercutter“ (wird bei jedem Aufklappen neu gefüllt).
    private var deviceMenu: NSMenu?
    private var changes: AnyCancellable?
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        // Vor dem Modell und den Menüs, damit Startmeldungen und Titel übersetzt sind.
        Localization.load()
        model = AppModel()
        installMenus()
        showWorkspace()
        changes = model.objectWillChange.sink { [weak self] in
            DispatchQueue.main.async { self?.refreshWindow() }
        }
        NSApp.activate(ignoringOtherApps: true)
        if CommandLine.arguments.contains("--ui-test") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { runUITest(self.model) }
            DispatchQueue.main.asyncAfter(deadline: .now() + 30) {
                fputs("Native UI test timed out: \(NSApp.windows.count) windows\n", stderr)
                exit(1)
            }
        }
    }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let model else { return .terminateNow }
        return model.canDiscard(resume: true) ? .terminateNow : .terminateCancel
    }
    func application(_ application: NSApplication, open urls: [URL]) {
        if let url = urls.first { model?.open(url) }
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showWorkspace(); return true
    }

    func showWorkspace() {
        if let window = workspaceWindow { window.makeKeyAndOrderFront(nil); return }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1240, height: 820),
            // Mit .fullSizeContentView: Die geteilte Ansicht zeichnet ihren Symbolleisten-Bereich
            // sonst zusätzlich in den Inhalt, der Kopf der Arbeitsfläche wirkt verblasst und angeschnitten.
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        let hosting = NSHostingView(rootView: Workspace(model: model))
        // Sonst ersetzt SwiftUI die Symbolleiste des Fensters durch eine eigene,
        // die nur den Seitenleisten-Knopf enthält; der Titel kommt aus refreshWindow().
        hosting.sceneBridgingOptions = []
        window.contentView = hosting
        // Widerrufen von Werkzeug- und Löschaktionen über Bearbeiten → Widerrufen.
        model.undoManager = window.undoManager
        window.minSize = NSSize(width: 960, height: 700)
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.toolbarStyle = .unified
        let toolbar = NSToolbar(identifier: "VisiCutToolbar")
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = true
        window.toolbar = toolbar
        workspaceWindow = window
        window.center()
        window.makeKeyAndOrderFront(nil)
        refreshWindow()
    }

    func refreshWindow() {
        workspaceWindow?.title = model.project.name + " — VisiCutRust"
        workspaceWindow?.isDocumentEdited = model.dirty
        workspaceWindow?.representedURL = model.projectURL
        workspaceWindow?.toolbar?.validateVisibleItems()
    }

    func windowWillClose(_ notification: Notification) {
        if let window = notification.object as? NSWindow, window === workspaceWindow { workspaceWindow = nil }
    }

    private func menuItem(_ title: String, action: Selector?, key: String = "", modifiers: NSEvent.ModifierFlags = .command, target: AnyObject? = nil) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        item.target = target ?? self
        return item
    }
    private func installMenus() {
        let bar = NSMenu()
        func submenu(_ title: String) -> NSMenu {
            let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            let menu = NSMenu(title: title); item.submenu = menu; bar.addItem(item); return menu
        }
        let app = submenu("VisiCutRust")
        app.addItem(menuItem(tr("Über VisiCutRust"), action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), target: NSApp))
        app.addItem(.separator())
        app.addItem(menuItem(tr("Einstellungen …"), action: #selector(showSettings), key: ","))
        app.addItem(.separator())
        let services = NSMenuItem(title: tr("Dienste"), action: nil, keyEquivalent: "")
        services.submenu = NSMenu(title: tr("Dienste")); app.addItem(services); NSApp.servicesMenu = services.submenu
        app.addItem(.separator())
        app.addItem(menuItem(tr("VisiCutRust ausblenden"), action: #selector(NSApplication.hide(_:)), key: "h", target: NSApp))
        app.addItem(menuItem(tr("Andere ausblenden"), action: #selector(NSApplication.hideOtherApplications(_:)), key: "h", modifiers: [.command, .option], target: NSApp))
        app.addItem(menuItem(tr("Alle einblenden"), action: #selector(NSApplication.unhideAllApplications(_:)), target: NSApp))
        app.addItem(.separator())
        app.addItem(menuItem(tr("VisiCutRust beenden"), action: #selector(NSApplication.terminate(_:)), key: "q", target: NSApp))
        let file = submenu(tr("Ablage"))
        file.addItem(menuItem(tr("Neues Projekt"), action: #selector(newProject), key: "n"))
        file.addItem(menuItem(tr("Öffnen …"), action: #selector(openFile), key: "o"))
        // Zuletzt geöffnete Dateien; die Einträge entstehen beim Aufklappen (menuNeedsUpdate).
        let recent = NSMenu(title: tr("Zuletzt geöffnet"))
        recent.delegate = self
        fillRecentMenu(recent)
        recentMenu = recent
        let recentItem = NSMenuItem(title: tr("Zuletzt geöffnet"), action: nil, keyEquivalent: "")
        recentItem.submenu = recent
        file.addItem(recentItem)
        file.addItem(menuItem(tr("Zwischenspeicher leeren …"), action: #selector(clearCache)))
        file.addItem(menuItem(tr("Hinzufügen …"), action: #selector(addFiles), key: "o", modifiers: [.command, .shift]))
        file.addItem(menuItem(tr("Beispiel öffnen"), action: #selector(openDemo)))
        file.addItem(menuItem(tr("Bitmap vektorisieren …"), action: #selector(vectorizeBitmap)))
        file.addItem(.separator())
        // Aktiver Lasercutter und Einstellungen statt einer Auswahl in der Seitenleiste.
        let devices = NSMenu(title: tr("Lasercutter"))
        devices.delegate = self
        deviceMenu = devices
        fillDeviceMenu(devices)
        let devicesItem = NSMenuItem(title: tr("Lasercutter"), action: nil, keyEquivalent: "")
        devicesItem.submenu = devices
        file.addItem(devicesItem)
        file.addItem(menuItem(tr("Einstellungen …"), action: #selector(showSettings)))
        file.addItem(.separator())
        file.addItem(menuItem(tr("Sichern"), action: #selector(saveProject), key: "s"))
        file.addItem(menuItem(tr("Sichern unter …"), action: #selector(saveAs), key: "s", modifiers: [.command, .shift]))
        file.addItem(menuItem(tr("LTT-Job exportieren …"), action: #selector(exportJob), key: "e", modifiers: [.command, .shift]))
        file.addItem(menuItem(tr("Als VisiCut-Projekt (PLF) exportieren …"), action: #selector(exportPlf)))
        file.addItem(.separator())
        let close = menuItem(tr("Fenster schließen"), action: #selector(NSWindow.performClose(_:)), key: "w"); close.target = nil; file.addItem(close)
        let edit = submenu(tr("Bearbeiten"))
        let editItems: [(String, String, String)] = [
            (tr("Widerrufen"), "undo:", "z"), (tr("Ausschneiden"), "cut:", "x"), (tr("Kopieren"), "copy:", "c"),
            (tr("Einsetzen"), "paste:", "v"), (tr("Alles auswählen"), "selectAll:", "a")]
        for (title, selector, key) in editItems {
            let item = menuItem(title, action: Selector(selector), key: key); item.target = nil; edit.addItem(item)
        }
        edit.addItem(.separator())
        edit.addItem(menuItem(tr("Duplizieren"), action: #selector(duplicateArtwork), key: "d"))
        edit.addItem(menuItem(tr("Teil duplizieren"), action: #selector(duplicatePart)))
        // Entfernt das aktive Teil (bei nur einem Teil das ganze Motiv).
        edit.addItem(menuItem(tr("Motiv löschen"), action: #selector(deleteArtwork)))
        let object = submenu(tr("Objekt"))
        object.addItem(menuItem(tr("Text …"), action: #selector(insertText)))
        object.addItem(menuItem(tr("Form …"), action: #selector(insertShape)))
        object.addItem(menuItem(tr("Schild …"), action: #selector(insertSign)))
        object.addItem(.separator())
        object.addItem(menuItem(tr("Kopien im Raster …"), action: #selector(arrayCopies)))
        object.addItem(menuItem(tr("Teile verschachteln (Nesting) …"), action: #selector(nestParts)))
        object.addItem(.separator())
        object.addItem(menuItem(tr("90° nach links drehen"), action: #selector(rotateLeft)))
        object.addItem(menuItem(tr("90° nach rechts drehen"), action: #selector(rotateRight)))
        object.addItem(menuItem(tr("Horizontal spiegeln"), action: #selector(flipHorizontal)))
        object.addItem(menuItem(tr("Vertikal spiegeln"), action: #selector(flipVertical)))
        let view = submenu(tr("Darstellung"))
        view.addItem(menuItem(tr("Zoom zurücksetzen"), action: #selector(fitView), key: "0"))
        view.addItem(menuItem(tr("Raster anzeigen"), action: #selector(toggleGrid)))
        view.addItem(.separator())
        view.addItem(menuItem(tr("Kamerabild anzeigen"), action: #selector(toggleCamera), key: "k"))
        view.addItem(menuItem(tr("Kamerabild aktualisieren"), action: #selector(refreshCamera), key: "k", modifiers: [.command, .shift]))
        view.addItem(.separator())
        // Sprache der Oberfläche: Systemsprache oder eine feste Sprache (Name in der jeweiligen Sprache).
        let languageMenu = NSMenu(title: tr("Sprache"))
        let systemLanguage = menuItem(tr("Systemsprache"), action: #selector(chooseLanguage(_:)))
        systemLanguage.state = Localization.chosen == nil ? .on : .off
        languageMenu.addItem(systemLanguage)
        languageMenu.addItem(.separator())
        for language in Localization.languages {
            let item = menuItem(language.name, action: #selector(chooseLanguage(_:)))
            item.representedObject = language.code
            item.state = Localization.chosen == language.code ? .on : .off
            languageMenu.addItem(item)
        }
        let languageItem = NSMenuItem(title: tr("Sprache"), action: nil, keyEquivalent: "")
        languageItem.submenu = languageMenu
        view.addItem(languageItem)
        let job = submenu(tr("Job"))
        job.addItem(menuItem(tr("FAU-Materialprofil übernehmen"), action: #selector(applyProfile)))
        job.addItem(menuItem(tr("Materialbibliothek …"), action: #selector(showMaterials), key: "m", modifiers: [.command, .shift]))
        job.addItem(menuItem(tr("Auftragsvorschau und Zeit …"), action: #selector(previewJob), key: "p", modifiers: [.command, .shift]))
        job.addItem(.separator())
        // Reihenfolge der Schnitt- und Markierpfade; das Häkchen setzt validateMenuItem.
        let order = NSMenu(title: tr("Schnittreihenfolge"))
        for cutOrder in CutOrder.allCases {
            if cutOrder == .file { order.addItem(.separator()) }
            let item = menuItem(cutOrder.title, action: #selector(chooseCutOrder(_:)))
            item.representedObject = cutOrder.rawValue
            item.toolTip = cutOrder.help
            order.addItem(item)
        }
        let orderItem = NSMenuItem(title: tr("Schnittreihenfolge"), action: nil, keyEquivalent: "")
        orderItem.submenu = order
        job.addItem(orderItem)
        job.addItem(.separator())
        job.addItem(menuItem(tr("An Lasercutter senden …"), action: #selector(sendJob)))
        let windows = submenu(tr("Fenster"))
        windows.addItem(menuItem(tr("Arbeitsbereich anzeigen"), action: #selector(showMainWindow)))
        let minimize = menuItem(tr("Im Dock ablegen"), action: #selector(NSWindow.performMiniaturize(_:)), key: "m"); minimize.target = nil; windows.addItem(minimize)
        NSApp.windowsMenu = windows
        NSApp.mainMenu = bar
    }

    /// Einträge von „Zuletzt geöffnet“: je Datei ihr Name (Pfad als Tooltip), danach „Liste leeren“.
    private func fillRecentMenu(_ menu: NSMenu) {
        menu.removeAllItems()
        let paths = RecentFiles.paths
        if paths.isEmpty {
            // Ohne Aktion ist der Eintrag deaktiviert.
            menu.addItem(NSMenuItem(title: tr("Keine Dateien"), action: nil, keyEquivalent: ""))
            return
        }
        for path in paths {
            let item = menuItem(URL(fileURLWithPath: path).lastPathComponent, action: #selector(openRecent(_:)))
            item.toolTip = path
            item.representedObject = path
            menu.addItem(item)
        }
        menu.addItem(.separator())
        menu.addItem(menuItem(tr("Liste leeren"), action: #selector(clearRecent)))
    }

    /// Eingerichtete Lasercutter; der aktive hat ein Häkchen.
    private func fillDeviceMenu(_ menu: NSMenu) {
        menu.removeAllItems()
        for (index, device) in model.devices.devices.enumerated() {
            let item = menuItem(device.name, action: #selector(chooseDevice(_:)))
            item.representedObject = index
            item.state = index == model.devices.selected ? .on : .off
            item.toolTip = "\(device.hostname):\(device.port)"
            menu.addItem(item)
        }
    }
    @objc func chooseDevice(_ sender: NSMenuItem) {
        guard let index = sender.representedObject as? Int else { return }
        model.selectDevice(index)
        refreshWindow()
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        if menu === deviceMenu { fillDeviceMenu(menu) }
        if menu === recentMenu { fillRecentMenu(menu) }
    }

    @objc func openRecent(_ sender: NSMenuItem) {
        guard let path = sender.representedObject as? String else { return }
        showWorkspace()
        // Erst nach dem Schließen des Menüs öffnen (Zwischenspeichern, Liste wird neu sortiert).
        DispatchQueue.main.async { self.model.openRecent(path) }
    }
    @objc func chooseCutOrder(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let order = CutOrder(rawValue: raw),
              model.project.cut_order != order else { return }
        model.project.cut_order = order
    }
    @objc func clearRecent() {
        RecentFiles.clear()
        if let recentMenu { fillRecentMenu(recentMenu) }
    }
    /// Endgültiges Löschen, daher mit Bestätigung.
    @objc func clearCache() {
        let alert = NSAlert()
        alert.messageText = tr("{count} ungesicherte Stände löschen?", ["count": SessionCache.count])
        alert.informativeText = tr("Die abgelegten Projekte im Zwischenspeicher werden endgültig gelöscht. Gesicherte Projekte und das geöffnete Projekt bleiben erhalten.")
        alert.addButton(withTitle: tr("Löschen")).hasDestructiveAction = true
        alert.addButton(withTitle: tr("Abbrechen"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        model.clearCache()
        if let recentMenu { fillRecentMenu(recentMenu) }
    }

    /// Wechselt die Sprache und baut Menüs, Symbolleiste, Fenstertitel und
    /// SwiftUI-Ansichten neu auf (die Auswahl bleibt in UserDefaults gespeichert).
    @objc func chooseLanguage(_ sender: NSMenuItem) {
        let code = sender.representedObject as? String
        // Erst nach dem Schließen des Menüs ersetzen, das die Aktion ausgelöst hat.
        DispatchQueue.main.async {
            Localization.choose(code)
            self.installMenus()
            self.refreshLocalizedTexts()
        }
    }

    private func refreshLocalizedTexts() {
        for item in workspaceWindow?.toolbar?.items ?? [] {
            guard let spec = toolbarSpec(item.itemIdentifier) else { continue }
            item.label = spec.0; item.paletteLabel = spec.0; item.toolTip = spec.0
            item.image = NSImage(systemSymbolName: spec.1, accessibilityDescription: spec.0)
        }
        settingsWindow?.title = tr("Einstellungen")
        materialsWindow?.title = tr("Materialbibliothek")
        model.objectWillChange.send()
        refreshWindow()
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        switch item.action {
        case #selector(exportPlf): return model.project.hasArtwork && !model.busy
        case #selector(exportJob), #selector(sendJob), #selector(previewJob): return model.sendEnabled
        case #selector(applyProfile): return !model.busy && (model.preset != nil || model.project.steps.contains { step in
            model.selectedMaterial?.profiles.contains {
                abs($0.thickness_mm - model.project.thickness_mm) < 0.0001 && $0.operation == step.operation && $0.power_percent > 0
            } ?? false
        })
        case #selector(centerArtwork): return model.project.hasArtwork && !model.busy
        case #selector(toggleGrid): item.state = model.showGrid ? .on : .off; return true
        case #selector(toggleCamera): item.state = model.showCamera ? .on : .off; return !model.device.camera_url.isEmpty
        case #selector(refreshCamera): return model.showCamera && !model.cameraLoading
        case #selector(openFile), #selector(openDemo), #selector(newProject), #selector(showSettings), #selector(openRecent(_:)),
             #selector(chooseDevice(_:)): return !model.busy
        case #selector(chooseCutOrder(_:)):
            item.state = (item.representedObject as? String) == model.project.cut_order.rawValue ? .on : .off
            return !model.busy
        case #selector(clearRecent): return !RecentFiles.paths.isEmpty
        case #selector(clearCache): return !model.busy && SessionCache.enabled && SessionCache.count > 0
        case #selector(addFiles): return !model.busy && !model.showJobPreview && model.composeTool == nil
        case #selector(duplicatePart):
            return model.project.hasArtwork && !model.busy && !model.showJobPreview && model.composeTool == nil
        case #selector(vectorizeBitmap): return !model.busy && !model.showJobPreview && model.vectorizeSource == nil
        case #selector(insertText), #selector(insertShape), #selector(insertSign):
            return !model.busy && !model.showJobPreview && model.composeTool == nil
        case #selector(arrayCopies), #selector(nestParts), #selector(duplicateArtwork), #selector(deleteArtwork),
             #selector(rotateLeft), #selector(rotateRight), #selector(flipHorizontal), #selector(flipVertical):
            return model.project.hasArtwork && !model.busy && !model.showJobPreview && model.composeTool == nil
        default: return true
        }
    }
    func validateToolbarItem(_ item: NSToolbarItem) -> Bool {
        if ["send", "export", "preview"].contains(item.itemIdentifier.rawValue) { return model.sendEnabled }
        return !model.busy
    }
    @objc func newProject() { showWorkspace(); model.newProject() }
    @objc func openFile() { showWorkspace(); model.chooseFile() }
    @objc func openDemo() { showWorkspace(); model.demo() }
    @objc func vectorizeBitmap() { showWorkspace(); model.chooseBitmap() }
    @objc func insertText() { showWorkspace(); model.composeTool = .text }
    @objc func insertShape() { showWorkspace(); model.composeTool = .shape }
    @objc func insertSign() { showWorkspace(); model.composeTool = .sign }
    @objc func arrayCopies() { showWorkspace(); model.composeTool = .array }
    @objc func nestParts() { showWorkspace(); model.composeTool = .nest }
    @objc func duplicateArtwork() { model.duplicateArtwork() }
    @objc func duplicatePart() { model.duplicatePart() }
    @objc func addFiles() { showWorkspace(); model.chooseAdditionalFiles() }
    @objc func deleteArtwork() { model.deleteArtwork() }
    @objc func rotateLeft() { model.transformArtwork(rotate: -90) }
    @objc func rotateRight() { model.transformArtwork(rotate: 90) }
    @objc func flipHorizontal() { model.transformArtwork(flipHorizontal: true) }
    @objc func flipVertical() { model.transformArtwork(flipVertical: true) }
    @objc func saveProject() { model.save() }
    @objc func saveAs() { model.save(asCopy: true) }
    @objc func exportJob() { model.export() }
    @objc func exportPlf() { model.exportPlf() }
    @objc func sendJob() { model.send() }
    @objc func previewJob() { model.previewJob() }
    @objc func applyProfile() { model.applyPreset() }
    @objc func centerArtwork() { model.center() }
    @objc func fitView() { model.fitZoom() }
    @objc func toggleGrid() { model.showGrid.toggle() }
    @objc func toggleCamera() { model.toggleCamera() }
    @objc func refreshCamera() { model.refreshCamera() }
    @objc func showMainWindow() { showWorkspace() }
    @objc func showMaterials() {
        if let window = materialsWindow { window.makeKeyAndOrderFront(nil); return }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 860, height: 560), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: MaterialEditor(model: model))
        window.title = tr("Materialbibliothek")
        window.isReleasedWhenClosed = false
        materialsWindow = window
        window.center(); window.makeKeyAndOrderFront(nil)
    }
    @objc func showSettings() {
        if let window = settingsWindow { window.makeKeyAndOrderFront(nil); return }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 820, height: 560), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: DeviceSettings(model: model))
        window.title = tr("Einstellungen")
        window.isReleasedWhenClosed = false
        settingsWindow = window
        window.center(); window.makeKeyAndOrderFront(nil)
    }
    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { toolbarDefaultItemIdentifiers(toolbar) + [.space] }
    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { [.toggleSidebar, .init("open"), .init("save"), .flexibleSpace, .init("preview"), .init("export"), .init("send")] }
    /// Beschriftung, Symbol und Aktion eines Symbolleistenelements.
    private func toolbarSpec(_ identifier: NSToolbarItem.Identifier) -> (String, String, Selector)? {
        switch identifier.rawValue {
        case "open": return (tr("Öffnen"), "folder", #selector(openFile))
        case "save": return (tr("Sichern"), "square.and.arrow.down", #selector(saveProject))
        case "preview": return (tr("Vorschau & Zeit"), "eye", #selector(previewJob))
        case "export": return (tr("Exportieren"), "square.and.arrow.up", #selector(exportJob))
        case "send": return (tr("An Lasercutter senden"), "paperplane", #selector(sendJob))
        default: return nil
        }
    }
    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier identifier: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        guard let spec = toolbarSpec(identifier) else { return nil }
        let item = NSToolbarItem(itemIdentifier: identifier)
        item.label = spec.0; item.paletteLabel = spec.0; item.toolTip = spec.0
        item.image = NSImage(systemSymbolName: spec.1, accessibilityDescription: spec.0)
        item.target = self; item.action = spec.2
        item.isEnabled = ["send", "export", "preview"].contains(identifier.rawValue) ? model.sendEnabled : !model.busy
        return item
    }
}

@main
enum VisiCutRustApp {
    @MainActor static func main() {
        if CommandLine.arguments.contains("--ui-test") {
            // Never read or change the user's device list during self-tests.
            let settings = FileManager.default.temporaryDirectory.appendingPathComponent("visicut-ui-test-\(UUID().uuidString)")
            setenv("VISICUT_RUST_CONFIG_DIR", settings.path, 1)
        }
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        app.run()
        withExtendedLifetime(delegate) {}
    }
}

struct Workspace: View {
    @ObservedObject var model: AppModel
    private var bedSummary: String {
        let summary = "\(model.device.name) · 1000 × 600 mm"
        return model.project.rotary_axis ? summary + " · " + tr("Rotationseinheit") : summary
    }

    var body: some View {
        NavigationSplitView {
            Inspector(model: model)
                .navigationSplitViewColumnWidth(min: 320, ideal: 340, max: 400)
        } detail: {
            VStack(spacing: 0) {
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        if model.project.hasArtwork {
                            EditableTitle(text: $model.project.name)
                        } else {
                            Text(tr("Arbeitsbereich")).font(.title2).fontWeight(.semibold)
                        }
                        Text(bedSummary).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if !model.device.camera_url.isEmpty {
                        Toggle(isOn: Binding(get: { model.showCamera }, set: { _ in model.toggleCamera() })) {
                            Image(systemName: "camera")
                        }.toggleStyle(.button).help(tr("Kamerabild anzeigen"))
                        if model.showCamera {
                            if model.cameraLoading { ProgressView().controlSize(.small) }
                            else { Button { model.refreshCamera() } label: { Image(systemName: "arrow.clockwise") }.help(tr("Kamerabild aktualisieren")) }
                        }
                    }
                    Toggle(isOn: $model.showGrid) { Image(systemName: "grid") }
                        .toggleStyle(.button).help(tr("Raster anzeigen"))
                    Button { model.zoomBy(1 / 1.25) } label: { Image(systemName: "minus.magnifyingglass") }.help(tr("Verkleinern"))
                    Text("\(Int(model.zoom * 100)) %").monospacedDigit().frame(width: 48)
                    Button { model.zoomBy(1.25) } label: { Image(systemName: "plus.magnifyingglass") }.help(tr("Vergrößern"))
                    // 100 % zeigt das ganze Arbeitsbett.
                    Button(tr("Zurücksetzen")) { model.fitZoom() }
                        .help(tr("Zoom auf 100 % zurücksetzen (ganzes Arbeitsbett)"))
                }.buttonStyle(.borderless).padding(20)
                if model.project.hasArtwork && !model.project.fitsBed {
                    Label(tr("Das Motiv liegt außerhalb des Arbeitsbetts und kann so nicht gesendet werden."), systemImage: "exclamationmark.triangle.fill")
                        .font(.callout).foregroundStyle(.white)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 20).padding(.vertical, 8)
                        .background(Color.red)
                }
                Divider()
                HStack {
                    Button(tr("Vorschau & Zeit berechnen"), systemImage: "eye") { model.previewJob() }
                        .disabled(!model.sendEnabled)
                    if let prepared = model.preparedJob {
                        Text(tr("Ca. {time}", ["time": duration(prepared.estimated_seconds)])).monospacedDigit()
                        Spacer()
                        Toggle(tr("Bearbeitung anzeigen"), isOn: $model.showProcessing)
                        Text(tr("Rot: Schnitt · Blau: Gravur · Orange: 3D · Violett: Markieren")).font(.caption).foregroundStyle(.secondary)
                    } else {
                        Text(tr("Nach Änderungen neu berechnen")).font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        if model.processingImage != nil {
                            Toggle(tr("Bearbeitung anzeigen"), isOn: $model.showProcessing)
                            Text(tr("Rot: Schnitt · Blau: Gravur · Orange: 3D · Violett: Markieren")).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }.padding(.horizontal, 20).padding(.vertical, 10)
                Divider()
                ZStack {
                    Color(nsColor: .underPageBackgroundColor)
                    if model.project.hasArtwork {
                        BedCanvas(model: model).padding(24)
                    } else {
                        VStack(spacing: 16) {
                            Image(systemName: "square.and.arrow.down").font(.system(size: 44, weight: .light)).foregroundStyle(.secondary)
                            Text(tr("Mit einer SVG beginnen")).font(.title2).fontWeight(.semibold)
                            Text(tr("Importiere dein Motiv, wähle ein Material\nund bereite den Job für den Lasercutter vor."))
                                .foregroundStyle(.secondary).multilineTextAlignment(.center)
                            Button(tr("SVG importieren …")) { model.chooseFile() }.buttonStyle(.borderedProminent)
                            Button(tr("Beispiel öffnen")) { model.demo() }.buttonStyle(.link)
                            Button(tr("Bitmap vektorisieren …")) { model.chooseBitmap() }.buttonStyle(.link)
                        }.padding(40)
                    }
                }
                Divider()
                HStack(spacing: 8) {
                    if model.busy { ProgressView().controlSize(.small) }
                    else { Image(systemName: "info.circle").foregroundStyle(.secondary) }
                    Text(model.status).font(.callout).foregroundStyle(.secondary).lineLimit(2)
                    Spacer()
                }.padding(.horizontal, 16).padding(.vertical, 10)
            }
            .frame(minWidth: 560, minHeight: 640)
            .sheet(item: $model.vectorizeSource) { source in VectorizeSheet(model: model, source: source) }
            .sheet(item: $model.composeTool) { tool in ComposeSheet(model: model, tool: tool) }
        }
        .navigationTitle(model.project.name)
        .sheet(isPresented: $model.showJobPreview) { JobPreview(model: model) }
        .alert("VisiCutRust", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button(tr("OK"), role: .cancel) { model.error = nil }
        } message: { Text(model.error ?? "") }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            guard !model.busy, !providers.isEmpty else { return false }
            var urls = [URL?](repeating: nil, count: providers.count)
            let group = DispatchGroup()
            for (index, provider) in providers.enumerated() {
                group.enter()
                provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                    let url = (item as? Data).flatMap { URL(dataRepresentation: $0, relativeTo: nil) }
                    DispatchQueue.main.async {
                        urls[index] = url
                        group.leave()
                    }
                }
            }
            group.notify(queue: .main) { model.dropFiles(urls.compactMap { $0 }) }
            return true
        }
    }
}

private struct ObjectRowsHeight: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

struct Inspector: View {
    @ObservedObject var model: AppModel
    /// Gemessene Höhe aller Zeilen der Objektliste.
    @State private var objectRowsHeight: CGFloat = 0
    /// Bis zu so vielen Objekten wächst die Liste mit, darüber scrollt sie.
    private let visibleObjects = 5
    var material: Binding<String> { Binding(get: { model.materialSelection }, set: { model.selectMaterial($0) }) }

    /// Höhe passend zur Anzahl; ab mehr als `visibleObjects` Zeilen scrollbar.
    private var objectListHeight: CGFloat {
        let count = model.objects.count
        guard count > 0, objectRowsHeight > 0 else { return CGFloat(min(count, visibleObjects)) * 32 + 8 }
        guard count > visibleObjects else { return objectRowsHeight }
        return 8 + (objectRowsHeight - 8) / CGFloat(count) * CGFloat(visibleObjects)
    }

    var body: some View {
        Form {
            if model.device.rotary_axis {
                Section(tr("Rotationseinheit")) {
                    Toggle(tr("Rotationseinheit verwenden"), isOn: $model.project.rotary_axis)
                    if model.project.rotary_axis {
                        NumberField(tr("Durchmesser"), value: $model.project.rotary_diameter_mm, unit: "mm")
                        Text(tr("Y entspricht dem Umfang ({circumference} mm). Gravur zentriert am Gerät ausrichten. Schritte je Umdrehung aus LibLaserCut, am Gerät nicht validiert.",
                                ["circumference": format(Double.pi * model.project.rotary_diameter_mm)]))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            Section(tr("Material")) {
                LabeledContent(tr("Material")) {
                    NativePopup(options: [PopupOption("", tr("Material wählen")), .separator]
                        + model.activeMaterials.map { PopupOption($0.name, $0.name) }
                        + [.separator, PopupOption("__custom", tr("Eigenes Material …"))],
                        selection: material, identifier: "materialPicker", maxWidth: 210)
                }
                if model.materialSelection == "__custom" {
                    TextField(tr("Bezeichnung"), text: $model.project.material)
                }
                // Nur Materialien mit echten Stärken zeigen die Auswahl; gibt es nur
                // „Nicht festgelegt“ (z. B. Folien, Beschichtungen), entfällt sie.
                if let material = model.selectedMaterial, material.thicknesses.contains(where: { $0 > 0 }) {
                    LabeledContent(tr("Stärke")) {
                        // Bis zu drei Stärken als eigene Knöpfe, mehr (Acryl …) im Menü.
                        if material.thicknesses.count <= 3 {
                            Picker(tr("Stärke"), selection: Binding(get: { model.project.thickness_mm },
                                                                    set: { model.selectThickness($0) })) {
                                ForEach(material.thicknesses, id: \.self) { thickness in
                                    Text(thickness == 0 ? tr("Nicht festgelegt") : "\(format(thickness)) mm").tag(thickness)
                                }
                            }
                            .pickerStyle(.segmented).labelsHidden().fixedSize()
                            .accessibilityIdentifier("thicknessButtons")
                        } else {
                            NativePopup(options: material.thicknesses.map {
                                PopupOption(String($0), $0 == 0 ? tr("Nicht festgelegt") : "\(format($0)) mm")
                            }, selection: Binding(get: { String(model.project.thickness_mm) }, set: {
                                if let value = Double($0) { model.selectThickness(value) }
                            }), identifier: "thicknessPicker")
                            .fixedSize()
                        }
                    }
                } else if model.selectedMaterial?.thicknesses.isEmpty ?? true {
                    NumberField(tr("Stärke"), value: $model.project.thickness_mm, unit: "mm")
                }
            }
            if model.project.hasArtwork {
                PartsSection(model: model)
            }
            Section(tr("Objektzuordnung")) {
                Picker(tr("Zuordnung"), selection: Binding(get: { model.assignmentMode }, set: { model.setAssignmentMode($0) })) {
                    ForEach(AssignmentMode.allCases) { Text($0.title).tag($0) }
                }.pickerStyle(.segmented).disabled(!model.project.hasArtwork)
                    .accessibilityIdentifier("assignmentMode")
                if model.assignmentMode == .whole {
                    Text(tr("Ein Verfahren für das gesamte Motiv. Für Zuordnungen nach Strichfarbe oder anderen Merkmalen „Regeln“ wählen.")).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                } else if model.assignmentMode == .rules {
                    Text(tr("Regeln ordnen Objekte nach Merkmalen zu, z. B. nach Strichfarbe: Rot (#ff0000) → Schneiden, Blau (#0000ff) → Gravieren, Grün (#00ff00) → Markieren.")
                         + " " + tr("Vorlage wählen oder Bedingungen (Farbe, Linienstärke, Ebene, Typ, ID) anpassen; „Rest“ erfasst alle übrigen Objekte.")).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                if model.assignmentMode == .objects {
                    ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 4) {
                            ForEach(model.objects) { object in
                                HStack {
                                    Text(object.label).lineLimit(2).font(.callout)
                                    Spacer(minLength: 6)
                                    AssignmentMenu(selection: Binding(get: { model.assignment(object.id) },
                                        set: { model.assign(object.id, to: $0) }))
                                }
                                .padding(.horizontal, 6).padding(.vertical, 3)
                                // Ausgewählt: Objekt unter der Maus (Liste oder Arbeitsfläche).
                                .background(RoundedRectangle(cornerRadius: 6)
                                    .fill(model.hoveredObject == object.id ? Color.accentColor.opacity(0.2) : Color.clear))
                                .id(object.id)
                                .contextMenu { Button(tr("Objekt löschen")) { model.deleteObject(object.id) } }
                                .contentShape(Rectangle())
                                // Objekt unter der Maus auf der Arbeitsfläche hervorheben.
                                .onHover { inside in
                                    if inside { model.hoverObject(object.id) }
                                    else if model.hoveredObject == object.id { model.hoverObject(nil) }
                                }

                            }
                        }.padding(.vertical, 4)
                        .background(GeometryReader { geometry in
                            Color.clear.preference(key: ObjectRowsHeight.self, value: geometry.size.height)
                        })
                    }
                    .onPreferenceChange(ObjectRowsHeight.self) { objectRowsHeight = $0 }
                    .frame(height: objectListHeight)
                    // Steht die Maus auf der Arbeitsfläche, die Zeile ins Bild holen.
                    .onChange(of: model.hoveredObject) { _, index in
                        if model.hoverFromCanvas, let index { withAnimation(.easeOut(duration: 0.15)) { proxy.scrollTo(index, anchor: .center) } }
                    }
                    }
                }
                Toggle(tr("Doppelte Linien entfernen"), isOn: $model.project.remove_duplicate_lines)
                    .help(tr("Linien, die genau auf einer bereits geschnittenen Linie desselben Schritts liegen (z. B. gemeinsame Kanten), nur einmal schneiden"))
            }
            if model.assignmentMode == .rules { RulesSection(model: model) }
            if model.project.steps.isEmpty {
            Section(tr("Bearbeitung · gesamtes Motiv")) {
                OperationSelector(selection: Binding(get: { model.project.operation }, set: { model.selectOperation($0) }))
                if model.project.operation == .mark && model.preset == nil {
                    Text(tr("Konturen markieren. Eigene Leistung einstellen; kein Schnittprofil übernehmen."))
                        .font(.caption).foregroundStyle(.secondary)
                }
                if model.preset == nil {
                    Text(model.project.material.isEmpty ? tr("Material wählen, um sein Profil zu übernehmen.") : tr("Kein Profil für diese Kombination; Parameter manuell eingeben."))
                        .font(.callout).foregroundStyle(.secondary)
                }
                NumberField(tr("Leistung"), value: $model.project.power_percent, unit: "%", profile: model.preset?.power_percent)
                NumberField(tr("Geschwindigkeit"), value: $model.project.speed_percent, unit: "%", profile: model.preset?.speed_percent)
                Stepper(value: $model.project.passes, in: 1...100) {
                    LabeledContent(tr("Durchgänge"), value: String(model.project.passes))
                }
                if model.project.operation.isRaster {
                    RasterOptions(settings: $model.project.raster, operation: model.project.operation)
                }
            }
            } else {
                ForEach(model.project.steps.indices, id: \.self) { index in
                    // In „Einzeln“ nur Verfahren, denen Objekte zugeordnet sind;
                    // die Einstellungen der übrigen bleiben erhalten.
                    if model.assignmentMode != .objects || model.objectCount(index) > 0 {
                        StepInspector(model: model, index: index)
                    }
                }
            }
            Section(tr("Position und Größe")) {
                PositionSizeGrid(model: model)
                if model.originBottomLeft {
                    Text(tr("Nullpunkt unten links")).font(.caption).foregroundStyle(.secondary)
                }
                TransformControls(model: model)
            }
        }
        .formStyle(.grouped)
        .disabled(model.busy)
        .navigationTitle(tr("Job vorbereiten"))
    }
}

/// Liste aller Teile (eine importierte Datei je Teil); Klick wählt das aktive Teil.
struct PartsSection: View {
    @ObservedObject var model: AppModel
    var body: some View {
        Section(tr("Teile")) {
            ForEach(model.project.partNames.indices, id: \.self) { index in
                let active = index == model.project.active_part
                Button { model.selectPart(index) } label: {
                    HStack(spacing: 8) {
                        Image(systemName: active ? "checkmark.circle.fill" : "circle")
                            .foregroundStyle(active ? Color.accentColor : Color.secondary)
                        Text(model.partTitle(index)).fontWeight(active ? .semibold : .regular).lineLimit(1)
                        Spacer(minLength: 0)
                    }.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(active ? .isSelected : [])
            }
            HStack {
                Button(tr("Datei hinzufügen …")) { model.chooseAdditionalFiles() }
                Spacer()
                Button { model.duplicatePart() } label: { Image(systemName: "plus.square.on.square") }
                    .help(tr("Teil duplizieren"))
                Button { model.deleteArtwork() } label: { Image(systemName: "trash") }
                    .help(tr("Teil entfernen"))
            }.buttonStyle(.borderless)
        }
    }
}

struct OperationSelector: View {
    @Binding var selection: Operation
    var body: some View {
        HStack(spacing: 5) {
            ForEach(Operation.allCases) { operation in
                Button { selection = operation } label: {
                    Text(operation.title).font(.caption).fontWeight(selection == operation ? .semibold : .regular)
                        .frame(maxWidth: .infinity).padding(.vertical, 8)
                        .foregroundStyle(operation.color)
                        .background(operation.color.opacity(selection == operation ? 0.18 : 0.04), in: RoundedRectangle(cornerRadius: 6))
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(operation.color.opacity(selection == operation ? 0.8 : 0.2), lineWidth: 1))
                }.buttonStyle(.plain).accessibilityAddTraits(selection == operation ? .isSelected : [])
            }
        }
    }
}

struct AssignmentMenu: View {
    @Binding var selection: String
    private var operation: Operation? { Operation(rawValue: selection) }
    var body: some View {
        Menu {
            ForEach(Operation.allCases) { operation in
                Button { selection = operation.rawValue } label: { Label(operation.title, systemImage: operation.symbol) }
            }
            Divider()
            Button(tr("Ignorieren"), systemImage: "eye.slash") { selection = "Ignore" }
        } label: {
            Label(operation?.title ?? tr("Ignorieren"), systemImage: operation?.symbol ?? "eye.slash")
                .font(.callout).foregroundStyle(operation?.color ?? .secondary)
                .padding(.horizontal, 8).padding(.vertical, 5)
                .background((operation?.color ?? .gray).opacity(0.12), in: RoundedRectangle(cornerRadius: 6))
        }.menuStyle(.borderlessButton).fixedSize().accessibilityLabel(tr("Bearbeitung zuweisen"))
    }
}

/// Farbige Bearbeitung im Schrittkopf mit Pfeil; ein Klick öffnet das Menü
/// der Bearbeitungen. Ein SwiftUI-`Menu` würde die Farbe verlieren.
struct OperationSwitcher: View {
    let operation: Operation
    let choose: (Operation) -> Void
    var body: some View {
        Button(action: show) {
            HStack(spacing: 4) {
                Label(operation.title, systemImage: operation.symbol).fontWeight(.semibold)
                Image(systemName: "chevron.down").font(.caption2.weight(.bold))
            }
            .foregroundStyle(operation.color)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(tr("Bearbeitung ändern"))
        .accessibilityLabel(tr("Bearbeitung ändern"))
        .accessibilityValue(operation.title)
    }
    private func show() {
        guard let window = NSApp.keyWindow, let view = window.contentView else { return }
        let handler = Handler(choose)
        let menu = NSMenu()
        for candidate in Operation.allCases {
            let item = NSMenuItem(title: candidate.title, action: #selector(Handler.chosen(_:)), keyEquivalent: "")
            item.target = handler
            item.representedObject = candidate.rawValue
            item.image = NSImage(systemSymbolName: candidate.symbol, accessibilityDescription: nil)
            item.state = candidate == operation ? .on : .off
            menu.addItem(item)
        }
        // popUp läuft modal; der Handler lebt bis zur Auswahl.
        menu.popUp(positioning: nil, at: view.convert(window.mouseLocationOutsideOfEventStream, from: nil), in: view)
        _ = handler
    }
    private final class Handler: NSObject {
        let choose: (Operation) -> Void
        init(_ choose: @escaping (Operation) -> Void) { self.choose = choose }
        @objc func chosen(_ sender: NSMenuItem) {
            if let raw = sender.representedObject as? String, let operation = Operation(rawValue: raw) { choose(operation) }
        }
    }
}

/// Projektname über der Arbeitsfläche; Doppelklick zum Umbenennen, ⏎ oder
/// Klick daneben übernimmt, Esc bricht ab.
struct EditableTitle: View {
    @Binding var text: String
    @State private var draft = ""
    @State private var editing = false
    @FocusState private var focused: Bool
    var body: some View {
        if editing {
            TextField(tr("Projektname"), text: $draft)
                .font(.title2).fontWeight(.semibold)
                .textFieldStyle(.plain)
                .focused($focused)
                .onSubmit(commit)
                .onExitCommand { editing = false }
                .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
                .onAppear { focused = true }
                .frame(maxWidth: 420, alignment: .leading)
        } else {
            Text(text).font(.title2).fontWeight(.semibold)
                .contentShape(Rectangle())
                .onTapGesture(count: 2) { draft = text; editing = true }
                .help(tr("Doppelklick zum Umbenennen"))
                .accessibilityAction(named: tr("Umbenennen")) { draft = text; editing = true }
        }
    }
    private func commit() {
        guard editing else { return }
        editing = false
        let name = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty && name != text { text = name }
    }
}

struct StepInspector: View {
    @ObservedObject var model: AppModel
    let index: Int
    /// Stand beim Erzeugen der Zeile; hält die Ansicht stabil, falls der
    /// Schritt beim Moduswechsel schon entfernt wurde.
    let snapshot: JobStep
    init(model: AppModel, index: Int) {
        self.model = model; self.index = index
        snapshot = model.project.steps.indices.contains(index) ? model.project.steps[index] : JobStep(operation: .cut, objects: [],
            power_percent: 0, speed_percent: 100, passes: 1, filters: nil, rest: false, raster: .fau, additional: [])
    }
    private var binding: Binding<JobStep> { $model.project.steps.safe(index, default: snapshot) }
    private var step: JobStep { binding.wrappedValue }
    var body: some View {
        Section {
            HStack {
                if step.isRule {
                    // Bearbeitung umschaltbar; Bedingungen bleiben erhalten.
                    OperationSwitcher(operation: step.operation) { model.changeRuleStep(index, to: $0) }
                } else {
                    Label(step.operation.title, systemImage: step.operation.symbol)
                        .fontWeight(.semibold).foregroundStyle(step.operation.color)
                }
                Spacer()
                if step.isRule {
                    Button { model.insertRuleStep(after: index) } label: { Image(systemName: "plus") }
                        .buttonStyle(.borderless).help(tr("Schritt mit gleicher Bearbeitung darunter einfügen"))
                    Button { if model.project.steps.indices.contains(index) { model.project.steps.remove(at: index) } } label: { Image(systemName: "trash") }
                        .buttonStyle(.borderless).help(tr("Schritt entfernen"))
                }
            }
            Text(tr("{count} Objekte", ["count": model.objectCount(index)])).foregroundStyle(.secondary)
            if step.isRule {
                Toggle(tr("Rest: alle übrigen Objekte"), isOn: Binding(get: { step.rest }, set: {
                    binding.wrappedValue.rest = $0
                    binding.wrappedValue.filters = $0 ? nil : []
                }))
                if !step.rest {
                    FilterSetEditor(filters: Binding(get: { binding.wrappedValue.filters ?? [] },
                                                     set: { binding.wrappedValue.filters = $0 }),
                                    mapping: model.mapping)
                }
            }
            let profile = model.stepPreset(step.operation)
            if step.operation == .mark && step.power_percent == 0 && profile == nil {
                Text(tr("Zum Markieren zuerst eine eigene Leistung einstellen.")).font(.caption).foregroundStyle(.secondary)
            }
            NumberField(tr("Leistung"), value: binding.power_percent, unit: "%", profile: profile?.power_percent)
            NumberField(tr("Geschwindigkeit"), value: binding.speed_percent, unit: "%", profile: profile?.speed_percent)
            Stepper(value: binding.passes, in: 1...100) {
                LabeledContent(tr("Durchgänge"), value: String(step.passes))
            }
            ParameterSets(sets: binding.additional,
                          first: ParameterSet(power_percent: step.power_percent, speed_percent: step.speed_percent, passes: step.passes))
            if step.operation.isRaster {
                RasterOptions(settings: binding.raster, operation: step.operation)
            } else {
                Toggle(tr("Nur Umriss"), isOn: Binding(get: { binding.wrappedValue.outline ?? false },
                                                      set: { binding.wrappedValue.outline = $0 ? true : nil }))
                    .help(tr("Schneidet nur den Rand der Fläche, die alle Objekte dieses Schritts zusammen bedecken (VisiCut „Kontur“): Überlappungen und innere Linien entfallen, Bilder zählen als Rechteck."))
            }
        }
    }
}

struct JobPreview: View {
    @ObservedObject var model: AppModel
    /// „Ca. 2 min 5 s · 1 Durchgänge · 2 Parametersätze“ für einen Schritt.
    private func stepSummary(_ step: PreparedStep) -> String {
        var summary = tr("Ca. {time}", ["time": duration(step.estimated_seconds)])
            + " · " + tr("{count} Durchgänge", ["count": step.passes])
        if step.parameter_sets > 1 { summary += " · " + tr("{count} Parametersätze", ["count": step.parameter_sets]) }
        return summary
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(tr("Auftragsvorschau")).font(.title2).bold()
                Spacer()
                Text(model.project.name).foregroundStyle(.secondary)
            }
            if let prepared = model.preparedJob {
                HStack(alignment: .top, spacing: 24) {
                    VStack {
                        JobSimulation(job: prepared, project: model.project, image: model.jobImage, playback: model.playback)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        Text(tr("Rot: Schneiden · Blau: Gravieren · Orange: 3D-Gravur · Violett: Markieren"))
                            .font(.caption).foregroundStyle(.secondary)
                        // Umschließendes Rechteck aller Teile (wie das Vorschaubild).
                        let bounds = model.project.partsBounds
                        let shownY: Double = model.originBottomLeft ? model.project.bed_height_mm - Double(bounds.maxY) : Double(bounds.minY)
                        Text("\(format(Double(bounds.width))) × \(format(Double(bounds.height))) mm · X \(format(Double(bounds.minX))) / Y \(format(shownY)) mm")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        Text(tr("Ca. {time}", ["time": duration(prepared.estimated_seconds)])).font(.title).monospacedDigit()
                        Text(tr("Geschätzte Bearbeitungszeit")).foregroundStyle(.secondary)
                        ForEach(prepared.warnings, id: \.self) { warning in
                            Label(warning, systemImage: "exclamationmark.triangle").font(.callout).foregroundStyle(.orange)
                        }
                        ForEach(prepared.steps.indices, id: \.self) { index in
                            let step = prepared.steps[index]
                            VStack(alignment: .leading, spacing: 5) {
                                Label("\(index + 1). \(step.operation.title)", systemImage: step.operation.symbol)
                                    .bold().foregroundStyle(step.operation.color)
                                Text(step.name).font(.caption).monospaced()
                                Text(stepSummary(step))
                                Text(tr("Leistung {power} % · Tempo {speed} %", ["power": format(step.power_percent), "speed": format(step.speed_percent)]))
                                Text(step.description).foregroundStyle(.secondary)
                            }.font(.callout)
                        }
                        Text(tr("Schätzung aus Fahrwegen, Tempo und Durchgängen; bei Gravur mit Zeilenrücklauf und Overscan. Beschleunigung und Geräteeinstellungen können die tatsächliche Dauer verändern."))
                            .font(.caption).foregroundStyle(.secondary)
                        Text("\(ByteCountFormatter.string(fromByteCount: Int64(prepared.byteCount), countStyle: .file)) · \(model.project.hostname):\(String(model.project.port))")
                            .font(.caption).foregroundStyle(.secondary)
                        Divider()
                        Toggle(tr("Alles in eine Datei"), isOn: $model.singleJob)
                            .help(tr("Alle Verfahren in einem LTT-Auftrag statt einem je Verfahren"))
                        Text(model.singleJob
                             ? tr("Ein Auftrag mit allen Verfahren: Gravuren zuerst, dann Markieren und Schneiden.")
                             : tr("{count} Einzelaufträge · Engrav → Eng3D → Mark → Cut; jeder wird am Gerät einzeln gestartet.", ["count": prepared.jobs.count]))
                            .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    }
                    }.frame(width: 270, alignment: .leading)
                }
            }
            Divider()
            HStack {
                Button(tr("Schließen")) { model.showJobPreview = false }.keyboardShortcut(.cancelAction)
                Spacer()
                Button(tr("LTT exportieren …")) { model.export() }.disabled(!model.sendEnabled)
                Button(tr("An Lasercutter senden …")) { model.sendReviewedJob() }
                    .buttonStyle(.borderedProminent).disabled(!model.sendEnabled || model.preparedJob == nil)
            }
        }.padding(24).frame(width: 940, height: 640)
            .background(Color(nsColor: .windowBackgroundColor))
    }
}

struct PopupOption: Equatable {
    let id: String
    let title: String
    let isSeparator: Bool
    init(_ id: String, _ title: String) { self.id = id; self.title = title; self.isSeparator = false }
    private init() { id = ""; title = ""; isSeparator = true }
    static var separator: Self { Self() }
}

struct NativePopup: NSViewRepresentable {
    let options: [PopupOption]
    @Binding var selection: String
    let identifier: String
    /// Höchstbreite; längere Titel werden in einer Zeile mit „…“ gekürzt.
    var maxWidth: CGFloat? = nil
    @Environment(\.isEnabled) private var isEnabled

    func makeCoordinator() -> Coordinator { Coordinator(selection: $selection) }
    func makeNSView(context: Context) -> NSPopUpButton {
        let button = NSPopUpButton(frame: .zero, pullsDown: false)
        // Sichtbarer Rahmen mit Doppelpfeil, damit das Menü als solches erkennbar ist.
        button.bezelStyle = .push
        button.isBordered = true
        button.controlSize = .regular
        button.lineBreakMode = .byTruncatingTail
        button.setAccessibilityIdentifier(identifier)
        button.target = context.coordinator
        button.action = #selector(Coordinator.changed(_:))
        return button
    }
    func updateNSView(_ button: NSPopUpButton, context: Context) {
        context.coordinator.selection = $selection
        if context.coordinator.options != options {
            button.removeAllItems()
            for option in options {
                if option.isSeparator { button.menu?.addItem(.separator()) }
                else {
                    button.addItem(withTitle: option.title)
                    button.lastItem?.representedObject = option.id
                }
            }
            context.coordinator.options = options
        }
        let index = button.itemArray.firstIndex { $0.representedObject as? String == selection }
        button.selectItem(at: index ?? -1)
        button.isEnabled = isEnabled
        button.toolTip = button.selectedItem?.title
    }
    /// Mit `maxWidth` nie breiter als erlaubt und angeboten: Lange Titel werden
    /// in einer Zeile mit „…“ gekürzt (der Tooltip zeigt den ganzen Titel),
    /// statt dass das Menü in eine zweite Zeile rutscht.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView button: NSPopUpButton, context: Context) -> CGSize? {
        let ideal = button.intrinsicContentSize
        guard let maxWidth else { return ideal }
        // NSPopUpButton ist so breit wie sein längster Eintrag; hier zählt der
        // gewählte, damit kurze Titel wie die anderen Zeilen rechts stehen.
        let font = button.font ?? .systemFont(ofSize: NSFont.systemFontSize)
        let titleWidth = { (title: String) in ceil((title as NSString).size(withAttributes: [.font: font]).width) }
        let widest = button.itemArray.map { titleWidth($0.title) }.max() ?? 0
        let selected = titleWidth(button.titleOfSelectedItem ?? "")
        var width = min(ideal.width - widest + selected, maxWidth)
        if let offered = proposal.width, offered.isFinite { width = min(width, max(offered, 80)) }
        return CGSize(width: width, height: ideal.height)
    }
    final class Coordinator: NSObject {
        var selection: Binding<String>
        var options: [PopupOption] = []
        init(selection: Binding<String>) { self.selection = selection }
        @objc func changed(_ sender: NSPopUpButton) {
            if let value = sender.selectedItem?.representedObject as? String { selection.wrappedValue = value }
        }
    }
}

/// Wie das Transformieren-Feld in Grafikprogrammen: Bezugspunkt, X/Y, B/H und
/// rechts die Kette, die das Seitenverhältnis hält.
struct PositionSizeGrid: View {
    @ObservedObject var model: AppModel
    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            ReferencePointPicker(x: $model.referenceX, y: $model.referenceY)
            Grid(alignment: .leading, horizontalSpacing: 4, verticalSpacing: 6) {
                GridRow {
                    Text("X").help(tr("Position X des Bezugspunkts"))
                    UnitField(title: "X", value: Binding(get: { model.referencePoint.0 },
                        set: { model.project.x_mm = $0 - model.project.width_mm * model.referenceX }), unit: "mm")
                    Text("B").help(tr("Breite")).padding(.leading, 6)
                    UnitField(title: tr("Breite"), value: Binding(get: { model.project.width_mm }, set: { model.changeWidth($0) }), unit: "mm")
                }
                GridRow {
                    // Bei Nullpunkt unten links vom unteren Bettrand aus gemessen.
                    Text("Y").help(tr("Position Y des Bezugspunkts"))
                    UnitField(title: "Y", value: Binding(get: { model.displayY }, set: { model.setDisplayY($0) }), unit: "mm")
                    Text("H").help(tr("Höhe")).padding(.leading, 6)
                    UnitField(title: tr("Höhe"), value: Binding(get: { model.project.height_mm }, set: { model.changeHeight($0) }), unit: "mm")
                }
            }
            Button { model.keepProportions.toggle() } label: {
                Image(systemName: "link")
                    .foregroundStyle(model.keepProportions ? Color.accentColor : Color.secondary)
                    // Gelöst: durchgestrichene Kette (SF Symbols hat kein „link.slash“).
                    .overlay { if !model.keepProportions { Rectangle().frame(width: 1.5, height: 20).rotationEffect(.degrees(45)).foregroundStyle(.secondary) } }
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.borderless)
            .help(model.keepProportions ? tr("Seitenverhältnis beibehalten") : tr("Seitenverhältnis frei"))
            .accessibilityLabel(model.keepProportions ? tr("Seitenverhältnis beibehalten") : tr("Seitenverhältnis frei"))
        }
    }
}

/// Zahlenfeld mit der Einheit im Feld, wie in Grafikprogrammen.
struct UnitField: View {
    let title: String
    @Binding var value: Double
    let unit: String
    var width: CGFloat = 76
    var body: some View {
        TextField(title, value: $value, format: .number.precision(.fractionLength(0...2)))
            .textFieldStyle(.roundedBorder).labelsHidden()
            .frame(width: width)
            .overlay(alignment: .trailing) {
                Text(unit).font(.callout).foregroundStyle(.secondary).padding(.trailing, 6).allowsHitTesting(false)
            }
    }
}

struct NumberField: View {
    let title: String
    @Binding var value: Double
    let unit: String
    /// Wert des Materialprofils; weicht der Wert ab, wird er orange markiert
    /// und lässt sich links vom Feld zurücksetzen.
    let profile: Double?
    init(_ title: String, value: Binding<Double>, unit: String, profile: Double? = nil) {
        self.title = title; self._value = value; self.unit = unit; self.profile = profile
    }
    private var deviates: Bool { profile.map { abs($0 - value) > 0.0001 } ?? false }
    var body: some View {
        LabeledContent {
            HStack(spacing: 6) {
                if deviates, let profile {
                    Button { value = profile } label: { Image(systemName: "arrow.clockwise") }
                        .buttonStyle(.borderless).foregroundStyle(.orange)
                        .help(tr("Auf den Profilwert {value} {unit} zurücksetzen", ["value": format(profile), "unit": unit]))
                }
                TextField(title, value: $value, format: .number.precision(.fractionLength(0...3)))
                    .multilineTextAlignment(.trailing).textFieldStyle(.roundedBorder)
                    .frame(width: 82).labelsHidden()
                Text(unit).foregroundStyle(.secondary).fixedSize().frame(width: 40, alignment: .leading)
            }
        } label: {
            Text(title).foregroundStyle(deviates ? Color.orange : Color.primary)
        }
    }
}

struct BedCanvas: NSViewRepresentable {
    @ObservedObject var model: AppModel
    func makeNSView(context: Context) -> BedView { BedView(model: model) }
    func updateNSView(_ view: BedView, context: Context) { view.model = model; view.needsDisplay = true }
}

/// Anfasser wie VisiCuts `EditRectangle`: Ecken und Kantenmitten als Anteil
/// von Breite und Höhe (wie `workspace::HANDLES` der egui-Oberfläche).
let resizeHandles: [CGPoint] = [CGPoint(x: 0, y: 0), CGPoint(x: 0.5, y: 0), CGPoint(x: 1, y: 0), CGPoint(x: 1, y: 0.5),
                                CGPoint(x: 1, y: 1), CGPoint(x: 0.5, y: 1), CGPoint(x: 0, y: 1), CGPoint(x: 0, y: 0.5)]

/// Rechteck in mm, wenn der Anfasser `handle` von `rect` auf `pointer` gezogen
/// wird (wie `workspace::drag_handle`): die gegenüberliegende Seite bleibt
/// stehen; Ecken skalieren `proportional` entlang der Diagonale, Kanten
/// ändern eine Seite und `proportional` die andere um ihre Mitte.
func dragHandle(_ rect: CGRect, _ handle: CGPoint, to pointer: CGPoint, proportional: Bool) -> CGRect {
    let minimum = 0.1
    let (w, h) = (Double(rect.width), Double(rect.height))
    func side(_ t: CGFloat) -> Double { t > 0.5 ? 1 : (t < 0.5 ? -1 : 0) }
    let (sx, sy) = (side(handle.x), side(handle.y))
    let fixed = CGPoint(x: rect.minX + rect.width * (1 - handle.x), y: rect.minY + rect.height * (1 - handle.y))
    let dx = Double(pointer.x - fixed.x) * sx, dy = Double(pointer.y - fixed.y) * sy
    var (nw, nh) = (w, h)
    switch (sx != 0, sy != 0) {
    case (true, true) where proportional:
        let t = max((dx * w + dy * h) / (w * w + h * h), minimum / min(w, h))
        (nw, nh) = (w * t, h * t)
    case (true, true): (nw, nh) = (max(dx, minimum), max(dy, minimum))
    case (true, false):
        nw = max(dx, minimum)
        if proportional { nh = max(h * nw / w, minimum) }
    case (false, true):
        nh = max(dy, minimum)
        if proportional { nw = max(w * nh / h, minimum) }
    default: break
    }
    func place(_ fixed: CGFloat, _ side: Double, _ size: Double) -> Double {
        side > 0 ? Double(fixed) : (side < 0 ? Double(fixed) - size : Double(fixed) - size / 2)
    }
    return CGRect(x: place(fixed.x, sx, nw), y: place(fixed.y, sy, nh), width: nw, height: nh)
}

/// Drehwinkel in Grad (Bildschirm mit y nach unten: positiv im
/// Uhrzeigersinn wie „transform“) zwischen −180° und 180°; `snap` rastet in
/// 15°-Schritten ein (wie `workspace::drag_rotation`).
func dragRotation(center: CGPoint, start: CGPoint, pointer: CGPoint, snap: Bool) -> Double {
    func angle(_ p: CGPoint) -> Double { atan2(Double(p.y - center.y), Double(p.x - center.x)) }
    var degrees = (angle(pointer) - angle(start)) * 180 / .pi
    degrees = (degrees + 180).truncatingRemainder(dividingBy: 360)
    if degrees < 0 { degrees += 360 }
    degrees -= 180
    return snap ? (degrees / 15).rounded() * 15 : degrees
}

final class BedView: NSView {
    var model: AppModel
    private var dragStart: NSPoint?
    private var originalPosition = NSPoint.zero
    /// Ziehen an einem Anfasser: Größe ändern (Anfasser, Lage beim Start in
    /// mm) oder drehen (Startpunkt, aktueller Winkel).
    private enum HandleDrag {
        case resize(CGPoint, CGRect)
        case rotate(NSPoint, Double)
    }
    private var handleDrag: HandleDrag?
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }
    /// In „Einzeln“: Objekt unter der Maus in der Liste als ausgewählt zeigen.
    override func mouseMoved(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        updateCursor(at: p)
        guard model.assignmentMode == .objects, model.project.hasArtwork else { return }
        guard let outlines = model.objectOutlines else { model.ensureObjectOutlines(); return }
        let mm = CGPoint(x: (p.x - bed.minX) / scale - model.project.x_mm, y: (p.y - bed.minY) / scale - model.project.y_mm)
        let hit = objectAt(outlines, mm, tolerance: 4 / scale)
        if hit != model.hoveredObject || (hit != nil && !model.hoverFromCanvas) { model.hoverObject(hit, fromCanvas: true) }
    }
    override func mouseExited(with event: NSEvent) {
        if model.hoverFromCanvas { model.hoverObject(nil) }
    }
    init(model: AppModel) {
        self.model = model
        super.init(frame: .zero)
        // Seit macOS 14 zeichnen NSViews sonst über ihren Rand, etwa über den Kopfbereich.
        clipsToBounds = true
    }
    required init?(coder: NSCoder) { fatalError("Not used") }

    private var scale: Double {
        // 32 pt links, 20 pt rechts (Platz für die Linealzahl 1000), 30 oben, 12 unten.
        min(max(1, bounds.width - 52) / model.project.bed_width_mm,
            max(1, bounds.height - 42) / model.project.bed_height_mm) * model.zoom
    }
    private var bed: NSRect {
        let pan = clamped(model.pan)
        return NSRect(x: 32 + pan.width, y: 30 + pan.height,
                      width: model.project.bed_width_mm * scale, height: model.project.bed_height_mm * scale)
    }
    /// Verschiebung nur innerhalb des Betts: Der Ursprung oben links kommt nicht
    /// über seine Ausgangslage hinaus, die Ecke unten rechts nicht über den Rand.
    private func clamped(_ pan: CGSize) -> CGSize {
        let width = model.project.bed_width_mm * scale, height = model.project.bed_height_mm * scale
        return CGSize(width: min(0, max(pan.width, bounds.width - 52 - width)),
                      height: min(0, max(pan.height, bounds.height - 42 - height)))
    }
    /// Zoomt um den Punkt unter dem Mauszeiger.
    private func zoom(by factor: Double, at point: NSPoint) {
        let before = bed, oldScale = scale
        let mm = CGPoint(x: (point.x - before.minX) / oldScale, y: (point.y - before.minY) / oldScale)
        model.zoomBy(factor)
        guard model.zoom > 1 else { needsDisplay = true; return }
        let newScale = scale
        model.pan = clamped(CGSize(width: point.x - mm.x * newScale - 32, height: point.y - mm.y * newScale - 30))
        needsDisplay = true
    }
    override func scrollWheel(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        // Trackpad (und Magic Mouse) liefern Gesten-Phasen, ein Mausrad nicht –
        // auch keines mit feiner Auflösung (z. B. weiches Scrollen bei Logitech).
        let gesture = event.phase != [] || event.momentumPhase != []
        if gesture && !event.modifierFlags.contains(.command) && model.zoom > 1 {
            // Zwei Finger auf dem Trackpad verschieben das vergrößerte Bett.
            let pan = clamped(model.pan)
            model.pan = clamped(CGSize(width: pan.width + event.scrollingDeltaX, height: pan.height + event.scrollingDeltaY))
            needsDisplay = true
        } else {
            // Mausrad (oder ⌘ + Scrollen) zoomt, auch über 100 % hinaus.
            let delta = event.hasPreciseScrollingDeltas ? event.scrollingDeltaY / 50 : event.scrollingDeltaY / 5
            guard delta != 0 else { return }
            zoom(by: pow(1.1, delta), at: point)
        }
    }
    override func magnify(with event: NSEvent) {
        zoom(by: 1 + event.magnification, at: convert(event.locationInWindow, from: nil))
    }
    /// Rechteck in Bett-mm als Ansichtskoordinaten.
    private func viewRect(_ x: Double, _ y: Double, _ width: Double, _ height: Double) -> NSRect {
        let origin = self.bed.origin, factor = self.scale
        return NSRect(x: origin.x + x * factor, y: origin.y + y * factor, width: width * factor, height: height * factor)
    }
    private func viewRect(_ mm: CGRect) -> NSRect {
        viewRect(Double(mm.minX), Double(mm.minY), Double(mm.width), Double(mm.height))
    }
    /// Das aktive Teil.
    private var motif: NSRect {
        let p = model.project
        return viewRect(p.x_mm, p.y_mm, p.width_mm, p.height_mm)
    }
    private var showsHandles: Bool { model.project.hasArtwork && !model.busy }
    /// Mittelpunkt eines Anfassers in Ansichtskoordinaten.
    private func handlePoint(_ handle: CGPoint) -> NSPoint {
        NSPoint(x: motif.minX + motif.width * handle.x, y: motif.minY + motif.height * handle.y)
    }
    /// Drehanfasser über der Mitte, bei wenig Platz (Lineal) darunter;
    /// dazu der Punkt auf der Kante, an dem seine Linie ansetzt.
    private var rotationKnob: (edge: NSPoint, knob: NSPoint) {
        motif.minY - 22 >= max(bed.minY, 30)
            ? (NSPoint(x: motif.midX, y: motif.minY), NSPoint(x: motif.midX, y: motif.minY - 22))
            : (NSPoint(x: motif.midX, y: motif.maxY), NSPoint(x: motif.midX, y: motif.maxY + 22))
    }
    /// Anfasser unter dem Punkt: Größe (Anteil) oder `nil` mit `rotate`.
    private func handle(at point: NSPoint) -> (resize: CGPoint?, rotate: Bool)? {
        guard showsHandles else { return nil }
        let knob = rotationKnob.knob
        if hypot(point.x - knob.x, point.y - knob.y) <= 8 { return (nil, true) }
        for handle in resizeHandles {
            let center = handlePoint(handle)
            if abs(point.x - center.x) <= 7 && abs(point.y - center.y) <= 7 { return (handle, false) }
        }
        return nil
    }
    private func updateCursor(at point: NSPoint) {
        guard let hit = handle(at: point) else { NSCursor.arrow.set(); return }
        guard let handle = hit.resize else { NSCursor.crosshair.set(); return }
        if #available(macOS 15.0, *) {
            let position: NSCursor.FrameResizePosition
            switch (handle.x, handle.y) {
            case (0, 0): position = .topLeft
            case (0.5, 0): position = .top
            case (1, 0): position = .topRight
            case (1, 0.5): position = .right
            case (1, 1): position = .bottomRight
            case (0.5, 1): position = .bottom
            case (0, 1): position = .bottomLeft
            default: position = .left
            }
            NSCursor.frameResize(position: position, directions: .all).set()
        } else if handle.y == 0.5 {
            NSCursor.resizeLeftRight.set()
        } else if handle.x == 0.5 {
            NSCursor.resizeUpDown.set()
        } else {
            NSCursor.crosshair.set()
        }
    }
    /// Index (in der Liste aller Teile) des obersten nicht aktiven Teils unter dem Punkt.
    private func inactivePart(at point: NSPoint) -> Int? {
        model.project.inactiveParts.reversed().first { entry in
            viewRect(entry.part.x_mm, entry.part.y_mm, entry.part.width_mm, entry.part.height_mm).contains(point)
        }?.index
    }
    override func draw(_ dirtyRect: NSRect) {
        // The bed is document content: keep a white paper surface so black SVG
        // paths stay visible even when macOS chrome uses Dark Mode.
        NSColor.white.setFill(); bed.fill()
        if model.showCamera, let camera = model.cameraImage {
            camera.draw(in: bed, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: [.interpolation: NSImageInterpolation.high])
        }
        let path = NSBezierPath(rect: bed); path.lineWidth = 1
        NSColor(calibratedWhite: 0.65, alpha: 1).setStroke(); path.stroke()
        // Raster und Lineal in der Reihe 1 – 2 – 5 (… 1, 2, 5, 10, 20, 50 mm …);
        // die Rasterlinien an den Linealzahlen sind kräftiger.
        func niceStep(_ minPoints: Double) -> Double {
            for exponent in -1...3 {
                for mantissa in [1.0, 2.0, 5.0] {
                    let step = mantissa * pow(10, Double(exponent))
                    if step <= 1000 && step * scale >= minPoints { return step }
                }
            }
            return 1000
        }
        let labelStep = niceStep(40)
        let visible = bed.intersection(bounds)
        func marks(_ length: Double, _ step: Double, from: Double, to: Double) -> [Double] {
            let first = max(0, floor(from / step)), last = min(floor(length / step + 1e-6), ceil(to / step))
            guard first <= last else { return [] }
            return (Int(first)...Int(last)).map { Double($0) * step }
        }
        let xRange = ((visible.minX - bed.minX) / scale, (visible.maxX - bed.minX) / scale)
        let yRange = ((visible.minY - bed.minY) / scale, (visible.maxY - bed.minY) / scale)
        if model.showGrid && !(model.showCamera && model.cameraImage != nil) && !visible.isEmpty {
            let step = niceStep(6)
            let fine = NSBezierPath(), major = NSBezierPath()
            fine.lineWidth = 0.5; major.lineWidth = 0.5
            func isMajor(_ mm: Double) -> Bool { abs((mm / labelStep).rounded() * labelStep - mm) < step * 0.01 }
            for x in marks(model.project.bed_width_mm, step, from: xRange.0, to: xRange.1) {
                let line = isMajor(x) ? major : fine
                line.move(to: NSPoint(x: bed.minX + x * scale, y: bed.minY))
                line.line(to: NSPoint(x: bed.minX + x * scale, y: bed.maxY))
            }
            for y in marks(model.project.bed_height_mm, step, from: yRange.0, to: yRange.1) {
                let line = isMajor(y) ? major : fine
                line.move(to: NSPoint(x: bed.minX, y: bed.minY + y * scale))
                line.line(to: NSPoint(x: bed.maxX, y: bed.minY + y * scale))
            }
            NSColor(calibratedWhite: 0.89, alpha: 1).setStroke(); fine.stroke()
            NSColor(calibratedWhite: 0.78, alpha: 1).setStroke(); major.stroke()
        }
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: bed).addClip()
        let hints: [NSImageRep.HintKey: Any] = [.interpolation: NSImageInterpolation.high]
        if model.showProcessing, let job = model.jobImage {
            // Die Auftragsvorschau umfasst alle Teile.
            job.draw(in: viewRect(model.project.partsBounds), from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: hints)
        } else if model.showProcessing, let live = model.processingImage {
            // Bearbeitung in Farbe, schon vor der Auftragsvorschau.
            live.draw(in: viewRect(model.project.partsBounds), from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: hints)
        } else {
            for part in model.partImages where !part.active {
                part.image?.draw(in: viewRect(part.rect), from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: hints)
            }
            model.image?.draw(in: motif, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: hints)
        }
        // Nicht aktive Teile mit dünnem grauen Rahmen, das aktive in Akzentfarbe.
        NSColor(calibratedWhite: 0.6, alpha: 1).setStroke()
        for entry in model.project.inactiveParts {
            let frame = NSBezierPath(rect: viewRect(entry.part.x_mm, entry.part.y_mm, entry.part.width_mm, entry.part.height_mm))
            frame.lineWidth = 0.75
            frame.stroke()
        }
        // Auswahl schwarz gestrichelt: Blau, Rot, Orange und Violett stehen
        // für Bearbeitungen und dürfen nicht verwechselt werden.
        let selection = NSBezierPath(rect: motif.insetBy(dx: 0.5, dy: 0.5)); selection.lineWidth = 1
        selection.setLineDash([5, 3], count: 2, phase: 0)
        NSColor(calibratedWhite: 0, alpha: 0.75).setStroke(); selection.stroke()
        // Objekt unter der Maus in der Objektliste: gelb (keine Bearbeitungsfarbe).
        if let outline = model.hoverOutline, model.assignmentMode == .objects {
            let path = NSBezierPath()
            for contour in outline {
                for (i, p) in contour.enumerated() {
                    let point = NSPoint(x: bed.minX + (model.project.x_mm + p.x) * scale, y: bed.minY + (model.project.y_mm + p.y) * scale)
                    if i == 0 { path.move(to: point) } else { path.line(to: point) }
                }
            }
            path.lineJoinStyle = .round; path.lineCapStyle = .round
            path.lineWidth = 4; NSColor.systemYellow.withAlphaComponent(0.9).setStroke(); path.stroke()
            path.lineWidth = 1; NSColor(calibratedWhite: 0, alpha: 0.7).setStroke(); path.stroke()
        }
        // Vorschau der Drehung: Motiv und Rahmen gedreht um die Mitte.
        if case .rotate(_, let degrees) = handleDrag {
            NSGraphicsContext.saveGraphicsState()
            let turn = NSAffineTransform()
            turn.translateX(by: motif.midX, yBy: motif.midY)
            turn.rotate(byDegrees: degrees)
            turn.translateX(by: -motif.midX, yBy: -motif.midY)
            turn.concat()
            model.image?.draw(in: motif, from: .zero, operation: .sourceOver, fraction: 0.6, respectFlipped: true, hints: hints)
            let frame = NSBezierPath(rect: motif); frame.lineWidth = 1
            frame.setLineDash([5, 3], count: 2, phase: 0)
            NSColor(calibratedWhite: 0, alpha: 0.75).setStroke(); frame.stroke()
            NSGraphicsContext.restoreGraphicsState()
        }
        NSGraphicsContext.restoreGraphicsState()
        // Lineale in eigenen Randstreifen über der Fläche: auch vergrößert oder
        // verschoben (und im Dunkelmodus) bleiben die Zahlen lesbar.
        NSColor.underPageBackgroundColor.setFill()
        NSRect(x: 0, y: 0, width: 30, height: bounds.height).fill()
        NSRect(x: 0, y: 0, width: bounds.width, height: 26).fill()
        let attributes: [NSAttributedString.Key: Any] = [.font: NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular), .foregroundColor: NSColor.secondaryLabelColor]
        func label(_ mm: Double) -> NSString { (labelStep >= 1 ? String(Int(mm.rounded())) : String(format: "%.1f", mm)) as NSString }
        for x in marks(model.project.bed_width_mm, labelStep, from: xRange.0, to: xRange.1) {
            let text = label(x), width = text.size(withAttributes: attributes).width
            let px = bed.minX + x * scale - width / 2
            // Nicht in die Ecke mit der Einheit schreiben.
            if px >= 30 { text.draw(at: NSPoint(x: px, y: 10), withAttributes: attributes) }
        }
        // Bei Nullpunkt unten links zählt die Y-Skala vom unteren Bettrand aufwärts.
        let bottomLeft = model.originBottomLeft
        for y in marks(model.project.bed_height_mm, labelStep, from: 0, to: model.project.bed_height_mm) {
            let position = bottomLeft ? model.project.bed_height_mm - y : y
            let py = bed.minY + position * scale - 6
            let text = label(y), width = text.size(withAttributes: attributes).width
            // Rechtsbündig am Streifen.
            if py >= 24 && py <= bounds.maxY - 10 { text.draw(at: NSPoint(x: 27 - width, y: py), withAttributes: attributes) }
        }
        // Einheit in der Ecke, an der sich die Lineale treffen.
        ("mm" as NSString).draw(at: NSPoint(x: 2, y: 10), withAttributes: attributes)
        drawHandles()
    }
    /// Weiße Anfasser mit dunklem Rand, darüber der Drehanfasser.
    private func drawHandles() {
        guard showsHandles else { return }
        let stroke = NSColor(calibratedWhite: 0, alpha: 0.8)
        let (edge, knob) = rotationKnob
        let line = NSBezierPath(); line.move(to: edge); line.line(to: knob); line.lineWidth = 1
        stroke.setStroke(); line.stroke()
        let circle = NSBezierPath(ovalIn: NSRect(x: knob.x - 4.5, y: knob.y - 4.5, width: 9, height: 9))
        NSColor.white.setFill(); circle.fill(); circle.lineWidth = 1; circle.stroke()
        for handle in resizeHandles {
            let center = handlePoint(handle)
            let square = NSBezierPath(roundedRect: NSRect(x: center.x - 3.5, y: center.y - 3.5, width: 7, height: 7), xRadius: 1, yRadius: 1)
            NSColor.white.setFill(); square.fill(); square.lineWidth = 1; square.stroke()
        }
        if case .rotate(_, let degrees) = handleDrag {
            let text = String(format: "%.0f°", degrees) as NSString
            text.draw(at: NSPoint(x: knob.x + 10, y: knob.y - 7),
                      withAttributes: [.font: NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium), .foregroundColor: NSColor.labelColor])
        }
    }
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard !model.busy else { return }
        if let hit = handle(at: point) {
            window?.makeFirstResponder(self)
            let p = model.project
            handleDrag = hit.resize.map { .resize($0, CGRect(x: p.x_mm, y: p.y_mm, width: p.width_mm, height: p.height_mm)) }
                ?? .rotate(point, 0)
            return
        }
        if !motif.contains(point) {
            // Klick auf ein anderes Teil wählt es aus; Ziehen verschiebt dann dieses Teil.
            guard let index = inactivePart(at: point) else { return }
            model.selectPart(index)
            guard model.project.active_part == index, motif.contains(point) else { return }
        }
        window?.makeFirstResponder(self)
        dragStart = point
        originalPosition = NSPoint(x: model.project.x_mm, y: model.project.y_mm)
        NSCursor.closedHand.push()
    }
    override func mouseDragged(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let shift = event.modifierFlags.contains(.shift)
        switch handleDrag {
        case .resize(let handle, let start):
            // Die Kette bei B/H hält das Seitenverhältnis, Umschalt kehrt das um.
            let mm = CGPoint(x: (point.x - bed.minX) / scale, y: (point.y - bed.minY) / scale)
            let rect = dragHandle(start, handle, to: mm, proportional: model.keepProportions != shift)
            model.project.x_mm = rect.minX; model.project.y_mm = rect.minY
            model.project.width_mm = rect.width; model.project.height_mm = rect.height
            needsDisplay = true
            return
        case .rotate(let start, _):
            handleDrag = .rotate(start, dragRotation(center: NSPoint(x: motif.midX, y: motif.midY), start: start, pointer: point, snap: shift))
            needsDisplay = true
            return
        case nil: break
        }
        guard let start = dragStart else { return }
        model.project.x_mm = min(max(0, originalPosition.x + (point.x - start.x) / scale), max(0, model.project.bed_width_mm - model.project.width_mm))
        model.project.y_mm = min(max(0, originalPosition.y + (point.y - start.y) / scale), max(0, model.project.bed_height_mm - model.project.height_mm))
        needsDisplay = true
    }
    override func mouseUp(with event: NSEvent) {
        if case .rotate(_, let degrees) = handleDrag, abs(degrees) >= 0.1 {
            // Dreht wie „Drehen“ im Inspektor um die Mitte (⌘Z widerruft).
            model.transformArtwork(rotate: degrees)
        }
        if handleDrag != nil {
            handleDrag = nil
            needsDisplay = true
            updateCursor(at: convert(event.locationInWindow, from: nil))
        }
        if dragStart != nil { NSCursor.pop() }
        dragStart = nil
    }
    override func keyDown(with event: NSEvent) {
        guard !model.busy else { return }
        let step = event.modifierFlags.contains(.shift) ? 10.0 : 1.0
        switch event.keyCode {
        case 123: model.project.x_mm = max(0, model.project.x_mm - step)
        case 124: model.project.x_mm = min(max(0, model.project.bed_width_mm - model.project.width_mm), model.project.x_mm + step)
        case 125: model.project.y_mm = min(max(0, model.project.bed_height_mm - model.project.height_mm), model.project.y_mm + step)
        case 126: model.project.y_mm = max(0, model.project.y_mm - step)
        // ⌫, ⌘⌫ und Entf löschen das Motiv (⌘Z widerruft).
        case 51, 117: model.deleteArtwork()
        default: super.keyDown(with: event)
        }
    }
    override func menu(for event: NSEvent) -> NSMenu? {
        let point = convert(event.locationInWindow, from: nil)
        guard !model.busy, model.project.hasArtwork else { return nil }
        if !motif.contains(point) {
            // Rechtsklick auf ein anderes Teil wählt es zuerst aus.
            guard let index = inactivePart(at: point) else { return nil }
            model.selectPart(index)
            guard model.project.active_part == index else { return nil }
        }
        window?.makeFirstResponder(self)
        let menu = NSMenu()
        func item(_ title: String, _ action: Selector) {
            let entry = NSMenuItem(title: title, action: action, keyEquivalent: "")
            entry.target = self
            menu.addItem(entry)
        }
        item(tr("Duplizieren"), #selector(duplicateArtwork))
        item(tr("Teil duplizieren"), #selector(duplicatePart))
        item(tr("Kopien im Raster …"), #selector(arrayCopies))
        item(tr("Teile verschachteln (Nesting) …"), #selector(nestParts))
        menu.addItem(.separator())
        item(tr("90° nach links drehen"), #selector(rotateLeft))
        item(tr("90° nach rechts drehen"), #selector(rotateRight))
        item(tr("Horizontal spiegeln"), #selector(flipHorizontal))
        item(tr("Vertikal spiegeln"), #selector(flipVertical))
        menu.addItem(.separator())
        item(model.project.parts.isEmpty ? tr("Löschen") : tr("Teil entfernen"), #selector(deleteArtwork))
        return menu
    }
    @objc func duplicateArtwork() { model.duplicateArtwork() }
    @objc func duplicatePart() { model.duplicatePart() }
    @objc func arrayCopies() { model.composeTool = .array }
    @objc func nestParts() { model.composeTool = .nest }
    @objc func rotateLeft() { model.transformArtwork(rotate: -90) }
    @objc func rotateRight() { model.transformArtwork(rotate: 90) }
    @objc func flipHorizontal() { model.transformArtwork(flipHorizontal: true) }
    @objc func flipVertical() { model.transformArtwork(flipVertical: true) }
    @objc func centerArtwork() { model.center() }
    @objc func deleteArtwork() { model.deleteArtwork() }
}

extension BedView: NSMenuItemValidation {
    func validateMenuItem(_ item: NSMenuItem) -> Bool { model.project.hasArtwork && !model.busy }
}

/// Device list, rotary jobs and the camera pipeline, using an isolated
/// settings directory and a local image instead of a network camera.
@MainActor
func testDevicesRotaryAndCamera(_ model: AppModel) throws {
    let fau = model.device
    guard model.devices.devices.count == 1, fau.name == "LTT iLaser 4000", fau.rotary_axis,
          fau.camera_calibration?.reference_points.count == 6, model.project.hostname == "lasercutter2"
    else { throw CoreError("FAU-Lasercutter nicht als Standard eingerichtet") }
    var store = model.devices
    var second = fau
    second.name = "LTT Test"; second.hostname = "127.0.0.1"; second.port = 9101; second.rotary_axis = false
    let camera = FileManager.default.temporaryDirectory.appendingPathComponent("visicut-camera-\(UUID().uuidString).png")
    let pixels = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 100, pixelsHigh: 60, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    try pixels.representation(using: .png, properties: [:])!.write(to: camera)
    defer { try? FileManager.default.removeItem(at: camera) }
    second.camera_url = camera.path
    second.camera_calibration = CameraCalibration(reference_points: [[0, 0], [1000, 600]], view_points: [[0, 0], [100, 60]])
    // Kamera-Anmeldung: Passwort nur im (Test-)Schlüsselbund, nicht in der Geräteliste.
    second.camera_user = "laser"; second.camera_password = "geheim"; second.camera_refresh_ms = 500
    store.devices.append(second)
    guard model.saveDevices(store) else { throw CoreError("Geräteliste nicht gespeichert") }
    model.project.rotary_axis = true
    model.selectDevice(1)
    guard model.project.hostname == "127.0.0.1", model.project.port == 9101, !model.project.rotary_axis
    else { throw CoreError("Gerätewechsel übernimmt Ziel oder Drehachse nicht") }
    let reloaded: DeviceList = try RustCore.decode("devices")
    guard CameraKeychain.filled(reloaded.store) == model.devices, reloaded.store.selected == 1 else { throw CoreError("Geräteliste nicht dauerhaft gespeichert") }
    let configDir = ProcessInfo.processInfo.environment["VISICUT_RUST_CONFIG_DIR"].map { URL(fileURLWithPath: $0) }
    if let file = configDir?.appendingPathComponent("devices.json") {
        let saved = try String(contentsOf: file, encoding: .utf8)
        guard !saved.contains("geheim"), saved.contains("laser"), model.device.camera_password == "geheim",
              model.device.camera_refresh_ms == 500 else { throw CoreError("Kamera-Passwort landet in der Geräteliste") }
    }

    let background: CameraBackground = try RustCore.decode("camera_background", project: model.project,
        values: ["device": RustCore.json(model.device)])
    guard let image = NSImage(data: Data(background.png)), image.size.width >= 100 else { throw CoreError("Kamerahintergrund fehlt") }
    let shot: CameraPicture = try RustCore.decode("camera_image", values: ["device": RustCore.json(model.device)])
    guard shot.width == 100, shot.height == 60 else { throw CoreError("Kamerabild falsch gelesen") }
    let page: ProjectResponse = try RustCore.decode("calibration_page", project: model.project,
        values: ["reference_points": fau.camera_calibration!.reference_points])
    let calibrationJob: PreparedJob = try RustCore.decode("prepare", project: page.project)
    guard page.objects?.count == 1, calibrationJob.steps.first?.operation == .mark else { throw CoreError("Kalibrierseite ungültig") }

    // Zeilenversatz: Testmuster mit einem Schritt je Block, Ergebnis in der Tabelle.
    let pattern: ProjectResponse = try RustCore.decode("shift_pattern", project: model.project, values: ["power_percent": 25.0])
    let patternJob: PreparedJob = try RustCore.decode("prepare", project: pattern.project)
    guard pattern.project.steps.count == 50, pattern.project.steps.first?.engrave_shift_px == -2,
          patternJob.steps.count == 50, patternJob.warnings.contains(where: { $0.contains("Zeilenversatz") })
    else { throw CoreError("Zeilenversatz-Testmuster ungültig") }
    var best = Array(repeating: 3, count: 10); best[9] = 5
    guard let calibrated = model.applyShiftTest(model.device.machine, best: best),
          calibrated.engrave_shift[0] == model.device.machine.engrave_shift[0],
          calibrated.engrave_shift[9] != model.device.machine.engrave_shift[9]
    else { throw CoreError("Zeilenversatz nicht übernommen") }
    var gentle = model.project
    gentle.machine.curve_acceleration_mm_s2 = MachineSettings.firmwareCurveAcceleration
    gentle.steps = []; gentle.operation = .cut
    let gentleJob: PreparedJob = try RustCore.decode("prepare", project: gentle)
    guard gentleJob.warnings.contains(where: { $0.contains("Experimentelle Kurvenbeschleunigung") })
    else { throw CoreError("Hinweis zur Kurvenbeschleunigung fehlt") }

    model.selectDevice(0)
    guard model.project.hostname == "lasercutter2" else { throw CoreError("Rückwechsel zum FAU-Gerät fehlgeschlagen") }
    model.project.rotary_axis = true
    model.project.rotary_diameter_mm = 80
    let rotary: PreparedJob = try RustCore.decode("prepare", project: model.project)
    guard rotary.warnings.contains(where: { $0.contains("Adjust rotary temp") }) else { throw CoreError("Drehachsen-Hinweis fehlt") }
    model.project.rotary_diameter_mm = 2
    guard (try? RustCore.decode("prepare", project: model.project) as PreparedJob) == nil else { throw CoreError("Zu kleiner Durchmesser akzeptiert") }
    model.project.rotary_axis = false
    model.project.rotary_diameter_mm = 100
}

/// Rule-based mapping, engraving options, parameter sets and the material
/// library; restores the project afterwards.
@MainActor
func testRulesEngravingAndMaterials(_ model: AppModel) throws {
    let saved = model.project
    defer { model.project = saved }
    model.setAssignmentMode(.rules)
    guard model.assignmentMode == .rules, let mapping = model.mapping,
          let template = mapping.predefined.first(where: { $0.name.hasPrefix("Rot schneiden") }),
          mapping.suggestions(.strokeColor).contains(where: { $0.value == "#ef7141" })
    else { throw CoreError("Regelzuordnung nicht verfügbar") }
    model.applyPredefined(template)
    guard let cut = model.project.steps.firstIndex(where: { $0.operation == .cut }) else { throw CoreError("Vorlage ohne Schnitt") }
    model.project.steps[cut].filters = [Filter(attribute: .strokeColor, value: "#ef7141", compare: false, inverted: false)]
    model.project.steps[cut].additional = [ParameterSet(power_percent: 50, speed_percent: 30, passes: 1)]
    // „+“ fügt die gleiche Bearbeitung direkt darunter ein; das Umschalten
    // behält die Bedingungen und übernimmt die neue Bearbeitung.
    let before = model.project.steps.count
    model.insertRuleStep(after: cut)
    guard model.project.steps.count == before + 1, model.project.steps[cut + 1].operation == .cut,
          model.project.steps[cut + 1].filters == [] else { throw CoreError("„+“ fügt keinen gleichen Schritt darunter ein") }
    model.project.steps[cut + 1].filters = [Filter(attribute: .id, value: "Gravurkreis", compare: false, inverted: false)]
    model.changeRuleStep(cut + 1, to: .engrave3d)
    guard model.project.steps[cut + 1].operation == .engrave3d,
          model.project.steps[cut + 1].filters?.first?.value == "Gravurkreis" else { throw CoreError("Bearbeitung nicht umgeschaltet") }
    // Für den weiteren Test ans Ende verschieben (wie bisher „Schritt hinzufügen“).
    model.project.steps.append(model.project.steps.remove(at: cut + 1))
    let deep = model.project.steps.count - 1
    model.project.steps[deep].filters = [Filter(attribute: .id, value: "Gravurkreis", compare: false, inverted: false)]
    model.project.steps[deep].power_percent = 40
    model.project.ignore_filters = [[Filter(attribute: .id, value: "Dreieck", compare: false, inverted: false)]]
    model.refreshMapping()
    // Ignored and rule-matched objects are not part of the rest.
    guard model.objectCount(cut) == 1, model.objectCount(deep) == 1,
          let rest = model.project.steps.firstIndex(where: \.rest), model.objectCount(rest) == 0
    else { throw CoreError("Regeln wählen falsche Objekte: \(model.mapping?.selections ?? [])") }
    model.project.ignore_filters = []
    model.refreshMapping()
    guard model.objectCount(rest) == 1 else { throw CoreError("Rest ohne Ignorierregel falsch") }
    if let engrave = model.project.steps.firstIndex(where: { $0.operation == .engrave }) {
        model.project.steps[engrave].raster.dithering = .floydSteinberg
        model.project.steps[engrave].raster.bidirectional = false
    }
    let job: PreparedJob = try RustCore.decode("prepare", project: model.project)
    guard job.jobs.map(\.operation) == [.engrave, .engrave3d, .cut],
          job.steps.first(where: { $0.operation == .cut })?.parameter_sets == 2,
          job.steps.first(where: { $0.operation == .engrave })?.description.contains("Floyd-Steinberg") == true,
          job.timeline.programs.contains(where: { $0.operation == .engrave3d })
    else { throw CoreError("Regel-Auftrag falsch: \(job.jobs.map(\.name))") }
    model.project.steps[cut].filters = [Filter(attribute: .strokeWidth, value: "breit", compare: false, inverted: false)]
    model.refreshMapping()
    guard model.mappingError != nil else { throw CoreError("Ungültige Linienstärke nicht gemeldet") }

    var materials = model.materials
    materials.append(Material(id: "ui-test", name: "UI-Test Material", profiles: [
        MaterialProfile(thickness_mm: 2, operation: .engrave3d, power_percent: 35, speed_percent: 80, source: nil)]))
    guard model.saveMaterials(materials) else { throw CoreError("Materialbibliothek nicht gesichert") }
    let reloaded: MaterialCatalog = try RustCore.decode("materials")
    guard reloaded.custom, reloaded.materials.contains(where: { $0.name == "UI-Test Material" }) else {
        throw CoreError("Eigene Materialbibliothek nicht geladen")
    }
    var duplicate = materials
    duplicate.append(materials[0])
    guard (try? RustCore.call("save_materials", values: ["library": RustCore.json(MaterialLibrary(source: "", device: "", materials: duplicate))])) == nil
    else { throw CoreError("Doppeltes Material akzeptiert") }
    model.resetMaterials()
    guard !model.materialsCustom, !model.materials.contains(where: { $0.name == "UI-Test Material" }),
          model.materials.contains(where: { $0.profiles.contains { $0.operation == .mark } })
    else { throw CoreError("FAU-Bibliothek nicht wiederhergestellt") }
}

@MainActor
func runUITest(_ model: AppModel) {
    // Explicit development mode: exercises native state, persistence and Rust
    // export without contacting the physical cutter or opening file dialogs.
    do {
        guard let root = NSApp.windows.first(where: { $0.isVisible })?.contentView else { throw CoreError("Arbeitsfenster fehlt") }
        root.layoutSubtreeIfNeeded()
        guard let materialButton = popupControl(root, identifier: "materialPicker") else {
            dumpNativeViews(root)
            throw CoreError("Natives Materialmenü fehlt in der View-Hierarchie")
        }
        guard materialButton.indexOfItem(withTitle: "Acryl") >= 0 else { throw CoreError("Acryl fehlt im nativen Menü") }
        materialButton.selectItem(withTitle: "Acryl")
        guard let action = materialButton.action,
              NSApp.sendAction(action, to: materialButton.target, from: materialButton),
              model.project.material == "Acryl" else { throw CoreError("Natives Materialmenü ändert das Projekt nicht") }
        model.project.thickness_mm = 3
        model.project.operation = .cut
        model.applyPreset()
        guard model.preset != nil, model.project.power_percent == 100,
              abs(model.project.speed_percent - 9) < 0.001 else { throw CoreError("Materialprofil nicht übernommen") }
        model.project.operation = .engrave
        model.applyPreset()
        guard model.project.power_percent == 20, abs(model.project.speed_percent - 100) < 0.01 else { throw CoreError("Gravurprofil nicht übernommen") }
        model.selectMaterial("Sperrholz Pappel")
        guard let material = model.selectedMaterial, material.thicknesses.contains(model.project.thickness_mm) else { throw CoreError("Stärkenauswahl nicht synchronisiert") }
        model.selectMaterial("Acryl")
        model.project.thickness_mm = 3
        model.project.operation = .cut
        model.applyPreset()
        model.center()
        let prepared: PreparedJob = try RustCore.decode("prepare", project: model.project)
        guard prepared.jobs.first?.bytes.starts(with: Array("LTT".utf8)) == true else { throw CoreError("LTT-Export fehlgeschlagen") }
        let data = try JSONEncoder().encode(model.project)
        let reloaded = try JSONDecoder().decode(Project.self, from: data)
        guard reloaded == model.project else { throw CoreError("Projekt speichert Auswahl nicht") }
        let temporary = FileManager.default.temporaryDirectory.appendingPathComponent("visicut-native-ui-\(UUID().uuidString).vcr")
        defer { try? FileManager.default.removeItem(at: temporary) }
        model.projectURL = temporary
        guard model.save() else { throw CoreError("Projekt konnte nicht gesichert werden") }
        let loaded: ProjectResponse = try RustCore.decode("load", path: temporary.path)
        guard loaded.project.material == "Acryl", loaded.project.thickness_mm == 3,
              loaded.project.operation == .cut, loaded.project.power_percent == 100,
              abs(loaded.project.speed_percent - model.project.speed_percent) < 0.001,
              loaded.preview != nil else { throw CoreError("Rust lädt Materialauswahl nicht korrekt") }
        guard loaded.project.cut_order == .shortestTravel else { throw CoreError("Standard-Schnittreihenfolge nicht „Kürzeste Leerfahrten“") }
        model.project.cut_order = .shortestTravel
        guard model.save() else { throw CoreError("Schnittreihenfolge konnte nicht gesichert werden") }
        let ordered: ProjectResponse = try RustCore.decode("load", path: temporary.path)
        let orderedJob: PreparedJob = try RustCore.decode("prepare", project: model.project)
        guard ordered.project.cut_order == .shortestTravel, orderedJob.warnings.isEmpty
        else { throw CoreError("Schnittreihenfolge geht beim Sichern/Laden verloren") }
        // Ältere Projektdateien ohne das Feld laden mit der VisiCut-Reihenfolge.
        guard var legacy = try JSONSerialization.jsonObject(with: Data(contentsOf: temporary)) as? [String: Any]
        else { throw CoreError("Gesichertes Projekt ist kein JSON-Objekt") }
        legacy.removeValue(forKey: "cut_order")
        try JSONSerialization.data(withJSONObject: legacy).write(to: temporary, options: .atomic)
        let legacyLoaded: ProjectResponse = try RustCore.decode("load", path: temporary.path)
        guard legacyLoaded.project.cut_order == .visiCut else { throw CoreError("Ältere Projektdatei ohne Schnittreihenfolge nicht geladen") }
        model.project.cut_order = .visiCut
        try testVectorize(model)
        model.projectURL = nil
        let oldWidth = model.project.width_mm, oldHeight = model.project.height_mm
        model.changeWidth(oldWidth * 2)
        guard abs(model.project.height_mm - oldHeight * 2) < 0.001 else { throw CoreError("Proportionale Skalierung fehlgeschlagen") }
        model.changeWidth(oldWidth)
        // PLF für Java-VisiCut: Archiv mit Hinweisen, wieder lesbar.
        let plf = FileManager.default.temporaryDirectory.appendingPathComponent("visicutrust-ui-test.plf")
        let exported: WarningsResponse = try RustCore.decode("export_plf", project: model.project, path: plf.path)
        let reopened: ProjectResponse = try RustCore.decode("import", project: model.project, path: plf.path)
        try? FileManager.default.removeItem(at: plf)
        guard !exported.warnings.isEmpty, reopened.project.hasArtwork else { throw CoreError("PLF-Export fehlgeschlagen") }
        // Anfasser: Ecke proportional, Kante mit Kette um die Mitte, Drehung rastet ein.
        let box = CGRect(x: 10, y: 20, width: 40, height: 20)
        guard dragHandle(box, CGPoint(x: 1, y: 1), to: CGPoint(x: 90, y: 60), proportional: true) == CGRect(x: 10, y: 20, width: 80, height: 40),
              dragHandle(box, CGPoint(x: 0, y: 0), to: CGPoint(x: 30, y: 30), proportional: true) == CGRect(x: 30, y: 30, width: 20, height: 10),
              dragHandle(box, CGPoint(x: 1, y: 0.5), to: CGPoint(x: 90, y: 0), proportional: true) == CGRect(x: 10, y: 10, width: 80, height: 40),
              dragHandle(box, CGPoint(x: 0, y: 0.5), to: CGPoint(x: 30, y: 0), proportional: false) == CGRect(x: 30, y: 20, width: 20, height: 20),
              abs(dragRotation(center: .zero, start: CGPoint(x: 10, y: 0), pointer: CGPoint(x: 0, y: 10), snap: false) - 90) < 1e-6,
              dragRotation(center: .zero, start: CGPoint(x: 10, y: 0), pointer: CGPoint(x: 10, y: 2), snap: true) == 15
        else { throw CoreError("Anfasser rechnen falsch") }
        model.center()
        guard model.objects.count == 3 else { throw CoreError("SVG-Objektliste fehlt") }
        model.setIndividual(true)
        model.assign(1, to: "Engrave")
        model.assign(2, to: "Mark")
        // Marking uses the FAU mark profile if there is one, never the cut parameters.
        let markPreset = model.selectedMaterial?.profiles.first {
            abs($0.thickness_mm - model.project.thickness_mm) < 0.0001 && $0.operation == .mark && $0.power_percent > 0
        }
        guard let markIndex = model.project.steps.firstIndex(where: { $0.operation == .mark }),
              model.project.steps[markIndex].power_percent == (markPreset?.power_percent ?? 0)
        else { throw CoreError("Markieren übernimmt ungewollt Schnittparameter") }
        model.project.steps[markIndex].power_percent = 5
        model.project.steps[markIndex].speed_percent = 60
        model.project.steps[markIndex].passes = 2
        let mixed: PreparedJob = try RustCore.decode("prepare", project: model.project)
        guard mixed.steps.count == 3, mixed.steps[0].operation == .engrave,
              mixed.steps[1].operation == .mark, mixed.steps[2].operation == .cut, mixed.estimated_seconds > 0,
              NSImage(data: Data(mixed.preview_png)) != nil else { throw CoreError("Gemischte Auftragsvorschau fehlt") }
        try testJobSimulation(mixed)
        model.projectURL = temporary
        guard model.save() else { throw CoreError("Objektzuordnung konnte nicht gespeichert werden") }
        let mixedReloaded: ProjectResponse = try RustCore.decode("load", path: temporary.path)
        guard mixedReloaded.project.steps == model.project.steps else { throw CoreError("Zuordnung oder Parameter gingen verloren") }
        model.projectURL = nil
        model.preparedJob = mixed
        model.jobImage = NSImage(data: Data(mixed.preview_png))
        model.project.x_mm += 1
        guard model.preparedJob == nil, model.jobImage == nil else { throw CoreError("Veraltete Auftragsvorschau bleibt aktiv") }
        model.project.x_mm -= 1
        model.preparedJob = mixed
        model.jobImage = NSImage(data: Data(mixed.preview_png))
        try testDevicesRotaryAndCamera(model)
        try testRulesEngravingAndMaterials(model)
        try testComposeTools(model)
        // Exercise the actual asynchronous preparation and sheet presentation.
        model.preparedJob = nil
        model.jobImage = nil
        model.previewJob()
        model.dirty = false
        print("Native UI state tests passed: material, thickness, operation, presets, proportional scaling, handles, PLF export, camera login, native save / Rust reload, cut order and legacy files, bitmap vectorization, Rust export, object assignments, mixed job preview and estimate, stale preview invalidation, devices, rotary axis, camera background and calibration, rule mapping, 3D engraving, dithering, parameter sets, material library, text, sign, copies, nesting")
    } catch {
        fputs("Native UI tests failed: \(error)\n", stderr)
        exit(1)
    }
    captureUITest(model, deadline: Date().addingTimeInterval(20))
}

@MainActor
func captureUITest(_ model: AppModel, deadline: Date) {
    DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
        if model.busy, Date() < deadline { captureUITest(model, deadline: deadline); return }
        guard !model.busy, model.preparedJob != nil, model.showJobPreview else {
            fputs("Native UI tests failed: asynchronous preview not ready\n", stderr); exit(1)
        }
        // SwiftUI presents sheets using different window hierarchies across
        // macOS versions. Wait for the actual slider to enter a visible view,
        // rather than assuming that updating the model has presented the sheet.
        let windows = NSApp.windows.filter { $0.isVisible }
        let roots = windows.flatMap { window in
            ([window] + window.sheets + (window.childWindows ?? [])).compactMap { $0.contentView }
        }
        for root in roots { root.layoutSubtreeIfNeeded() }
        guard let view = roots.first(where: { simulationSlider($0) != nil }),
              let slider = simulationSlider(view), let prepared = model.preparedJob else {
            if Date() < deadline { captureUITest(model, deadline: deadline); return }
            fputs("Native UI tests failed: preview slider not presented before deadline (\(windows.count) visible windows)\n", stderr)
            for window in windows {
                fputs("Window: \(window.title), sheets: \(window.sheets.count), children: \(window.childWindows?.count ?? 0)\n", stderr)
            }
            for root in roots { dumpNativeViews(root) }
            exit(1)
        }
        // Finish the asynchronous setup before checking that slider actions do
        // not dirty the document. SwiftUI may commit focused fields while the
        // preview is being presented on older macOS versions.
        model.dirty = false
        let document = model.project
        @MainActor func moveSlider(_ value: Double) {
            slider.doubleValue = value
            guard let action = slider.action, NSApp.sendAction(action, to: slider.target, from: slider),
                  abs(model.playback.seconds - value) < 0.001 else {
                fputs("Native UI tests failed: Zeitslider ohne Wirkung\n", stderr); exit(1)
            }
        }
        moveSlider(prepared.estimated_seconds)
        moveSlider(0)
        let markRun = prepared.timeline.runs.first { prepared.timeline.programs[$0.program_index].operation == .mark }!
        moveSlider((markRun.entry_end_seconds + markRun.end_seconds) / 2)
        guard model.project == document, !model.dirty else {
            fputs("Native UI tests failed: moving the slider changed the document (content: \(model.project != document), dirty: \(model.dirty))\n", stderr); exit(1)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            guard let image = view.bitmapImageRepForCachingDisplay(in: view.bounds) else {
                fputs("Native UI tests failed: could not capture preview view\n", stderr); exit(1)
            }
            view.cacheDisplay(in: view.bounds, to: image)
            if let index = CommandLine.arguments.firstIndex(of: "--capture"), CommandLine.arguments.count > index + 1 {
                guard let png = image.representation(using: .png, properties: [:]) else {
                    fputs("Native UI tests failed: could not encode preview PNG\n", stderr); exit(1)
                }
                do { try png.write(to: URL(fileURLWithPath: CommandLine.arguments[index + 1])) }
                catch { fputs("Capture failed: \(error)\n", stderr); exit(1) }
            }
            print("Native simulation tests passed: slider start/end/backward, interpolation, playback, three operation jobs")
            exit(0)
        }
    }
}

@MainActor
func simulationSlider(_ view: NSView) -> NSSlider? {
    if let slider = view as? NSSlider, slider.accessibilityIdentifier() == "jobTimeSlider" { return slider }
    for child in view.subviews { if let slider = simulationSlider(child) { return slider } }
    return nil
}

@MainActor
func dumpNativeViews(_ view: NSView, depth: Int = 0) {
    guard depth < 40 else { return }
    if depth < 4 || view is NSPopUpButton {
        fputs("View \(depth): \(type(of: view)), identifier=\(view.accessibilityIdentifier()), frame=\(view.frame)\n", stderr)
    }
    for child in view.subviews { dumpNativeViews(child, depth: depth + 1) }
}

@MainActor
func popupControl(_ view: NSView, identifier: String, depth: Int = 0) -> NSPopUpButton? {
    guard depth < 40 else { return nil }
    if let popup = view as? NSPopUpButton, popup.accessibilityIdentifier() == identifier { return popup }
    for child in view.subviews {
        if let found = popupControl(child, identifier: identifier, depth: depth + 1) { return found }
    }
    return nil
}
