use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::modding::resource_id::{ModId, ResourceId, ResourceIdError};

/// Identitas semantik unik untuk alat (Phase 11.5).
/// Format string standar: `namespace:path` (misal: "core:stone_pickaxe").
/// INVARIANT GUARDRAIL 1: Berbeda secara semantik dan struktural dari ResourceId (tanpa konversi implisit).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ToolId {
    pub namespace: ModId,
    pub path: String,
}

impl ToolId {
    pub fn new<N: Into<String>, P: Into<String>>(
        namespace: N,
        path: P,
    ) -> Result<Self, ResourceIdError> {
        let ns = ModId::new(namespace)?;
        let p = path.into();
        if p.is_empty() {
            return Err(ResourceIdError::EmptyString);
        }
        if !p
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '/')
        {
            return Err(ResourceIdError::InvalidPath(p));
        }
        Ok(Self {
            namespace: ns,
            path: p,
        })
    }

    pub fn core<P: Into<String>>(path: P) -> Result<Self, ResourceIdError> {
        Self::new(ModId::CORE, path)
    }

    pub fn as_str(&self) -> String {
        format!("{}:{}", self.namespace, self.path)
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl std::str::FromStr for ToolId {
    type Err = ResourceIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 {
            return Err(ResourceIdError::MissingDelimiter);
        }
        if parts.len() > 2 {
            return Err(ResourceIdError::TooManyDelimiters);
        }
        Self::new(parts[0], parts[1])
    }
}

/// Identitas semantik unik untuk objek interaktif dunia (Phase 11.6).
/// Format string standar: `namespace:path` (misal: "core:ancient_switch", "core:vault_door").
/// INVARIANT: Berbeda secara semantik dan struktural dari ResourceId dan ToolId (tanpa konversi implisit).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InteractableId {
    pub namespace: ModId,
    pub path: String,
}

impl InteractableId {
    pub fn new<N: Into<String>, P: Into<String>>(
        namespace: N,
        path: P,
    ) -> Result<Self, ResourceIdError> {
        let ns = ModId::new(namespace)?;
        let p = path.into();
        if p.is_empty() {
            return Err(ResourceIdError::EmptyString);
        }
        if !p
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '/')
        {
            return Err(ResourceIdError::InvalidPath(p));
        }
        Ok(Self {
            namespace: ns,
            path: p,
        })
    }

    pub fn core<P: Into<String>>(path: P) -> Result<Self, ResourceIdError> {
        Self::new(ModId::CORE, path)
    }

    pub fn as_str(&self) -> String {
        format!("{}:{}", self.namespace, self.path)
    }
}

impl fmt::Display for InteractableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl std::str::FromStr for InteractableId {
    type Err = ResourceIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 {
            return Err(ResourceIdError::MissingDelimiter);
        }
        if parts.len() > 2 {
            return Err(ResourceIdError::TooManyDelimiters);
        }
        Self::new(parts[0], parts[1])
    }
}

/// Komponen deklarasi bahwa suatu blok adalah objek yang dapat berinteraksi secara generik (Phase 11.6).
/// INVARIANT: Menjadi SATU-SATUNYA otoritas konten untuk objek interaktif.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractableComponent {
    /// Identitas semantik unik untuk objek interaktif ini
    pub id: InteractableId,
    /// Daftar aksi yang diizinkan sesuai urutan preferensi konten
    #[serde(default)]
    pub allowed_actions: Vec<crate::interaction::types::InteractableAction>,
    /// Status runtime awal saat pertama kali dimuat atau di-reset
    #[serde(default)]
    pub initial_state: crate::interaction::types::InteractableState,
    /// Isyarat audio opsional untuk feedback semantik
    #[serde(default)]
    pub audio_cue: Option<crate::interaction::types::AudioCue>,
    /// Isyarat visual opsional untuk feedback semantik
    #[serde(default)]
    pub visual_cue: Option<crate::interaction::types::VisualCue>,
}

/// Kategori semantik alat (Phase 11.5).
/// Digunakan untuk mengekspresikan kompatibilitas luas antara alat dan resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
    Generic,
}

impl fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pickaxe => write!(f, "pickaxe"),
            Self::Axe => write!(f, "axe"),
            Self::Shovel => write!(f, "shovel"),
            Self::Hoe => write!(f, "hoe"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

/// Kebutuhan alat untuk memanen resource tertentu (Phase 11.5).
/// INVARIANT GUARDRAIL 2: SATU-SATUNYA SUMBER OTORITATIF untuk kebutuhan alat dari konten disimpan di `HarvestableComponent.required_tool`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolRequirement {
    /// Tidak membutuhkan alat apa pun (dapat dipanen dengan tangan kosong)
    #[default]
    None,
    /// Membutuhkan alat apa pun yang valid dan tidak rusak (durabilitas > 0)
    AnyTool,
    /// Membutuhkan alat dari kategori tertentu (misal Pickaxe) yang tidak rusak
    Category(ToolCategory),
    /// Membutuhkan alat dengan ToolId spesifik yang tidak rusak
    Specific(ToolId),
}

fn default_base_efficiency() -> f32 {
    1.0
}

/// Aturan efektivitas/efisiensi alat terhadap resource (Phase 11.5).
/// INVARIANT GUARDRAIL 4: Efektivitas adalah metadata semantik deterministik dan TIDAK mengubah kuantitas panen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolEffectiveness {
    /// Efisiensi dasar alat terhadap resource apa pun (default: 1.0)
    #[serde(default = "default_base_efficiency")]
    pub base_efficiency: f32,
    /// Pengali efisiensi spesifik per ResourceId resource
    #[serde(default)]
    pub resource_multipliers: HashMap<ResourceId, f32>,
}

impl Default for ToolEffectiveness {
    fn default() -> Self {
        Self {
            base_efficiency: 1.0,
            resource_multipliers: HashMap::new(),
        }
    }
}

impl ToolEffectiveness {
    pub fn new(base_efficiency: f32) -> Result<Self, String> {
        let eff = Self {
            base_efficiency,
            resource_multipliers: HashMap::new(),
        };
        eff.validate()?;
        Ok(eff)
    }

    pub fn with_multiplier(
        mut self,
        resource_id: ResourceId,
        multiplier: f32,
    ) -> Result<Self, String> {
        if !multiplier.is_finite() || multiplier < 0.0 {
            return Err(format!(
                "Invalid multiplier {} for resource '{}': must be finite and >= 0.0",
                multiplier, resource_id
            ));
        }
        self.resource_multipliers.insert(resource_id, multiplier);
        Ok(self)
    }

    /// INVARIANT GUARDRAIL 5 & 13: Validasi floating-point secara deterministik.
    pub fn validate(&self) -> Result<(), String> {
        if !self.base_efficiency.is_finite() || self.base_efficiency < 0.0 {
            return Err(format!(
                "Invalid base_efficiency {}: must be finite and >= 0.0",
                self.base_efficiency
            ));
        }
        for (res, &m) in &self.resource_multipliers {
            if !m.is_finite() || m < 0.0 {
                return Err(format!(
                    "Invalid multiplier {} for resource '{}': must be finite and >= 0.0",
                    m, res
                ));
            }
        }
        Ok(())
    }

    /// Menghitung nilai efektivitas deterministik terhadap suatu resource
    #[inline(always)]
    pub fn calculate_effectiveness(&self, resource_id: &ResourceId) -> f32 {
        let mult = self
            .resource_multipliers
            .get(resource_id)
            .copied()
            .unwrap_or(1.0);
        self.base_efficiency * mult
    }
}

/// Definisi data statis alat (Phase 11.5).
/// INVARIANT GUARDRAIL 3: `max_durability` adalah SATU-SATUNYA SUMBER OTORITATIF untuk durabilitas maksimum alat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub tool_id: ToolId,
    pub category: ToolCategory,
    pub max_durability: u32,
    #[serde(default)]
    pub effectiveness: ToolEffectiveness,
}

impl ToolDefinition {
    pub fn new(tool_id: ToolId, category: ToolCategory, max_durability: u32) -> Self {
        Self {
            tool_id,
            category,
            max_durability,
            effectiveness: ToolEffectiveness::default(),
        }
    }

    pub fn with_effectiveness(mut self, effectiveness: ToolEffectiveness) -> Result<Self, String> {
        effectiveness.validate()?;
        self.effectiveness = effectiveness;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.effectiveness.validate()
    }
}

/// Definisi data material yang diparsing dari file JSON (`materials/*.json`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialDefinition {
    pub id: ResourceId,
    pub name: String,
    /// Massa jenis dalam kg/m^3
    pub density: f32,
    /// Kekuatan geser dalam MPa
    pub shear_strength: f32,
    /// Warna datar sRGB [R, G, B] dalam rentang [0.0..1.0]
    pub color: [f32; 3],
    #[serde(default = "default_true")]
    pub solid: bool,
    #[serde(default)]
    pub transparent: bool,
}

fn default_true() -> bool {
    true
}

/// Komponen generik untuk blok yang berfungsi sebagai penopang/fondasi struktural (bedrock anchor)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StructuralAnchorComponent {
    pub is_anchor: bool,
}

/// Komponen generik untuk blok yang memiliki kemampuan gaya angkat medan anti-gravitasi
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LiftCapacityComponent {
    pub capacity_kg: f32,
    pub radius_m: f32,
    pub power_consumption_w: f32,
}

/// Komponen deklarasi bahwa suatu blok dapat dipanen / dikumpulkan sebagai resource (Phase 11.3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarvestableComponent {
    /// Identitas unik semantik resource yang dihasilkan (misal `core:iron_ore`, `core:stone`)
    pub resource: ResourceId,
    /// Kuantitas hasil panen (yield) dasar deterministik per voxel (default: 1)
    #[serde(default = "default_yield_quantity")]
    pub yield_quantity: u32,
    /// Apakah blok ini dapat dipanen saat ini (default: true)
    #[serde(default = "default_true")]
    pub harvestable: bool,
    /// Kebutuhan alat untuk memanen resource ini (Phase 11.5).
    /// INVARIANT GUARDRAIL 2: SATU-SATUNYA SUMBER OTORITATIF untuk kebutuhan alat dari konten.
    #[serde(default)]
    pub required_tool: ToolRequirement,
}

fn default_yield_quantity() -> u32 {
    1
}

/// Aturan penopang fisik yang disyaratkan saat blok ditempatkan di dunia (Phase 11.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SupportRule {
    /// Blok membutuhkan setidaknya satu tetangga solid yang resident di salah satu dari 6 sisi (+X, -X, +Y, -Y, +Z, -Z)
    #[default]
    AnyAdjacent,
    /// Blok membutuhkan tetangga solid yang resident tepat di bawahnya (candidate + (0, -1, 0))
    FloorOnly,
    /// Blok membutuhkan tetangga solid yang resident pada sisi tempat ia ditempelkan (target_voxel)
    AttachmentFace,
    /// Blok dapat melayang bebas tanpa membutuhkan penopang fisik apa pun
    None,
}

/// Komponen penempatan dan aturan pembangunan untuk balok voxel (Phase 11.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildComponent {
    /// Apakah blok ini membutuhkan penopang fisik saat ditempatkan (default: true).
    /// INVARIANT: Jika `requires_support == false`, maka `support_rule` diabaikan secara semantik.
    #[serde(default = "default_true")]
    pub requires_support: bool,
    /// Aturan penopang yang harus dipenuhi jika `requires_support == true` (default: AnyAdjacent)
    #[serde(default)]
    pub support_rule: SupportRule,
    /// Batasan orientasi penempatan opsional (misal: hanya sisi tertentu)
    #[serde(default)]
    pub allowed_orientations: Option<Vec<crate::interaction::types::BlockOrientation>>,
}

impl Default for BuildComponent {
    fn default() -> Self {
        Self {
            requires_support: true,
            support_rule: SupportRule::AnyAdjacent,
            allowed_orientations: None,
        }
    }
}

/// Kumpulan komponen kapabilitas generik yang dapat ditempelkan pada blok
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BlockComponents {
    #[serde(default)]
    pub structural_anchor: Option<StructuralAnchorComponent>,
    #[serde(default)]
    pub lift_capacity: Option<LiftCapacityComponent>,
    #[serde(default)]
    pub harvestable: Option<HarvestableComponent>,
    #[serde(default)]
    pub build: Option<BuildComponent>,
    #[serde(default)]
    pub interactable: Option<InteractableComponent>,
    /// Properti kustom dinamis tambahan untuk ekstensi masa depan
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Definisi data blok yang diparsing dari file JSON (`blocks/*.json`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDefinition {
    pub id: ResourceId,
    /// Referensi ResourceId material yang digunakan blok ini
    pub material: ResourceId,
    #[serde(default)]
    pub hardness: Option<f32>,
    #[serde(default)]
    pub components: BlockComponents,
    #[serde(default)]
    pub tags: Vec<String>,
}
