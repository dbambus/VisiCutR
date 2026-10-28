#target illustrator
// "An VisiCutR senden" fuer Adobe Illustrator CS5.1 oder neuer (ExtendScript).
//
// Exportiert das aktive Dokument (oder nur die Auswahl, an ihrer Lage auf der
// Zeichenflaeche) als SVG, Texte als Pfade, und fuegt es in VisiCutR als Teil
// hinzu. Ist dort schon ein Teil gleichen Namens, ersetzt VisiCutR nur dessen
// Grafik; Lage und Zuordnung bleiben (wie --update --add der Befehlszeile).
//
// Die Uebergabe geht wie bei einem zweiten Start von VisiCutR an die laufende
// Instanz (Port und Kennwort in instance.json im Einstellungsordner); laeuft
// VisiCutR nicht, startet das Skript es und wartet darauf. Den Programmpfad
// legt VisiCutR bei jedem Start in visicutr-path.txt im Einstellungsordner ab.
//
// Nur ASCII: Illustrator liest Skripte ohne BOM in der Systemcodierung.
// Teil von VisiCutR, LGPL-3.0-or-later.

var VisiCutR = (function () {
    var PATH_FILE = "visicutr-path.txt";
    var INSTANCE_FILE = "instance.json";
    // So lange wartet das Skript nach dem Start auf VisiCutR.
    var START_TIMEOUT_MS = 30000;

    // Zeichenkette als JSON; alles ausser druckbarem ASCII als \uXXXX, damit
    // die Codierung der Verbindung keine Rolle spielt.
    function jsonString(text) {
        var out = "\"";
        for (var i = 0; i < text.length; i++) {
            var c = text.charAt(i);
            var code = text.charCodeAt(i);
            if (c === "\"" || c === "\\") {
                out += "\\" + c;
            } else if (code < 0x20 || code > 0x7e) {
                var hex = code.toString(16);
                while (hex.length < 4) {
                    hex = "0" + hex;
                }
                out += "\\u" + hex;
            } else {
                out += c;
            }
        }
        return out + "\"";
    }

    // Eine Zeile des Protokolls aus src/instance.rs.
    function requestLine(token, add, update, files) {
        var list = [];
        for (var i = 0; i < files.length; i++) {
            list.push(jsonString(files[i]));
        }
        return "{\"token\":" + jsonString(token) + ",\"add\":" + (add ? "true" : "false") +
            ",\"update\":" + (update ? "true" : "false") + ",\"files\":[" + list.join(",") + "]}";
    }

    // Port und Kennwort aus instance.json; null, wenn unbrauchbar.
    function parseAddress(text) {
        if (!text) {
            return null;
        }
        var port = /"port"\s*:\s*(\d+)/.exec(text);
        var token = /"token"\s*:\s*"([^"\\]*)"/.exec(text);
        if (!port || !token) {
            return null;
        }
        return { port: parseInt(port[1], 10), token: token[1] };
    }

    // Dateiname ohne Endung, nur Zeichen, die auf jedem System gehen; wie
    // document_name der Inkscape-Erweiterung (Umlaute ohne Punkte).
    function documentName(name) {
        var base = String(name || "").replace(/\.[A-Za-z0-9]{1,4}$/, "");
        var plain = { "\u00e4": "a", "\u00f6": "o", "\u00fc": "u", "\u00c4": "A", "\u00d6": "O",
            "\u00dc": "U", "\u00df": "ss", "\u00e9": "e", "\u00e8": "e", "\u00e0": "a" };
        var out = "";
        for (var i = 0; i < base.length; i++) {
            var c = base.charAt(i);
            if (plain.hasOwnProperty(c)) {
                out += plain[c];
            } else if (/[A-Za-z0-9._\- ]/.test(c)) {
                out += c;
            }
        }
        out = out.replace(/^[ .]+|[ .]+$/g, "");
        return out || "Zeichnung";
    }

    // Unter macOS steht im Pfad das Programm im Bundle; starten laesst sich
    // nur das Bundle selbst.
    function launchTarget(program) {
        var bundle = /^(.*\.app)\/Contents\/MacOS\/[^\/]+$/.exec(program);
        return bundle ? bundle[1] : program;
    }

    function trim(text) {
        return String(text).replace(/^\s+|\s+$/g, "");
    }

    // ---- Ab hier nur in Illustrator ----

    function separator() {
        return $.os.indexOf("Windows") === 0 ? "\\" : "/";
    }

    // Einstellungsordner von VisiCutR (device::config_dir).
    function configDir() {
        var custom = $.getenv("VISICUTR_CONFIG_DIR");
        return custom ? custom : Folder.userData.fsName + separator() + "VisiCutR";
    }

    function readText(path) {
        var file = new File(path);
        if (!file.exists) {
            return null;
        }
        file.encoding = "UTF-8";
        if (!file.open("r")) {
            return null;
        }
        var text = file.read();
        file.close();
        return text;
    }

    function writeText(path, text) {
        var file = new File(path);
        file.parent.create();
        file.encoding = "UTF-8";
        if (file.open("w")) {
            file.write(text);
            file.close();
        }
    }

    function exists(path) {
        return !!path && (new File(path).exists || new Folder(path).exists);
    }

    // Programmpfad: VISICUTR_BIN, der von VisiCutR gemerkte Pfad, uebliche
    // Orte; sonst fragen und die Antwort merken.
    function findProgram() {
        var sep = separator();
        var candidates = [$.getenv("VISICUTR_BIN"), readText(configDir() + sep + PATH_FILE)];
        if (sep === "\\") {
            var bases = [[$.getenv("LOCALAPPDATA"), "\\Programs"], [$.getenv("ProgramFiles"), ""],
                [$.getenv("ProgramW6432"), ""]];
            for (var i = 0; i < bases.length; i++) {
                if (bases[i][0]) {
                    candidates.push(bases[i][0] + bases[i][1] + "\\VisiCutR\\visicutr.exe");
                }
            }
        } else {
            candidates.push("/Applications/VisiCutR.app", "~/Applications/VisiCutR.app");
        }
        for (var j = 0; j < candidates.length; j++) {
            if (candidates[j] && exists(trim(candidates[j]))) {
                return trim(candidates[j]);
            }
        }
        if (!confirm("VisiCutR wurde nicht gefunden. Startet man VisiCutR einmal, findet das " +
                "Skript es danach von selbst.\n\nVisiCutR jetzt selbst suchen?")) {
            return null;
        }
        var chosen = sep === "\\" ? File.openDialog("VisiCutR suchen", "visicutr.exe:visicutr.exe")
            : File.openDialog("VisiCutR suchen");
        if (!chosen) {
            return null;
        }
        writeText(configDir() + sep + PATH_FILE, chosen.fsName);
        return chosen.fsName;
    }

    // Uebergibt die Datei an die laufende Instanz; true, wenn sie "OK" sagt.
    function forward(svgPath) {
        var address = parseAddress(readText(configDir() + separator() + INSTANCE_FILE));
        if (!address) {
            return false;
        }
        var socket = new Socket();
        socket.timeout = 5;
        if (!socket.open("127.0.0.1:" + address.port)) {
            return false;
        }
        socket.write(requestLine(address.token, true, true, [svgPath]) + "\n");
        var reply = socket.readln();
        socket.close();
        return /^OK/.test(reply);
    }

    // Neuer Ordner im Temp-Verzeichnis; VisiCutR liest die Datei beim
    // Neuladen erneut, sie bleibt deshalb liegen.
    function temporaryFolder() {
        var folder = new Folder(Folder.temp.fsName + separator() + "visicutr-" +
            new Date().getTime() + "-" + Math.floor(Math.random() * 1000000));
        folder.create();
        return folder;
    }

    function svgOptions() {
        var options = new ExportOptionsSVG();
        options.embedRasterImages = true;
        options.fontType = SVGFontType.OUTLINEFONT;
        options.cssProperties = SVGCSSPropertyLocation.PRESENTATIONATTRIBUTES;
        options.documentEncoding = SVGDocumentEncoding.UTF8;
        options.DTD = SVGDTDVersion.SVG1_1;
        options.coordinatePrecision = 4;
        options.preserveEditability = false;
        return options;
    }

    // Umrissrechteck [links, oben, rechts, unten] mehrerer Objekte.
    function bounds(items) {
        var result = null;
        for (var i = 0; i < items.length; i++) {
            var b = items[i].geometricBounds;
            if (!result) {
                result = [b[0], b[1], b[2], b[3]];
            } else {
                result = [Math.min(result[0], b[0]), Math.max(result[1], b[1]),
                    Math.max(result[2], b[2]), Math.min(result[3], b[3])];
            }
        }
        return result;
    }

    // Ein Array von Objekten, falls etwas gewaehlt ist (nicht Text im
    // Bearbeitungsmodus).
    function selectedItems(doc) {
        var selection = doc.selection;
        if (!selection || selection.typename !== undefined || selection.length === 0) {
            return null;
        }
        return selection;
    }

    // Schreibt die Zeichnung oder die Auswahl als SVG nach `file`.
    function exportDrawing(doc, file) {
        var items = selectedItems(doc);
        if (!items) {
            doc.exportFile(file, ExportType.SVG, svgOptions());
            return;
        }
        // Auswahl in ein neues Dokument mit gleich grosser Zeichenflaeche
        // kopieren und dort an dieselbe Stelle schieben.
        var board = doc.artboards[doc.artboards.getActiveArtboardIndex()].artboardRect;
        var copy = app.documents.add(doc.documentColorSpace, board[2] - board[0], board[1] - board[3]);
        try {
            var layer = copy.layers[0];
            var copies = [];
            for (var i = 0; i < items.length; i++) {
                copies.push(items[i].duplicate(layer, ElementPlacement.PLACEATEND));
            }
            var target = copy.artboards[0].artboardRect;
            var from = bounds(items);
            var to = bounds(copies);
            var dx = (from[0] - board[0]) - (to[0] - target[0]);
            var dy = (from[1] - board[1]) - (to[1] - target[1]);
            for (var j = 0; j < copies.length; j++) {
                copies[j].translate(dx, dy);
            }
            copy.exportFile(file, ExportType.SVG, svgOptions());
        } finally {
            copy.close(SaveOptions.DONOTSAVECHANGES);
            doc.activate();
        }
    }

    function run() {
        if (app.documents.length === 0) {
            alert("Kein Dokument ge\u00f6ffnet.", "VisiCutR");
            return;
        }
        var doc = app.activeDocument;
        var program = findProgram();
        if (!program) {
            return;
        }
        var file = new File(temporaryFolder().fsName + separator() + documentName(doc.name) + ".svg");
        try {
            exportDrawing(doc, file);
        } catch (error) {
            alert("Export als SVG fehlgeschlagen: " + error, "VisiCutR", true);
            return;
        }
        if (forward(file.fsName)) {
            return;
        }
        if (!new File(launchTarget(program)).execute()) {
            alert("VisiCutR konnte nicht gestartet werden:\n" + program, "VisiCutR", true);
            return;
        }
        for (var waited = 0; waited < START_TIMEOUT_MS; waited += 500) {
            $.sleep(500);
            if (forward(file.fsName)) {
                return;
            }
        }
        alert("VisiCutR antwortet nicht. Die Zeichnung liegt hier:\n" + file.fsName, "VisiCutR", true);
    }

    return {
        jsonString: jsonString,
        requestLine: requestLine,
        parseAddress: parseAddress,
        documentName: documentName,
        launchTarget: launchTarget,
        run: run
    };
})();

if (typeof app !== "undefined" && typeof ExportOptionsSVG !== "undefined") {
    VisiCutR.run();
}
