use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::modding::resource_id::{ModId, ResourceIdError};

/// Single Source of Truth untuk versi Engine API saat ini
pub const ENGINE_API_VERSION: &str = "0.2.0";

/// Error terkait versi mod atau ketidakcocokan Engine API
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    InvalidVersionString(String),
    InvalidRequirementString(String),
    IncompatibleEngineApi {
        required: String,
        current: String,
    },
    IncompatibleModDependency {
        dependency: String,
        required: String,
        installed: String,
    },
    MissingDependency(String),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVersionString(v) => write!(f, "Format semver version '{}' tidak valid", v),
            Self::InvalidRequirementString(r) => write!(f, "Format version requirement '{}' tidak valid", r),
            Self::IncompatibleEngineApi { required, current } => write!(
                f,
                "Engine API tidak kompatibel: Mod meminta '{}', namun engine saat ini '{}'",
                required, current
            ),
            Self::IncompatibleModDependency {
                dependency,
                required,
                installed,
            } => write!(
                f,
                "Dependensi '{}' tidak kompatibel: Versi dibutuhkan '{}', namun versi terpasang '{}'",
                dependency, required, installed
            ),
            Self::MissingDependency(dep) => write!(f, "Dependensi '{}' tidak ditemukan", dep),
        }
    }
}

impl std::error::Error for VersionError {}

/// Memeriksa apakah versi Engine API yang diminta mod kompatibel dengan engine saat ini
pub fn is_engine_api_compatible(
    required_str: &str,
    current_str: &str,
) -> Result<bool, VersionError> {
    let current = Version::parse(current_str)
        .map_err(|_| VersionError::InvalidVersionString(current_str.to_string()))?;

    // Jika mod hanya menulis "0.2", parsing sebagai requirement "^0.2"
    let req_formatted = if required_str.starts_with('^')
        || required_str.starts_with('~')
        || required_str.starts_with('=')
        || required_str.starts_with('>')
        || required_str.starts_with('<')
        || required_str.starts_with('*')
    {
        required_str.to_string()
    } else {
        format!("^{}", required_str)
    };

    let req = VersionReq::parse(&req_formatted)
        .map_err(|_| VersionError::InvalidRequirementString(required_str.to_string()))?;

    Ok(req.matches(&current))
}

/// Deklarasi kebutuhan dependensi mod
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRequirement {
    pub mod_id: ModId,
    /// Syarat versi (misal: ">=0.1.0", "^0.2", "*")
    pub version_req: String,
}

impl DependencyRequirement {
    pub fn new<S: Into<String>>(mod_id_str: &str, version_req: S) -> Result<Self, ResourceIdError> {
        let mod_id = ModId::new(mod_id_str)?;
        Ok(Self {
            mod_id,
            version_req: version_req.into(),
        })
    }

    pub fn matches(&self, installed_version_str: &str) -> Result<bool, VersionError> {
        let installed = Version::parse(installed_version_str)
            .map_err(|_| VersionError::InvalidVersionString(installed_version_str.to_string()))?;

        let req_formatted = if self.version_req.is_empty() || self.version_req == "*" {
            "*".to_string()
        } else if self.version_req.starts_with('^')
            || self.version_req.starts_with('~')
            || self.version_req.starts_with('=')
            || self.version_req.starts_with('>')
            || self.version_req.starts_with('<')
        {
            self.version_req.clone()
        } else {
            format!("^{}", self.version_req)
        };

        let req = VersionReq::parse(&req_formatted)
            .map_err(|_| VersionError::InvalidRequirementString(self.version_req.clone()))?;

        Ok(req.matches(&installed))
    }
}
