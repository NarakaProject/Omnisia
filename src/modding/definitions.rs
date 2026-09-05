use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::modding::resource_id::ResourceId;

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
