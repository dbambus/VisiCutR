//! FabLab-Einstellungen wie VisiCuts `LabSettings`: ein `.vcsettings`- oder
//! GitHub-Archiv mit Lasercuttern (`devices/`), Materialien samt Laserwerten
//! (`materials/`, `laserprofiles/`) und Zuordnungen (`mappings/`). Ein
//! Fingerabdruck des Inhalts zeigt, ob das FabLab eine neue Fassung
//! veröffentlicht hat.

use crate::device::{self, LaserDevice};
use crate::mapping::Predefined;
use crate::materials::{Library, Material, MaterialProfile};
use crate::project::{Operation, ParameterSet};
use crate::tr;
use std::collections::BTreeMap;
use std::io::Cursor;

/// Größte Datei im Archiv, die gelesen wird.
const MAX_ENTRY: u64 = 5 * 1024 * 1024;
/// All entries together, unpacked.
const MAX_ARCHIVE: u64 = 100 * 1024 * 1024;
/// Laserprofil-Dateien und ihr Verfahren; bei mehreren gilt die erste
/// (wie `scripts/import-fau-materials.py`).
const OPERATIONS: [(&str, Operation); 5] = [
    ("cut", Operation::Cut),
    ("engrave", Operation::Engrave),
    ("mark", Operation::Mark),
    ("engrave_32_3d", Operation::Engrave3d),
    ("engrave3d", Operation::Engrave3d),
];

pub struct LabSettings {
    pub devices: Vec<LaserDevice>,
    /// Materialien des ersten Lasercutters, für den das Archiv Laserwerte hat.
    pub library: Option<Library>,
    pub templates: Vec<Predefined>,
    pub warnings: Vec<String>,
    /// Ändert sich, sobald sich Geräte, Materialien, Laserwerte oder
    /// Zuordnungen im Archiv ändern.
    pub fingerprint: String,
}

impl LabSettings {
    pub fn profile_count(&self) -> usize {
        self.library
            .as_ref()
            .map_or(0, |l| l.materials.iter().map(|m| m.profiles.len()).sum())
    }
}

/// Lädt die Einstellungen des FabLabs `name` von `url`.
pub fn download(name: &str, url: &str) -> Result<LabSettings, String> {
    read(&crate::camera::fetch(url, 50 * 1024 * 1024)?, name)
}

/// Liest ein Einstellungsarchiv oder eine einzelne Gerätedatei.
pub fn read(bytes: &[u8], name: &str) -> Result<LabSettings, String> {
    let devices = device::import_bytes(bytes)?;
    if !bytes.starts_with(b"PK") {
        return Ok(LabSettings {
            devices,
            library: None,
            templates: Vec::new(),
            warnings: Vec::new(),
            fingerprint: fingerprint([("", bytes)]),
        });
    }
    let entries = entries(bytes)?;
    let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
    let library = library(&entries, &names, name);
    let (templates, warnings) =
        crate::import::import_templates(bytes, "lab.vcsettings").unwrap_or_default();
    Ok(LabSettings {
        devices,
        library,
        templates,
        warnings,
        fingerprint: fingerprint(entries.iter().map(|(p, c)| (p.as_str(), c.as_slice()))),
    })
}

/// Dateien der Einstellungsordner, Pfad ab dem Ordner (ohne den Ordner, den
/// GitHub davorsetzt), sortiert.
fn entries(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut result = BTreeMap::new();
    let mut budget = crate::import::ZipBudget::new(MAX_ARCHIVE);
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        if !file.is_file() || file.size() > MAX_ENTRY {
            continue;
        }
        let name = file.name().map_err(|e| e.to_string())?.to_string();
        let parts: Vec<&str> = name.split('/').collect();
        let Some(start) = parts
            .iter()
            .position(|p| matches!(*p, "devices" | "materials" | "laserprofiles" | "mappings"))
        else {
            continue;
        };
        let Some(content) = budget.read(&mut file, MAX_ENTRY)? else {
            continue;
        };
        result.insert(parts[start..].join("/"), content);
    }
    Ok(result)
}

/// FNV-1a über Pfade und Inhalte: über Programmversionen hinweg gleich.
fn fingerprint<'a>(entries: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (path, content) in entries {
        for byte in path
            .bytes()
            .chain([0])
            .chain(content.iter().copied())
            .chain([0])
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// VisiCut schreibt Sonderzeichen in Dateinamen als `_<Zeichencode>_`.
pub fn decode_name(name: &str) -> String {
    let mut result = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('_') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let decoded = (digits > 0 && after[digits..].starts_with('_'))
            .then(|| after[..digits].parse::<u32>().ok())
            .flatten()
            .filter(|code| *code >= 32)
            .and_then(char::from_u32);
        match decoded {
            Some(c) => {
                result.push(c);
                rest = &after[digits + 1..];
            }
            None => {
                result.push('_');
                rest = after;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Stärke → Verfahren → (Rang der Profildatei, Profil).
type Thicknesses = BTreeMap<String, BTreeMap<String, (usize, MaterialProfile)>>;

/// Materialien mit einfachen Laserwerten (ein Parametersatz) für einen der
/// Lasercutter `devices`, sonst für den einzigen im Archiv.
fn library(entries: &BTreeMap<String, Vec<u8>>, devices: &[&str], lab: &str) -> Option<Library> {
    let mut folders: Vec<&str> = entries
        .keys()
        .filter_map(|p| p.strip_prefix("laserprofiles/")?.split('/').next())
        .collect();
    folders.dedup();
    let folder = folders
        .iter()
        .find(|f| {
            let name = decode_name(f);
            devices.iter().any(|d| d.eq_ignore_ascii_case(&name))
        })
        .or_else(|| (folders.len() == 1).then(|| &folders[0]))?;
    let prefix = format!("laserprofiles/{folder}/");
    let mut found: BTreeMap<&str, Thicknesses> = BTreeMap::new();
    for (path, content) in entries.range(prefix.clone()..) {
        let Some(rest) = path.strip_prefix(&prefix) else {
            break;
        };
        let [material, thickness, file] = rest.split('/').collect::<Vec<_>>()[..] else {
            continue;
        };
        let Some(thickness_mm) = thickness
            .strip_suffix("mm")
            .and_then(|t| t.parse::<f32>().ok())
            .filter(|t| t.is_finite() && *t >= 0.0)
        else {
            continue;
        };
        let stem = file.strip_suffix(".xml").unwrap_or(file);
        let Some(rank) = OPERATIONS.iter().position(|(name, _)| *name == stem) else {
            continue;
        };
        let operation = OPERATIONS[rank].1;
        let Some(mut sets) = settings(content) else {
            continue;
        };
        let first = sets.remove(0);
        let slot = found
            .entry(material)
            .or_default()
            .entry(thickness.to_string())
            .or_default();
        let key = format!("{operation:?}");
        if slot.get(&key).is_none_or(|(old, _)| rank < *old) {
            slot.insert(
                key,
                (
                    rank,
                    MaterialProfile {
                        thickness_mm,
                        operation,
                        power_percent: first.power_percent,
                        speed_percent: first.speed_percent,
                        passes: first.passes,
                        additional: sets,
                        source: Some(path.clone()),
                    },
                ),
            );
        }
    }
    let mut materials: Vec<Material> = found
        .into_iter()
        .map(|(id, thicknesses)| {
            let mut profiles: Vec<MaterialProfile> = thicknesses
                .into_values()
                .flat_map(|ops| ops.into_values().map(|(_, p)| p))
                .collect();
            profiles.sort_by(|a, b| {
                a.thickness_mm
                    .total_cmp(&b.thickness_mm)
                    .then(operation_rank(a.operation).cmp(&operation_rank(b.operation)))
            });
            let name = entries
                .get(&format!("materials/{id}.xml"))
                .and_then(|xml| material_name(xml))
                .unwrap_or_else(|| decode_name(id));
            Material {
                id: id.to_string(),
                name,
                profiles,
            }
        })
        .collect();
    materials.sort_by_key(|m| m.name.to_lowercase());
    let library = Library {
        source: lab.to_string(),
        device: decode_name(folder),
        materials,
    };
    (!library.materials.is_empty() && library.validate().is_ok()).then_some(library)
}

fn operation_rank(operation: Operation) -> usize {
    [
        Operation::Cut,
        Operation::Engrave,
        Operation::Mark,
        Operation::Engrave3d,
    ]
    .iter()
    .position(|o| *o == operation)
    .unwrap_or(usize::MAX)
}

/// Höchstens so viele Parametersätze wie ein Schritt aufnimmt.
const MAX_SETS: usize = 16;

/// Parametersätze eines Laserprofils der Reihe nach; gleiche Sätze
/// hintereinander werden zu Durchgängen zusammengefasst.
fn settings(xml: &[u8]) -> Option<Vec<ParameterSet>> {
    let text = std::str::from_utf8(xml).ok()?;
    crate::svg::check_nesting(text).ok()?;
    let document = roxmltree::Document::parse(text).ok()?;
    let mut result: Vec<ParameterSet> = Vec::new();
    for set in document
        .root_element()
        .children()
        .filter(|n| n.is_element())
    {
        let value = |name: &str| -> Option<f32> {
            set.children()
                .find(|n| n.has_tag_name(name))?
                .text()?
                .trim()
                .parse()
                .ok()
                .filter(|v: &f32| v.is_finite())
        };
        let (power_percent, speed_percent) = (value("power")?, value("speed")?);
        match result.last_mut() {
            Some(last)
                if last.power_percent == power_percent && last.speed_percent == speed_percent =>
            {
                last.passes += 1;
            }
            _ => result.push(ParameterSet {
                power_percent,
                speed_percent,
                passes: 1,
            }),
        }
    }
    (!result.is_empty() && result.len() <= MAX_SETS).then_some(result)
}

fn material_name(xml: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(xml).ok()?;
    crate::svg::check_nesting(text).ok()?;
    let document = roxmltree::Document::parse(text).ok()?;
    let name = document
        .root_element()
        .children()
        .find(|n| n.has_tag_name("name"))?
        .text()?
        .trim();
    (!name.is_empty()).then(|| decode_name(name))
}

/// Kurzbeschreibung für Dialog und Statuszeile.
pub fn summary(settings: &LabSettings) -> String {
    let mut parts = vec![tr!("{count} Lasercutter", count = settings.devices.len())];
    if let Some(library) = &settings.library {
        parts.push(tr!(
            "{materials} Materialien mit {profiles} Laserwerten",
            materials = library.materials.len(),
            profiles = settings.profile_count()
        ));
    }
    if !settings.templates.is_empty() {
        parts.push(tr!("{count} Zuordnungen", count = settings.templates.len()));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            for (name, content) in files {
                zip.start_file(format!("visicut-settings-master/{name}"), options)
                    .unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        bytes
    }

    const DEVICE: &str = include_str!("../reference/FAU-LTT-iLaser-4000.xml");

    fn profile(power: f32, speed: f32) -> Vec<u8> {
        format!("<linked-list><LaosCutterProperty><power>{power}</power><speed>{speed}</speed></LaosCutterProperty></linked-list>").into_bytes()
    }

    #[test]
    fn decodes_visicut_file_names() {
        assert_eq!(decode_name("LTT_32_iLaser_32_4000"), "LTT iLaser 4000");
        assert_eq!(decode_name("Holz_45_Platte_x"), "Holz-Platte_x");
        assert_eq!(decode_name("a_5_b"), "a_5_b");
    }

    #[test]
    fn reads_devices_materials_and_mappings_like_visicut() {
        let device = device::from_visicut_xml(DEVICE).unwrap();
        let folder = format!("laserprofiles/{}", device.name.replace(' ', "_32_"));
        let two_sets = b"<linked-list><a><power>1</power><speed>1</speed></a><a><power>1</power><speed>1</speed></a><a><power>2</power><speed>2</speed></a></linked-list>";
        let files: Vec<(String, Vec<u8>)> = vec![
            ("devices/LTT.xml".into(), DEVICE.as_bytes().to_vec()),
            (
                "materials/Holz_45_Platte.xml".into(),
                b"<material><name>Holzplatte</name></material>".to_vec(),
            ),
            (
                format!("{folder}/Holz_45_Platte/3.0mm/cut.xml"),
                profile(80.0, 20.0),
            ),
            (
                format!("{folder}/Holz_45_Platte/3.0mm/engrave3d.xml"),
                profile(30.0, 50.0),
            ),
            (
                format!("{folder}/Holz_45_Platte/3.0mm/engrave_32_3d.xml"),
                profile(40.0, 60.0),
            ),
            (
                format!("{folder}/Holz_45_Platte/1.5mm/mark.xml"),
                profile(10.0, 100.0),
            ),
            (
                format!("{folder}/Holz_45_Platte/1.5mm/engrave.xml"),
                two_sets.to_vec(),
            ),
            (format!("{folder}/Acryl/2.0mm/cut.xml"), profile(100.0, 9.0)),
            (
                "laserprofiles/Epilog/Acryl/2.0mm/cut.xml".into(),
                profile(1.0, 1.0),
            ),
        ];
        let refs: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_slice()))
            .collect();
        let lab = read(&archive(&refs), "Testlabor").unwrap();
        assert_eq!(lab.devices.len(), 1);
        let library = lab.library.as_ref().unwrap();
        assert_eq!(
            (library.source.as_str(), library.device.as_str()),
            ("Testlabor", device.name.as_str())
        );
        let names: Vec<&str> = library.materials.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Acryl", "Holzplatte"]);
        let holz = &library.materials[1];
        let summary: Vec<_> = holz
            .profiles
            .iter()
            .map(|p| {
                (
                    p.thickness_mm,
                    p.operation,
                    p.power_percent,
                    p.speed_percent,
                )
            })
            .collect();
        // „engrave_32_3d“ geht vor „engrave3d“.
        assert_eq!(
            summary,
            [
                (1.5, Operation::Engrave, 1.0, 1.0),
                (1.5, Operation::Mark, 10.0, 100.0),
                (3.0, Operation::Cut, 80.0, 20.0),
                (3.0, Operation::Engrave3d, 40.0, 60.0),
            ]
        );
        // Zwei gleiche Sätze ergeben zwei Durchgänge, dann folgt der dritte.
        let engrave = &holz.profiles[0];
        assert_eq!(engrave.passes, 2);
        assert_eq!(
            engrave.additional,
            [ParameterSet {
                power_percent: 2.0,
                speed_percent: 2.0,
                passes: 1
            }]
        );
        assert_eq!(lab.profile_count(), 5);
        // Gleicher Inhalt, gleicher Fingerabdruck; geänderte Werte, neuer.
        let again = read(&archive(&refs), "Testlabor").unwrap();
        assert_eq!(again.fingerprint, lab.fingerprint);
        let mut changed = files.clone();
        changed[7].1 = profile(100.0, 8.0);
        let refs: Vec<(&str, &[u8])> = changed
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_slice()))
            .collect();
        assert_ne!(
            read(&archive(&refs), "Testlabor").unwrap().fingerprint,
            lab.fingerprint
        );
        assert!(summary_text_mentions_counts(&lab));
    }

    fn summary_text_mentions_counts(lab: &LabSettings) -> bool {
        let text = summary(lab);
        text.contains('1') && text.contains('2') && text.contains('5')
    }

    /// Needs internet access: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_fau_materials() {
        let (name, url) = device::LAB_SETTINGS
            .iter()
            .find(|l| l.0.contains("FAU"))
            .unwrap();
        let lab = download(name, url).unwrap();
        let library = lab.library.unwrap();
        assert!(
            library.materials.iter().any(|m| m.name == "Acryl"),
            "{library:?}"
        );
        assert!(library.materials.len() > 10);
    }
}
