use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::modding::registry::ResourceRegistry;
use crate::modding::resource_id::ResourceId;

/// Pengenal tipe material unik (2 byte).
/// Voxel hanya menyimpan ID ini, bukan keseluruhan definisi material.
#[repr(transparent)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Pod,
    Zeroable,
    Serialize,
    Deserialize,
)]
pub struct MaterialId(pub u16);

impl MaterialId {
    pub const AIR: Self = Self(0);
    pub const STONE: Self = Self(1);
    pub const DIRT: Self = Self(2);
    pub const GRASS: Self = Self(3);
    pub const SAND: Self = Self(4);
    pub const METAL_FRAME: Self = Self(5);
    pub const AG_CORE_CASING: Self = Self(6);
    pub const GOLD_ACCENT: Self = Self(7);
    pub const OAK_WOOD: Self = Self(8);
    pub const LEAF: Self = Self(9);
    pub const GLASS: Self = Self(10);
}

/// Definisi sifat fisik, visual, dan struktural dari suatu material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialDef {
    pub name: String,
    pub density_kg_m3: f32,
    pub shear_strength_mpa: f32,
    /// Warna sRGB datar/pastel (tanpa tekstur kasar 16x16)
    pub base_color: [f32; 3],
    pub is_solid: bool,
    pub is_transparent: bool,
}

/// Registri material sentral bersifat data-driven dengan integrasi ResourceId persisten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRegistry {
    registry: ResourceRegistry<MaterialDef>,
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::with_builtin_materials()
    }
}

impl MaterialRegistry {
    /// Membuat registri baru kosong
    pub fn new() -> Self {
        Self {
            registry: ResourceRegistry::new(),
        }
    }

    /// Registri standar dengan built-in materials namespace "core"
    pub fn with_builtin_materials() -> Self {
        let mut registry = Self::new();

        // 0: core:air
        registry.register_named(
            "core:air",
            MaterialDef {
                name: "Air".to_string(),
                density_kg_m3: 0.0,
                shear_strength_mpa: 0.0,
                base_color: [0.0, 0.0, 0.0],
                is_solid: false,
                is_transparent: true,
            },
        );

        // 1: core:stone (Pastel Slate Grey)
        registry.register_named(
            "core:stone",
            MaterialDef {
                name: "Stone".to_string(),
                density_kg_m3: 2700.0,
                shear_strength_mpa: 5.0,
                base_color: [0.62, 0.65, 0.68],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 2: core:dirt (Warm Muted Brown)
        registry.register_named(
            "core:dirt",
            MaterialDef {
                name: "Dirt".to_string(),
                density_kg_m3: 1600.0,
                shear_strength_mpa: 1.5,
                base_color: [0.55, 0.42, 0.33],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 3: core:grass (Soft Pastel Sage Green)
        registry.register_named(
            "core:grass",
            MaterialDef {
                name: "Grass".to_string(),
                density_kg_m3: 1400.0,
                shear_strength_mpa: 1.2,
                base_color: [0.46, 0.68, 0.42],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 4: core:sand (Pastel Warm Sand)
        registry.register_named(
            "core:sand",
            MaterialDef {
                name: "Sand".to_string(),
                density_kg_m3: 1800.0,
                shear_strength_mpa: 0.8,
                base_color: [0.85, 0.78, 0.60],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 5: core:metal_frame (Matte Steel Blue)
        registry.register_named(
            "core:metal_frame",
            MaterialDef {
                name: "Metal Frame".to_string(),
                density_kg_m3: 7850.0,
                shear_strength_mpa: 250.0,
                base_color: [0.35, 0.45, 0.55],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 6: core:ag_core_casing (Cybernetic Mint/Cyan)
        registry.register_named(
            "core:ag_core_casing",
            MaterialDef {
                name: "AntiGravity Core Casing".to_string(),
                density_kg_m3: 4500.0,
                shear_strength_mpa: 350.0,
                base_color: [0.25, 0.88, 0.82],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 7: core:gold_accent (Warm Pastel Gold)
        registry.register_named(
            "core:gold_accent",
            MaterialDef {
                name: "Gold Accent".to_string(),
                density_kg_m3: 19300.0,
                shear_strength_mpa: 100.0,
                base_color: [0.92, 0.75, 0.32],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 8: core:oak_wood (Soft Warm Cedar)
        registry.register_named(
            "core:oak_wood",
            MaterialDef {
                name: "Oak Wood".to_string(),
                density_kg_m3: 700.0,
                shear_strength_mpa: 15.0,
                base_color: [0.60, 0.45, 0.32],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 9: core:leaf (Pastel Forest Leaf)
        registry.register_named(
            "core:leaf",
            MaterialDef {
                name: "Leaf".to_string(),
                density_kg_m3: 200.0,
                shear_strength_mpa: 0.2,
                base_color: [0.35, 0.58, 0.38],
                is_solid: true,
                is_transparent: false,
            },
        );

        // 10: core:glass (Frosted Tint)
        registry.register_named(
            "core:glass",
            MaterialDef {
                name: "Glass".to_string(),
                density_kg_m3: 2500.0,
                shear_strength_mpa: 30.0,
                base_color: [0.85, 0.92, 0.95],
                is_solid: true,
                is_transparent: true,
            },
        );

        registry
    }

    /// Helper pendaftaran material dengan string resource ID (misal: "core:stone")
    pub fn register_named(&mut self, res_id_str: &str, def: MaterialDef) -> MaterialId {
        let res_id = ResourceId::parse(res_id_str)
            .unwrap_or_else(|e| panic!("Invalid resource ID string '{}': {}", res_id_str, e));
        self.register_resource(res_id, def)
    }

    /// Mendaftarkan material baru dengan ResourceId eksplisit
    pub fn register_resource(&mut self, res_id: ResourceId, def: MaterialDef) -> MaterialId {
        let idx = self
            .registry
            .register(res_id, def)
            .unwrap_or_else(|e| panic!("Gagal mendaftarkan material: {}", e));
        MaterialId(idx)
    }

    /// Mendaftarkan material baru secara anonim di bawah namespace core
    pub fn register(&mut self, def: MaterialDef) -> MaterialId {
        let name_slug = def.name.to_lowercase().replace(' ', "_");
        let res_id = ResourceId::core(&name_slug).unwrap_or_else(|_| {
            let fallback = format!("material_{}", self.registry.len());
            ResourceId::core(fallback).unwrap()
        });
        self.register_resource(res_id, def)
    }

    /// Mengambil referensi definisi material berdasarkan MaterialId O(1)
    #[inline(always)]
    pub fn get(&self, id: MaterialId) -> Option<&MaterialDef> {
        self.registry.get_by_index(id.0)
    }

    /// Mengambil referensi definisi material berdasarkan ResourceId
    #[inline(always)]
    pub fn get_by_resource_id(&self, res_id: &ResourceId) -> Option<&MaterialDef> {
        self.registry.get(res_id)
    }

    /// Mengonversi ResourceId persisten ke runtime MaterialId
    #[inline(always)]
    pub fn resolve_material_id(&self, res_id: &ResourceId) -> Option<MaterialId> {
        self.registry.resolve_runtime_id(res_id).map(MaterialId)
    }

    /// Mengambil ResourceId dari MaterialId runtime
    #[inline(always)]
    pub fn resolve_resource_id(&self, id: MaterialId) -> Option<&ResourceId> {
        self.registry.get_resource_id_by_index(id.0)
    }

    /// Mengambil warna dasar material dalam RGB f32
    #[inline(always)]
    pub fn get_color(&self, id: MaterialId) -> [f32; 3] {
        self.get(id)
            .map(|m| m.base_color)
            .unwrap_or([1.0, 0.0, 1.0]) // Magenta error fallback
    }

    /// Mengecek apakah suatu material solid
    #[inline(always)]
    pub fn is_solid(&self, id: MaterialId) -> bool {
        self.get(id).map(|m| m.is_solid).unwrap_or(false)
    }

    /// Mengecek apakah suatu material transparan
    #[inline(always)]
    pub fn is_transparent(&self, id: MaterialId) -> bool {
        self.get(id).map(|m| m.is_transparent).unwrap_or(true)
    }

    pub fn len(&self) -> usize {
        self.registry.len()
    }

    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ResourceId, &MaterialDef)> {
        self.registry.iter()
    }
}
