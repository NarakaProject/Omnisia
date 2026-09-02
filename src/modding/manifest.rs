use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::modding::resource_id::{ModId, ResourceId, ResourceIdError};
use crate::modding::version::{
    is_engine_api_compatible, DependencyRequirement, VersionError, ENGINE_API_VERSION,
};

/// Error saat validasi atau deserialisasi `mod.toml`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    TomlParseError(String),
    MissingId,
    MissingName,
    MissingVersion,
    MissingEngineApi,
    InvalidId(ResourceIdError),
    InvalidVersion(VersionError),
    IncompatibleApi { required: String, current: String },
    InvalidDependency { dep: String, err: String },
    InvalidOverride { reason: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TomlParseError(e) => write!(f, "Gagal parsing mod.toml: {}", e),
            Self::MissingId => write!(f, "Field 'id' wajib diisi dalam mod.toml"),
            Self::MissingName => write!(f, "Field 'name' wajib diisi dalam mod.toml"),
            Self::MissingVersion => write!(f, "Field 'version' wajib diisi dalam mod.toml"),
            Self::MissingEngineApi => write!(f, "Field 'engine_api' wajib diisi dalam mod.toml"),
            Self::InvalidId(e) => write!(f, "Mod ID tidak valid: {}", e),
            Self::InvalidVersion(e) => write!(f, "Versi mod tidak valid: {}", e),
            Self::IncompatibleApi { required, current } => write!(
                f,
                "Ketidakcocokan Engine API: Mod membutuhkan '{}', engine saat ini '{}'",
                required, current
            ),
            Self::InvalidDependency { dep, err } => {
                write!(f, "Deklarasi dependensi '{}' tidak valid: {}", dep, err)
            }
            Self::InvalidOverride { reason } => {
                write!(f, "Deklarasi override tidak valid: {}", reason)
            }
        }
    }
}

impl std::error::Error for ManifestError {}

/// Informasi pengarang mod (opsional)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    pub contact: Option<String>,
}

/// Deklarasi eksplisit untuk penimpaan (*override*) konten target
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideDeclaration {
    pub target: ResourceId,
    pub replacement: ResourceId,
}

/// Struktur data resmi untuk file `mod.toml`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModManifest {
    pub id: ModId,
    pub name: String,
    pub version: String,
    pub engine_api: String,
    pub description: Option<String>,
    pub author: Option<AuthorInfo>,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub overrides: Vec<OverrideDeclaration>,
}

impl ModManifest {
    /// Parsing manifest dari string teks TOML
    pub fn from_toml_str(toml_str: &str) -> Result<Self, ManifestError> {
        let manifest: Self =
            toml::from_str(toml_str).map_err(|e| ManifestError::TomlParseError(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Memvalidasi seluruh field manifest terhadap standar engine saat ini
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.name.trim().is_empty() {
            return Err(ManifestError::MissingName);
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::MissingVersion);
        }
        if self.engine_api.trim().is_empty() {
            return Err(ManifestError::MissingEngineApi);
        }

        // 1. Validasi Semver Version
        semver::Version::parse(&self.version).map_err(|e| {
            ManifestError::InvalidVersion(VersionError::InvalidVersionString(e.to_string()))
        })?;

        // 2. Validasi Kompatibilitas Engine API
        let compatible = is_engine_api_compatible(&self.engine_api, ENGINE_API_VERSION)
            .map_err(ManifestError::InvalidVersion)?;

        if !compatible {
            return Err(ManifestError::IncompatibleApi {
                required: self.engine_api.clone(),
                current: ENGINE_API_VERSION.to_string(),
            });
        }

        // 3. Validasi Format Dependency Keys
        for (dep_id, dep_req) in &self.dependencies {
            ModId::new(dep_id).map_err(|e| ManifestError::InvalidDependency {
                dep: dep_id.clone(),
                err: e.to_string(),
            })?;

            DependencyRequirement::new(dep_id, dep_req).map_err(|e| {
                ManifestError::InvalidDependency {
                    dep: dep_id.clone(),
                    err: e.to_string(),
                }
            })?;
        }

        // 4. Validasi Deklarasi Overrides
        for ov in &self.overrides {
            if ov.target == ov.replacement {
                return Err(ManifestError::InvalidOverride {
                    reason: format!(
                        "Target override '{}' tidak boleh sama dengan replacement",
                        ov.target
                    ),
                });
            }

            // Aturan Kepemilikan: Mod hanya boleh memakai replacement yang dimilikinya sendiri!
            if ov.replacement.namespace != self.id {
                return Err(ManifestError::InvalidOverride {
                    reason: format!(
                        "Mod '{}' mencoba menggunakan replacement '{}' dari namespace lain",
                        self.id, ov.replacement
                    ),
                });
            }
        }

        Ok(())
    }

    /// Mengambil daftar `DependencyRequirement` yang telah diparsing
    pub fn parsed_dependencies(&self) -> Result<Vec<DependencyRequirement>, ManifestError> {
        let mut reqs = Vec::new();
        for (dep_id, version_req) in &self.dependencies {
            let req = DependencyRequirement::new(dep_id, version_req).map_err(|e| {
                ManifestError::InvalidDependency {
                    dep: dep_id.clone(),
                    err: e.to_string(),
                }
            })?;
            reqs.push(req);
        }
        // Sorting deterministik
        reqs.sort_by(|a, b| a.mod_id.cmp(&b.mod_id));
        Ok(reqs)
    }
}
