use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use crate::modding::resource_id::{ModId, ResourceIdError};

/// Error saat validasi atau resolusi AssetId
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    EmptyString,
    InvalidNamespace(ResourceIdError),
    MissingDelimiter,
    TooManyDelimiters,
    InvalidPath(String),
    PathTraversalDetected(String),
    AbsolutePathNotAllowed(String),
    NamespaceNotRegistered(ModId),
    AssetNotFound(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyString => write!(f, "AssetId tidak boleh kosong"),
            Self::InvalidNamespace(e) => write!(f, "Namespace AssetId tidak valid: {}", e),
            Self::MissingDelimiter => {
                write!(f, "Format AssetId harus 'namespace:path' (kurang ':')")
            }
            Self::TooManyDelimiters => write!(f, "AssetId mengandung lebih dari satu pemisah ':'"),
            Self::InvalidPath(p) => write!(f, "Path asset '{}' mengandung karakter tidak valid", p),
            Self::PathTraversalDetected(p) => {
                write!(
                    f,
                    "Percobaan path traversal terdeteksi pada asset path: '{}'",
                    p
                )
            }
            Self::AbsolutePathNotAllowed(p) => {
                write!(f, "Path absolut tidak diizinkan pada AssetId: '{}'", p)
            }
            Self::NamespaceNotRegistered(ns) => {
                write!(f, "Namespace '{}' belum terdaftar pada AssetResolver", ns)
            }
            Self::AssetNotFound(id) => write!(f, "Asset '{}' tidak ditemukan pada filesystem", id),
        }
    }
}

impl std::error::Error for AssetError {}

/// Identitas persisten global untuk semua tipe aset (Texture, Model, Sound, Shader, UI, dll).
/// Format standar: `namespace:path` (contoh: `core:textures/stone.png`, `example_mod:models/reactor.glb`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId {
    pub namespace: ModId,
    pub path: String,
}

impl AssetId {
    pub fn new<N: Into<String>, P: Into<String>>(
        namespace: N,
        path: P,
    ) -> Result<Self, AssetError> {
        let ns = ModId::new(namespace).map_err(AssetError::InvalidNamespace)?;
        let p = path.into();
        Self::validate_path(&p)?;
        Ok(Self {
            namespace: ns,
            path: p,
        })
    }

    pub fn core<P: Into<String>>(path: P) -> Result<Self, AssetError> {
        Self::new(ModId::CORE, path)
    }

    pub fn parse(s: &str) -> Result<Self, AssetError> {
        if s.trim().is_empty() {
            return Err(AssetError::EmptyString);
        }
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 1 {
            return Err(AssetError::MissingDelimiter);
        }
        if parts.len() > 2 {
            return Err(AssetError::TooManyDelimiters);
        }
        Self::new(parts[0], parts[1])
    }

    fn validate_path(s: &str) -> Result<(), AssetError> {
        if s.is_empty() {
            return Err(AssetError::EmptyString);
        }

        // Cek path absolut
        if s.starts_with('/') || s.starts_with('\\') || (s.len() > 1 && s.as_bytes()[1] == b':') {
            return Err(AssetError::AbsolutePathNotAllowed(s.to_string()));
        }

        let path_obj = Path::new(s);
        for comp in path_obj.components() {
            match comp {
                Component::ParentDir => {
                    return Err(AssetError::PathTraversalDetected(s.to_string()));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(AssetError::AbsolutePathNotAllowed(s.to_string()));
                }
                Component::Normal(c) => {
                    let comp_str = c.to_string_lossy();
                    if !comp_str.chars().all(|ch| {
                        ch.is_ascii_lowercase()
                            || ch.is_ascii_digit()
                            || ch == '_'
                            || ch == '.'
                            || ch == '-'
                    }) {
                        return Err(AssetError::InvalidPath(s.to_string()));
                    }
                }
                Component::CurDir => {}
            }
        }

        Ok(())
    }

    pub fn to_canonical_string(&self) -> String {
        format!("{}:{}", self.namespace.as_str(), self.path)
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl FromStr for AssetId {
    type Err = AssetError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for AssetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_canonical_string())
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Lokasi fisik / sumber data dari aset yang telah di-resolve
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetLocation {
    Filesystem(PathBuf),
}

/// Abstraksi resolver aset yang independen dari filesystem layout
pub struct AssetResolver {
    roots: HashMap<ModId, PathBuf>,
}

impl Default for AssetResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetResolver {
    pub fn new() -> Self {
        Self {
            roots: HashMap::new(),
        }
    }

    /// Mendaftarkan root directory untuk namespace tertentu (misal: "core" -> "content/core")
    pub fn register_root<P: AsRef<Path>>(&mut self, namespace: ModId, root_path: P) {
        self.roots
            .insert(namespace, root_path.as_ref().to_path_buf());
    }

    /// Menyelesaikan AssetId menjadi AssetLocation fisik dengan validasi keamanan path containment
    pub fn resolve(&self, asset_id: &AssetId) -> Result<AssetLocation, AssetError> {
        let root = self
            .roots
            .get(&asset_id.namespace)
            .ok_or_else(|| AssetError::NamespaceNotRegistered(asset_id.namespace.clone()))?;

        let candidate_path = root.join(&asset_id.path);

        // Validasi keamanan ekstra: pastikan path tidak melarikan diri dari root
        if !candidate_path.starts_with(root) {
            return Err(AssetError::PathTraversalDetected(asset_id.path.clone()));
        }

        Ok(AssetLocation::Filesystem(candidate_path))
    }
}
