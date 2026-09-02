use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Error saat parsing atau validasi ID namespace atau resource
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceIdError {
    EmptyString,
    InvalidNamespace(String),
    InvalidPath(String),
    MissingDelimiter,
    TooManyDelimiters,
}

impl fmt::Display for ResourceIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyString => write!(f, "ID tidak boleh kosong"),
            Self::InvalidNamespace(ns) => write!(
                f,
                "Namespace '{}' tidak valid. Hanya karakter [a-z0-9_] yang diizinkan.",
                ns
            ),
            Self::InvalidPath(p) => write!(
                f,
                "Path '{}' tidak valid. Hanya karakter [a-z0-9_/] yang diizinkan.",
                p
            ),
            Self::MissingDelimiter => {
                write!(f, "Format ResourceId harus 'namespace:path' (kurang ':')")
            }
            Self::TooManyDelimiters => {
                write!(f, "ResourceId mengandung lebih dari satu pemisah ':'")
            }
        }
    }
}

impl std::error::Error for ResourceIdError {}

/// Pengenal unik namespace mod (contoh: "core", "example_mod", "tech_machinery").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModId(String);

impl ModId {
    pub const CORE: &'static str = "core";

    pub fn new<S: Into<String>>(id: S) -> Result<Self, ResourceIdError> {
        let s = id.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    pub fn core() -> Self {
        Self(Self::CORE.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(s: &str) -> Result<(), ResourceIdError> {
        if s.is_empty() {
            return Err(ResourceIdError::EmptyString);
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(ResourceIdError::InvalidNamespace(s.to_string()));
        }
        Ok(())
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ModId {
    type Err = ResourceIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for ModId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ModId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

/// Identitas persisten global untuk semua tipe resource (Material, Block, Structure, dll).
/// Format standar: `namespace:path` (contoh: `core:stone`, `example_mod:steel_block`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId {
    pub namespace: ModId,
    pub path: String,
}

impl ResourceId {
    pub fn new<N: Into<String>, P: Into<String>>(
        namespace: N,
        path: P,
    ) -> Result<Self, ResourceIdError> {
        let ns = ModId::new(namespace)?;
        let p = path.into();
        Self::validate_path(&p)?;
        Ok(Self {
            namespace: ns,
            path: p,
        })
    }

    pub fn core<P: Into<String>>(path: P) -> Result<Self, ResourceIdError> {
        Self::new(ModId::CORE, path)
    }

    pub fn parse(s: &str) -> Result<Self, ResourceIdError> {
        if s.is_empty() {
            return Err(ResourceIdError::EmptyString);
        }
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 1 {
            return Err(ResourceIdError::MissingDelimiter);
        }
        if parts.len() > 2 {
            return Err(ResourceIdError::TooManyDelimiters);
        }
        Self::new(parts[0], parts[1])
    }

    fn validate_path(s: &str) -> Result<(), ResourceIdError> {
        if s.is_empty() {
            return Err(ResourceIdError::EmptyString);
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '/')
        {
            return Err(ResourceIdError::InvalidPath(s.to_string()));
        }
        Ok(())
    }

    pub fn to_canonical_string(&self) -> String {
        format!("{}:{}", self.namespace.as_str(), self.path)
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl FromStr for ResourceId {
    type Err = ResourceIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for ResourceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_canonical_string())
    }
}

impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
