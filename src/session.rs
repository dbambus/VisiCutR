//! Zwischenspeicher für ungesicherte Projekte. Statt beim Öffnen, bei „Neu“
//! oder beim Beenden nach dem Sichern zu fragen, legen beide Oberflächen den
//! ungesicherten Stand hier ab und führen ihn unter „Zuletzt geöffnet“. Der
//! beim Beenden abgelegte Stand öffnet beim nächsten Start wieder. An einen
//! selbst gewählten Ort kommt ein Projekt erst über „Sichern“; danach wird
//! sein Zwischenstand gelöscht.

use crate::{i18n::tr, project::Project, tr};
use std::path::{Path, PathBuf};

/// Höchstens so viele Zwischenstände; ältere werden gelöscht.
pub const LIMIT: usize = 10;
/// Merkt sich den beim Beenden abgelegten Stand für den nächsten Start.
const RESUME: &str = "resume.txt";

/// Ordner der Zwischenstände im Einstellungsordner.
pub fn dir() -> PathBuf {
    crate::device::config_dir().join("cache")
}

/// Ob `path` ein Zwischenstand in `dir` ist. Solche Projekte haben noch
/// keinen eigenen Speicherort.
pub fn is_cached(dir: &Path, path: &Path) -> bool {
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_owned());
    path.parent()
        .is_some_and(|parent| canonical(parent) == canonical(dir))
}

/// Legt `project` ab und liefert den Pfad. Ein gleicher, schon abgelegter
/// Stand wird wiederverwendet; `previous` (der Zwischenstand, aus dem das
/// Projekt geöffnet wurde) wird überschrieben, sonst entsteht eine neue Datei.
pub fn stash(dir: &Path, project: &Project, previous: Option<&Path>) -> Result<PathBuf, String> {
    let bytes = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = match states(dir)
        .into_iter()
        .find(|p| std::fs::read(p).is_ok_and(|b| b == bytes))
    {
        Some(same) => same,
        None => match previous.filter(|p| is_cached(dir, p)) {
            Some(previous) => previous.to_owned(),
            None => free_name(dir, &project.name),
        },
    };
    // Auch ein unveränderter Stand wird neu geschrieben und damit der neueste.
    write(&path, &bytes)?;
    for old in states(dir).into_iter().skip(LIMIT) {
        let _ = std::fs::remove_file(old);
    }
    Ok(path)
}

/// Löscht einen Zwischenstand, nachdem das Projekt gesichert wurde.
pub fn discard(dir: &Path, path: &Path) {
    if is_cached(dir, path) {
        let _ = std::fs::remove_file(path);
    }
}

/// Löscht alle Zwischenstände und den Merker für den nächsten Start und
/// liefert die gelöschten Pfade (für „Zuletzt geöffnet“).
pub fn clear(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let removed = states(dir);
    for path in &removed {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    set_resume(dir, None)?;
    Ok(removed)
}

/// Merkt sich `path` für den nächsten Start (`None` vergisst ihn).
pub fn set_resume(dir: &Path, path: Option<&Path>) -> Result<(), String> {
    let file = dir.join(RESUME);
    match path {
        Some(path) => {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            write(&file, path.to_string_lossy().as_bytes())
        }
        None => match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
    }
}

/// Der beim Beenden abgelegte Stand, einmalig: danach ist er vergessen.
pub fn take_resume(dir: &Path) -> Option<PathBuf> {
    let file = dir.join(RESUME);
    let path = PathBuf::from(std::fs::read_to_string(&file).ok()?.trim());
    let _ = std::fs::remove_file(file);
    (is_cached(dir, &path) && path.is_file()).then_some(path)
}

/// Alle Zwischenstände, neueste zuerst.
pub fn states(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "vcr"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().map(|(_, p)| p).collect()
}

/// „Name (ungesichert).vcr“, bei Bedarf mit Nummer; der Name erscheint so
/// unter „Zuletzt geöffnet“.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect();
    let name = name.trim().trim_start_matches('.');
    let name = if name.is_empty() {
        tr("Neues Projekt")
    } else {
        name
    };
    (1..)
        .map(|n| {
            let stem = if n == 1 {
                tr!("{name} (ungesichert)", name = name)
            } else {
                tr!("{name} (ungesichert {number})", name = name, number = n)
            };
            dir.join(format!("{stem}.vcr"))
        })
        .find(|p| !p.exists())
        .expect("unendlich viele Namen")
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("visicut-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn stashes_reuses_equal_states_and_overwrites_the_opened_one() {
        let dir = temporary("stash");
        let mut project = Project {
            name: "Schild: Tür/Tor".into(),
            ..Project::default()
        };
        let first = stash(&dir, &project, None).unwrap();
        assert_eq!(
            first.file_name().unwrap(),
            "Schild_ Tür_Tor (ungesichert).vcr"
        );
        assert!(is_cached(&dir, &first));
        assert!(!is_cached(&dir, &std::env::temp_dir().join("x.vcr")));
        let loaded: Project = serde_json::from_slice(&std::fs::read(&first).unwrap()).unwrap();
        assert_eq!(loaded.name, project.name);
        // Unverändert: dieselbe Datei.
        assert_eq!(stash(&dir, &project, None).unwrap(), first);
        // Geändert, aus keinem Zwischenstand geöffnet: neue Datei.
        project.x_mm += 1.0;
        let second = stash(&dir, &project, None).unwrap();
        assert_eq!(
            second.file_name().unwrap(),
            "Schild_ Tür_Tor (ungesichert 2).vcr"
        );
        // Aus dem ersten geöffnet und geändert: der erste wird überschrieben.
        project.x_mm += 1.0;
        assert_eq!(stash(&dir, &project, Some(&first)).unwrap(), first);
        // Ein Pfad außerhalb des Zwischenspeichers wird nie überschrieben.
        let outside =
            std::env::temp_dir().join(format!("visicut-outside-{}.vcr", std::process::id()));
        project.x_mm += 1.0;
        assert_ne!(stash(&dir, &project, Some(&outside)).unwrap(), outside);
        assert!(!outside.exists());
        discard(&dir, &first);
        assert!(!first.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_only_the_newest_states() {
        let dir = temporary("limit");
        let mut project = Project::default();
        for i in 0..LIMIT + 2 {
            project.x_mm = i as f32;
            stash(&dir, &project, None).unwrap();
            // Unterscheidbare Änderungszeiten auch bei grober Auflösung.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Die zwei ältesten Stände (x = 0 und 1) sind gelöscht; freie Namen
        // werden wiederverwendet.
        let mut kept: Vec<f32> = states(&dir)
            .iter()
            .map(|p| {
                serde_json::from_slice::<Project>(&std::fs::read(p).unwrap())
                    .unwrap()
                    .x_mm
            })
            .collect();
        kept.sort_by(f32::total_cmp);
        assert_eq!(kept, (2..LIMIT + 2).map(|i| i as f32).collect::<Vec<_>>());
        let removed = clear(&dir).unwrap();
        assert_eq!(removed.len(), LIMIT);
        assert!(states(&dir).is_empty());
        assert!(clear(&dir).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resume_opens_once_and_only_from_the_cache() {
        let dir = temporary("resume");
        assert_eq!(take_resume(&dir), None);
        let path = stash(&dir, &Project::default(), None).unwrap();
        set_resume(&dir, Some(&path)).unwrap();
        assert_eq!(take_resume(&dir), Some(path.clone()));
        assert_eq!(take_resume(&dir), None);
        set_resume(&dir, Some(&std::env::temp_dir().join("fremd.vcr"))).unwrap();
        assert_eq!(take_resume(&dir), None);
        set_resume(&dir, Some(&path)).unwrap();
        set_resume(&dir, None).unwrap();
        assert_eq!(take_resume(&dir), None);
        // Leeren vergisst auch den Merker.
        set_resume(&dir, Some(&path)).unwrap();
        clear(&dir).unwrap();
        assert_eq!(take_resume(&dir), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
