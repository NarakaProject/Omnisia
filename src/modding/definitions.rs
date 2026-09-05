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

/// Kumpulan komponen kapabilitas generik yang dapat ditempelkan pada blok
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BlockComponents {
    #[serde(default)]
    pub structural_anchor: Option<StructuralAnchorComponent>,
    #[serde(default)]
    pub lift_capacity: Option<LiftCapacityComponent>,
    #[serde(default)]
    pub harvestable: Option<HarvestableComponent>,
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
