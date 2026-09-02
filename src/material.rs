use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Pengenal tipe material unik (2 byte).
/// Voxel hanya menyimpan ID ini, bukan keseluruhan definisi material.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Pod, Zeroable, Serialize, Deserialize)]
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

/// Registri material sentral bersifat data-driven.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRegistry {
    materials: Vec<MaterialDef>,
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
            materials: Vec::new(),
        }
    }

    /// Registri standar dengan palet warna flat/pastel arsitektural
    pub fn with_builtin_materials() -> Self {
        let mut registry = Self::new();

        // 0: Air
        registry.register(MaterialDef {
            name: "Air".to_string(),
            density_kg_m3: 0.0,
            shear_strength_mpa: 0.0,
            base_color: [0.0, 0.0, 0.0],
            is_solid: false,
            is_transparent: true,
        });

        // 1: Stone (Pastel Slate Grey)
        registry.register(MaterialDef {
            name: "Stone".to_string(),
            density_kg_m3: 2700.0,
            shear_strength_mpa: 5.0,
            base_color: [0.62, 0.65, 0.68],
            is_solid: true,
            is_transparent: false,
        });

        // 2: Dirt (Warm Muted Brown)
        registry.register(MaterialDef {
            name: "Dirt".to_string(),
            density_kg_m3: 1600.0,
            shear_strength_mpa: 1.5,
            base_color: [0.55, 0.42, 0.33],
            is_solid: true,
            is_transparent: false,
        });

        // 3: Grass (Soft Pastel Sage Green)
        registry.register(MaterialDef {
            name: "Grass".to_string(),
            density_kg_m3: 1400.0,
            shear_strength_mpa: 1.2,
            base_color: [0.46, 0.68, 0.42],
            is_solid: true,
            is_transparent: false,
        });

        // 4: Sand (Pastel Warm Sand)
        registry.register(MaterialDef {
            name: "Sand".to_string(),
            density_kg_m3: 1800.0,
            shear_strength_mpa: 0.8,
            base_color: [0.85, 0.78, 0.60],
            is_solid: true,
            is_transparent: false,
        });

        // 5: Metal Frame (Matte Steel Blue)
        registry.register(MaterialDef {
            name: "Metal Frame".to_string(),
            density_kg_m3: 7850.0,
            shear_strength_mpa: 250.0,
            base_color: [0.35, 0.45, 0.55],
            is_solid: true,
            is_transparent: false,
        });

        // 6: AntiGravity Core Casing (Cybernetic Mint/Cyan)
        registry.register(MaterialDef {
            name: "AntiGravity Core Casing".to_string(),
            density_kg_m3: 4500.0,
            shear_strength_mpa: 350.0,
            base_color: [0.25, 0.88, 0.82],
            is_solid: true,
            is_transparent: false,
        });

        // 7: Gold Accent (Warm Pastel Gold)
        registry.register(MaterialDef {
            name: "Gold Accent".to_string(),
            density_kg_m3: 19300.0,
            shear_strength_mpa: 100.0,
            base_color: [0.92, 0.75, 0.32],
            is_solid: true,
            is_transparent: false,
        });

        // 8: Oak Wood (Soft Warm Cedar)
        registry.register(MaterialDef {
            name: "Oak Wood".to_string(),
            density_kg_m3: 700.0,
            shear_strength_mpa: 15.0,
            base_color: [0.60, 0.45, 0.32],
            is_solid: true,
            is_transparent: false,
        });

        // 9: Leaf (Pastel Forest Leaf)
        registry.register(MaterialDef {
            name: "Leaf".to_string(),
            density_kg_m3: 200.0,
            shear_strength_mpa: 0.2,
            base_color: [0.35, 0.58, 0.38],
            is_solid: true,
            is_transparent: false,
        });

        // 10: Glass (Frosted Tint)
        registry.register(MaterialDef {
            name: "Glass".to_string(),
            density_kg_m3: 2500.0,
            shear_strength_mpa: 30.0,
            base_color: [0.85, 0.92, 0.95],
            is_solid: true,
            is_transparent: true,
        });

        registry
    }

    /// Mendaftarkan material baru dan mengembalikan `MaterialId`-nya
    pub fn register(&mut self, def: MaterialDef) -> MaterialId {
        let id = MaterialId(self.materials.len() as u16);
        self.materials.push(def);
        id
    }

    /// Mengambil referensi definisi material berdasarkan ID
    #[inline(always)]
    pub fn get(&self, id: MaterialId) -> Option<&MaterialDef> {
        self.materials.get(id.0 as usize)
    }

    /// Mengambil warna dasar material dalam RGB f32
    #[inline(always)]
    pub fn get_color(&self, id: MaterialId) -> [f32; 3] {
        self.get(id)
            .map(|m| m.base_color)
            .unwrap_or([1.0, 0.0, 1.0]) // Magenta error fallback
    }

    /// Mengecek apakah suatu material solid (menghalangi pandangan/solid block)
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
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}
