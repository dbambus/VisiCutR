//! Editable material library (VisiCut's material and laser profile
//! editors). The bundled FAU catalogue is used until the user saves own
//! changes to the settings directory.
use crate::project::{Operation, ParameterSet};
use crate::{i18n::tr, tr};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum MaterialError {
    Io(std::io::Error),
    Json(serde_json::Error),
    /// The library file cannot be read as a library.
    Damaged {
        path: PathBuf,
        error: serde_json::Error,
    },
    TooMany,
    NameMissing,
    Duplicate(String),
    /// `label` names material and thickness of the profile.
    Thickness {
        label: String,
    },
    Power {
        label: String,
    },
    Speed {
        label: String,
    },
    DuplicateProfile {
        label: String,
    },
}

impl fmt::Display for MaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Io(error) => error.to_string(),
            Self::Json(error) => error.to_string(),
            Self::Damaged { path, error } => tr!(
                "Materialbibliothek {path} ist beschädigt: {error}",
                path = path.display(),
                error = error
            ),
            Self::TooMany => tr("Zu viele Materialien").to_string(),
            Self::NameMissing => tr("Materialname fehlt").to_string(),
            Self::Duplicate(name) => {
                tr!("Material „{name}“ ist doppelt vorhanden", name = name)
            }
            Self::Thickness { label } => tr!(
                "{label}: Stärke muss zwischen 0 und 1000 mm liegen",
                label = label
            ),
            Self::Power { label } => tr!(
                "{label}: Leistung muss zwischen 0 und 100 % liegen",
                label = label
            ),
            Self::Speed { label } => tr!(
                "{label}: Geschwindigkeit muss zwischen 0 und 100 % liegen",
                label = label
            ),
            Self::DuplicateProfile { label } => tr!(
                "{label}: Profil für dieses Verfahren doppelt vorhanden",
                label = label
            ),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for MaterialError {}

impl From<std::io::Error> for MaterialError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for MaterialError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<MaterialError> for String {
    fn from(error: MaterialError) -> Self {
        error.to_string()
    }
}

const FILE: &str = "materials.json";
/// `source` der mitgelieferten FAU-Bibliothek.
pub const BUNDLED_SOURCE: &str = "fau-fablab/visicut-settings";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialProfile {
    pub thickness_mm: f32,
    pub operation: Operation,
    pub power_percent: f32,
    pub speed_percent: f32,
    /// Passes of the first parameter set (identical sets in a row in VisiCut).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub passes: u32,
    /// Further parameter sets run after the first (several entries in a
    /// VisiCut laser profile).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional: Vec<ParameterSet>,
    /// Origin of imported profiles, e.g. a FAU settings file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn one() -> u32 {
    1
}

fn is_one(value: &u32) -> bool {
    *value == 1
}

impl MaterialProfile {
    /// Number of parameter sets (the first and the additional ones).
    pub fn set_count(&self) -> usize {
        1 + self.additional.len()
    }

    /// Several passes or parameter sets.
    pub fn is_multi(&self) -> bool {
        self.passes > 1 || !self.additional.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub id: String,
    pub name: String,
    pub profiles: Vec<MaterialProfile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub source: String,
    pub device: String,
    pub materials: Vec<Material>,
}

impl Library {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../resources/materials.json"))
            .expect("bundled material catalogue must parse")
    }

    /// The user's library, or the bundled one; `true` if it is customised.
    pub fn load(dir: &Path) -> Result<(Self, bool), MaterialError> {
        let path = dir.join(FILE);
        if !path.exists() {
            return Ok((Self::bundled(), false));
        }
        let source = std::fs::read_to_string(&path)?;
        let library: Self = serde_json::from_str(&source)
            .map_err(|error| MaterialError::Damaged { path, error })?;
        library.validate()?;
        Ok((library, true))
    }

    pub fn save(&self, dir: &Path) -> Result<(), MaterialError> {
        self.validate()?;
        std::fs::create_dir_all(dir)?;
        let temporary = dir.join(format!("{FILE}.tmp"));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&temporary, json)?;
        Ok(std::fs::rename(&temporary, dir.join(FILE))?)
    }

    /// Back to the bundled FAU catalogue.
    pub fn reset(dir: &Path) -> Result<Self, MaterialError> {
        let path = dir.join(FILE);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(Self::bundled())
    }

    pub fn validate(&self) -> Result<(), MaterialError> {
        if self.materials.len() > 10_000 {
            return Err(MaterialError::TooMany);
        }
        for (i, material) in self.materials.iter().enumerate() {
            if material.name.trim().is_empty() {
                return Err(MaterialError::NameMissing);
            }
            if self.materials[..i].iter().any(|m| m.name == material.name) {
                return Err(MaterialError::Duplicate(material.name.clone()));
            }
            for (j, p) in material.profiles.iter().enumerate() {
                let label = format!("{}, {} mm", material.name, p.thickness_mm);
                if !p.thickness_mm.is_finite() || !(0.0..=1000.0).contains(&p.thickness_mm) {
                    return Err(MaterialError::Thickness { label });
                }
                if !p.power_percent.is_finite() || !(0.0..=100.0).contains(&p.power_percent) {
                    return Err(MaterialError::Power { label });
                }
                if !p.speed_percent.is_finite() || !(0.0..=100.0).contains(&p.speed_percent) {
                    return Err(MaterialError::Speed { label });
                }
                if material.profiles[..j].iter().any(|q| {
                    q.operation == p.operation && (q.thickness_mm - p.thickness_mm).abs() < 1e-4
                }) {
                    return Err(MaterialError::DuplicateProfile { label });
                }
            }
        }
        Ok(())
    }

    /// Adds or replaces materials by name.
    pub fn merge(&mut self, other: Library) -> usize {
        let count = other.materials.len();
        for material in other.materials {
            match self.materials.iter_mut().find(|m| m.name == material.name) {
                Some(existing) => *existing = material,
                None => self.materials.push(material),
            }
        }
        self.materials.sort_by_key(|m| m.name.to_lowercase());
        count
    }

    pub fn import(bytes: &[u8]) -> Result<Self, MaterialError> {
        let library: Self = serde_json::from_slice(bytes)?;
        library.validate()?;
        Ok(library)
    }

    pub fn export(&self, path: &Path) -> Result<(), MaterialError> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self)?;
        Ok(std::fs::write(path, json)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_source_is_known() {
        assert_eq!(Library::bundled().source, BUNDLED_SOURCE);
    }

    #[test]
    fn bundled_catalogue_includes_mark_and_3d_profiles() {
        let library = Library::bundled();
        library.validate().unwrap();
        let profiles = library.materials.iter().flat_map(|m| &m.profiles);
        assert!(profiles.clone().any(|p| p.operation == Operation::Mark));
        assert!(
            profiles
                .clone()
                .any(|p| p.operation == Operation::Engrave3d)
        );
        assert!(profiles.count() >= 115);
    }

    #[test]
    fn save_load_reset_merge_and_validation() {
        let dir = std::env::temp_dir().join(format!("visicut-materials-{}", std::process::id()));
        let (mut library, custom) = Library::load(&dir).unwrap();
        assert!(!custom);
        library.materials[0].profiles[0].power_percent = 42.0;
        library.save(&dir).unwrap();
        let (loaded, custom) = Library::load(&dir).unwrap();
        assert!(custom && loaded == library);
        let path = dir.join("export.json");
        let mut own = Library {
            materials: vec![library.materials[0].clone()],
            ..library.clone()
        };
        own.materials[0].name = "Eigenes Acryl".into();
        own.export(&path).unwrap();
        let count = library.materials.len();
        assert_eq!(
            library.merge(Library::import(&std::fs::read(&path).unwrap()).unwrap()),
            1
        );
        assert_eq!(library.materials.len(), count + 1);
        let mut duplicate = library.clone();
        let profile = duplicate.materials[0].profiles[0].clone();
        duplicate.materials[0].profiles.push(profile);
        assert!(matches!(
            duplicate.save(&dir),
            Err(MaterialError::DuplicateProfile { .. })
        ));
        assert_eq!(Library::reset(&dir).unwrap(), Library::bundled());
        assert!(!Library::load(&dir).unwrap().1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
