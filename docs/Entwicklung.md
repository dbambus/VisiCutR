# Entwicklung

## Bauen und testen

Für einen Build aus dem Quellcode werden Rust 1.90 und die Xcode Command
Line Tools benötigt:

```sh
bash scripts/bundle-macos.sh
open 'dist/VisiCutR.app'
```

Das mitgelieferte `rust-toolchain.toml` wählt Rust 1.90.

```sh
cargo test --lib --locked
```

Das Bundle ist lokal ad hoc signiert. Mit
`MACOS_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"` signiert das
Skript mit einer Developer ID (Hardened Runtime); die CI notarisiert dann
auch, wenn die Secrets hinterlegt sind (siehe [macOS-Signierung](#macos-signierung)).

Zum Entwickeln startet `cargo run --release --bin visicutr` dieselbe
Oberfläche direkt.

## Aufbau

`src/lib.rs` ist der Rust-Kern, `src/main.rs` die egui-Oberfläche. Die
frühere native macOS-Oberfläche (SwiftUI) ist entfernt; das macOS-Bundle
enthält dieselbe egui-Oberfläche wie Windows und Linux.

`tests/java_parity.rs` vergleicht die erzeugten LTT-Aufträge Byte für Byte
mit Referenzen aus dem originalen Java-VisiCut; neu erzeugt werden sie mit
`scripts/java-parity/generate.sh` aus dem Java-Stand des Tags `java-reference`
(siehe [Java-Vergleich](Java-Vergleich.md)).

Die Materialdaten sind in `resources/materials.json` eingebettet, die
Profile der anderen FAU-Lasercutter in `resources/profiles/`.
Originaldateien liegen unter `reference/fau-settings`.
Der Import lässt sich mit `scripts/import-fau-materials.py` aus einem
lokalen Checkout der FAU-Einstellungen wiederholen. 0 mm bedeutet in
den FAU-Profilen „nicht festgelegt“ und wird entsprechend angezeigt.
Profile mit 0 % Leistung werden nicht als übernehmbare Presets angeboten.

Details und fehlende Funktionen: [LTT-Protokoll](LTT-Protokoll.md).
Lizenz: LGPL-3.0-or-later, siehe `../COPYING.LESSER`.

## GitHub Actions und Releases

Der Workflow **VisiCutR releases** baut drei native Ziele auf GitHub-gehosteten Runnern:

| Ziel | Oberfläche | Download-Artefakt |
| --- | --- | --- |
| Windows x64 / MSVC | egui | ZIP mit EXE |
| Linux x64 / Ubuntu 24.04 | egui, X11/Wayland | tar.gz mit ausführbarer Datei |
| macOS Apple Silicon | egui | ZIP mit `.app` |

Bei Pull Requests und Änderungen an `master`/`main` laufen Rust-Tests auf
allen drei Zielen. Linux prüft zusätzlich Formatierung und Clippy; das
Mac-Ziel baut und signiert das Bundle. TCP-Tests verwenden ausschließlich
localhost. Die Downloads liegen unter **Actions → VisiCutR releases →
Artifacts** (14 Tage Aufbewahrung). Ein Push auf `master` veröffentlicht
nach erfolgreichen Builds auf allen drei Zielen ein vollständiges
[GitHub Release](https://github.com/dbambus/VisiCutR/releases)
`v<Cargo-Version>` mit drei Archiven und `SHA256SUMS.txt`, sofern es für diese
Version noch keines gibt; die Beschreibung ist der passende Abschnitt aus
[CHANGELOG.md](../CHANGELOG.md). Weitere Pushes mit derselben Version bauen nur
Artefakte. Für ein neues Release Version in `Cargo.toml`/`Cargo.lock` erhöhen
und einen Changelog-Abschnitt `## <Version> – <Datum>` ergänzen. Tags
`v<Version>` veröffentlichen ebenfalls (Tag und Cargo-Version müssen
übereinstimmen); nur Versionen mit Suffix wie `-rc.1` werden als Vorabversion
markiert. Ein manueller Lauf auf einem Versionstag wiederholt die
Veröffentlichung, auf einem Branch baut er nur Artefakte.
Der macOS-Download heißt
`VisiCutR-<Version>-aarch64-apple-darwin.zip`.
Weitere Workflows gibt es nicht: Die alten Java-Distributionsbuilds und der
Auslöser für den externen VisiCut-Buildserver wurden entfernt.

Für lokale portable Builds:

```sh
cargo build --locked --release --bin visicutr
cargo test --locked --all-targets
```

`scripts/package-ci.py` erstellt dieselben Archive lokal nach einem Build für
die jeweilige Host-Architektur.

## macOS-Signierung

Ohne Secrets wird die App ad hoc signiert. Mit diesen Repository-Secrets
signiert die CI mit einer Developer ID (Hardened Runtime) und, wenn auch die
Apple-ID-Werte hinterlegt sind, notarisiert und heftet das Ticket an:

| Secret | Inhalt |
| --- | --- |
| `MACOS_CERTIFICATE` | Developer-ID-Application-Zertifikat mit privatem Schlüssel als `.p12`, base64-kodiert (`base64 -i cert.p12`) |
| `MACOS_CERTIFICATE_PASSWORD` | Kennwort der `.p12`-Datei |
| `MACOS_SIGN_IDENTITY` | z. B. `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | Apple-ID, Team-ID und ein app-spezifisches Kennwort für `notarytool` |

Lokal signiert `MACOS_SIGN_IDENTITY=… bash scripts/bundle-macos.sh` mit einem
Zertifikat aus dem Anmelde-Schlüsselbund.

## Übersetzungen

Übersetzungen: Der deutsche Text ist der Schlüssel (`tr("…")`, mit
Platzhaltern `tr!("… {name}", name = …)`); die Tabellen liegen in
`resources/i18n/<sprache>.json` und werden in den Rust-Kern einkompiliert.
Ein Test prüft, dass jeder Text aus `src/` in jeder Sprache mit denselben
Platzhaltern übersetzt ist und keine veralteten Einträge bleiben. `cargo test --lib i18n::tests::export_missing -- --ignored`
schreibt fehlende Texte nach `target/i18n/missing-<sprache>.json`.

[← Übersicht](README.md)
