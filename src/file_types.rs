//! Windows: Dateitypen für die portable EXE zuordnen (Einstellungen →
//! Allgemein). Alles steht im Benutzerzweig `HKCU\Software\Classes`, braucht
//! also keine Administratorrechte und lässt sich vollständig entfernen.
//! VisiCutR-Projekte öffnen per Doppelklick; PLF und LaserScript nur, wenn
//! noch kein anderes Programm (etwa Java-VisiCut) sie öffnet; Grafiken
//! erscheinen nur unter „Öffnen mit“, ihr Standardprogramm bleibt.

use std::path::Path;

/// Programm-ID für VisiCutR-Projekte.
const PROJECT: &str = "VisiCutR.Project";
/// Programm-ID für alle importierten Formate.
const IMPORT: &str = "VisiCutR.File";
/// Schlüssel für „Öffnen mit“, unabhängig vom Ort der EXE.
const APPLICATION: &str = "Applications\\visicutr.exe";
/// Formate, die VisiCutR öffnet, wenn kein anderes Programm zugeordnet ist.
const OWN: [&str; 2] = [".plf", ".ls"];
/// Grafiken, die nur unter „Öffnen mit“ erscheinen.
const GRAPHICS: [&str; 4] = [".svg", ".pdf", ".dxf", ".eps"];

/// Ein Zeichenkettenwert unter `HKCU\Software\Classes`; `name` leer ist der
/// Standardwert des Schlüssels.
#[derive(Debug, PartialEq, Eq)]
struct Value {
    key: String,
    name: &'static str,
    data: String,
}

fn value(key: impl Into<String>, name: &'static str, data: impl Into<String>) -> Value {
    Value {
        key: key.into(),
        name,
        data: data.into(),
    }
}

fn command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// Werte, die immer geschrieben werden.
fn values(exe: &Path) -> Vec<Value> {
    let icon = format!("\"{}\",0", exe.display());
    let mut values = Vec::new();
    for (id, name) in [(PROJECT, "VisiCutR-Projekt"), (IMPORT, "VisiCutR-Import")] {
        values.push(value(id, "", name));
        values.push(value(format!("{id}\\DefaultIcon"), "", &icon));
        values.push(value(
            format!("{id}\\shell\\open\\command"),
            "",
            command(exe),
        ));
    }
    values.push(value(".vcr", "", PROJECT));
    values.push(value(".vcr\\OpenWithProgids", PROJECT, ""));
    for extension in OWN.iter().chain(&GRAPHICS) {
        values.push(value(format!("{extension}\\OpenWithProgids"), IMPORT, ""));
    }
    values.push(value(APPLICATION, "FriendlyAppName", "VisiCutR"));
    values.push(value(
        format!("{APPLICATION}\\shell\\open\\command"),
        "",
        command(exe),
    ));
    for extension in [".vcr"].iter().chain(&OWN).chain(&GRAPHICS) {
        values.push(value(
            format!("{APPLICATION}\\SupportedTypes"),
            extension,
            "",
        ));
    }
    values
}

/// Zuordnung zu `exe`; `.plf` und `.ls` nur, wenn `owner` (die bisherige
/// Programm-ID) leer ist oder schon VisiCutR gehört.
#[cfg_attr(not(windows), allow(dead_code))]
fn plan(exe: &Path, owner: impl Fn(&str) -> Option<String>) -> Vec<Value> {
    let mut values = values(exe);
    for extension in OWN {
        if owner(extension).is_none_or(|id| id.is_empty() || id == IMPORT) {
            values.push(value(extension, "", IMPORT));
        }
    }
    values
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Status {
    /// Zugeordnet zu dieser EXE.
    Registered,
    /// Zugeordnet zu einer EXE an einem anderen Ort.
    Elsewhere,
    Missing,
}

#[cfg(windows)]
mod registry {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW,
        RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
    };
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text).encode_wide().chain([0]).collect()
    }

    fn classes(key: &str) -> Vec<u16> {
        wide(&format!("Software\\Classes\\{key}"))
    }

    fn get(root: HKEY, key: &[u16], name: &str) -> Option<String> {
        let name = wide(name);
        let mut buffer = vec![0u16; 1024];
        let mut bytes = (buffer.len() * 2) as u32;
        let result = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        (result == ERROR_SUCCESS).then(|| {
            let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..length])
        })
    }

    fn check(result: u32, key: &str) -> Result<(), String> {
        if result == ERROR_SUCCESS || result == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!(
                "HKCU\\Software\\Classes\\{key}: {}",
                std::io::Error::from_raw_os_error(result as i32)
            ))
        }
    }

    fn notify() {
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    pub fn status(exe: &Path) -> Status {
        let key = classes(&format!("{PROJECT}\\shell\\open\\command"));
        match get(HKEY_CURRENT_USER, &key, "") {
            Some(found) if found.eq_ignore_ascii_case(&command(exe)) => Status::Registered,
            Some(_) => Status::Elsewhere,
            None => Status::Missing,
        }
    }

    pub fn register(exe: &Path) -> Result<(), String> {
        // Gesamtsicht aus Benutzer- und Systemzweig.
        let owner = |extension: &str| get(HKEY_CLASSES_ROOT, &wide(extension), "");
        for entry in plan(exe, owner) {
            let data = wide(&entry.data);
            let result = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    classes(&entry.key).as_ptr(),
                    wide(entry.name).as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            check(result, &entry.key)?;
        }
        notify();
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        for key in [PROJECT, IMPORT, APPLICATION] {
            check(
                unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, classes(key).as_ptr()) },
                key,
            )?;
        }
        for (extension, id) in [(".vcr", PROJECT)]
            .into_iter()
            .chain(OWN.iter().chain(&GRAPHICS).map(|e| (*e, IMPORT)))
        {
            let list = format!("{extension}\\OpenWithProgids");
            check(
                unsafe {
                    RegDeleteKeyValueW(
                        HKEY_CURRENT_USER,
                        classes(&list).as_ptr(),
                        wide(id).as_ptr(),
                    )
                },
                &list,
            )?;
            // Den Standardwert nur löschen, wenn er auf VisiCutR zeigt.
            if get(HKEY_CURRENT_USER, &classes(extension), "").as_deref() == Some(id) {
                check(
                    unsafe {
                        RegDeleteKeyValueW(
                            HKEY_CURRENT_USER,
                            classes(extension).as_ptr(),
                            std::ptr::null(),
                        )
                    },
                    extension,
                )?;
            }
        }
        notify();
        Ok(())
    }
}

#[cfg(windows)]
pub use registry::{register, status, unregister};

#[cfg(not(windows))]
pub fn status(_exe: &Path) -> Status {
    Status::Missing
}

#[cfg(not(windows))]
pub fn register(_exe: &Path) -> Result<(), String> {
    Err("Dateizuordnungen gibt es hier nur unter Windows".into())
}

#[cfg(not(windows))]
pub fn unregister() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(values: &'a [Value], key: &str, name: &str) -> Option<&'a str> {
        values
            .iter()
            .find(|v| v.key == key && v.name == name)
            .map(|v| v.data.as_str())
    }

    #[test]
    fn projects_open_with_the_exe_and_graphics_only_offer_it() {
        let exe = Path::new("C:\\Programme\\VisiCutR 0.19\\visicutr.exe");
        let values = plan(exe, |_| None);
        let command = "\"C:\\Programme\\VisiCutR 0.19\\visicutr.exe\" \"%1\"";
        assert_eq!(
            find(&values, "VisiCutR.Project\\shell\\open\\command", ""),
            Some(command)
        );
        assert_eq!(
            find(
                &values,
                "Applications\\visicutr.exe\\shell\\open\\command",
                ""
            ),
            Some(command)
        );
        assert_eq!(find(&values, ".vcr", ""), Some("VisiCutR.Project"));
        assert_eq!(find(&values, ".plf", ""), Some("VisiCutR.File"));
        // Grafiken behalten ihr Standardprogramm.
        assert_eq!(find(&values, ".svg", ""), None);
        assert_eq!(
            find(&values, ".svg\\OpenWithProgids", "VisiCutR.File"),
            Some("")
        );
        assert_eq!(
            find(
                &values,
                "Applications\\visicutr.exe\\SupportedTypes",
                ".dxf"
            ),
            Some("")
        );
    }

    #[test]
    fn existing_visicut_keeps_its_formats() {
        let exe = Path::new("D:\\visicutr.exe");
        let owner = |extension: &str| (extension == ".plf").then(|| "VisiCut.plf".to_owned());
        let values = plan(exe, owner);
        assert_eq!(find(&values, ".plf", ""), None);
        assert_eq!(
            find(&values, ".plf\\OpenWithProgids", "VisiCutR.File"),
            Some("")
        );
        assert_eq!(find(&values, ".ls", ""), Some("VisiCutR.File"));
        // Erneutes Zuordnen nach dem Verschieben der EXE.
        let again = plan(exe, |_| Some("VisiCutR.File".to_owned()));
        assert_eq!(find(&again, ".plf", ""), Some("VisiCutR.File"));
    }
}
