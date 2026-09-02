use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::modding::registry::{RegistryEntry, RegistryError, ResourceRegistry, ResourceSource};
use crate::modding::resource_id::{ModId, ResourceId};

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

/// Registri material sentral bersifat data-driven dengan integrasi ResourceId persisten dan provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRegistry {
    registry: ResourceRegistry<MaterialDef>,
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialRegistry {
    /// Membuat registri baru dengan menginisialisasi semantik engine bawaan `core:air` di indeks 0
    pub fn new() -> Self {
        let mut registry = ResourceRegistry::new();

        // 0: core:air (Semantik Internal Khusus Engine)
        let air_id = ResourceId::core("air").unwrap();
        registry
            .register(
                air_id,
                MaterialDef {
                    name: "Air".to_string(),
                    density_kg_m3: 0.0,
                    shear_strength_mpa: 0.0,
                    base_color: [0.0, 0.0, 0.0],
                    is_solid: false,
                    is_transparent: true,
                },
                ResourceSource::Core,
            )
            .expect("Gagal mendaftarkan core:air");

        Self { registry }
    }

    /// Mendaftarkan material baru dengan ResourceId eksplisit dan kepemilikan source
    pub fn register_resource(
        &mut self,
        res_id: ResourceId,
        def: MaterialDef,
        source: ResourceSource,
    ) -> Result<MaterialId, RegistryError> {
        let idx = self.registry.register(res_id, def, source)?;
        Ok(MaterialId(idx))
    }

    /// Menerapkan explicit override terhadap material yang sudah terdaftar
    pub fn apply_explicit_override(
        &mut self,
        target: &ResourceId,
        replacement: &ResourceId,
        source_mod: ModId,
    ) -> Result<(), RegistryError> {
        self.registry
            .apply_explicit_override(target, replacement, source_mod)
    }

    /// Mengambil referensi definisi material berdasarkan MaterialId O(1)
    #[inline(always)]
    pub fn get(&self, id: MaterialId) -> Option<&MaterialDef> {
        self.registry.get_by_index(id.0)
    }

    /// Mengambil entri lengkap (termasuk provenance) berdasarkan MaterialId
    #[inline(always)]
    pub fn get_entry(&self, id: MaterialId) -> Option<&RegistryEntry<MaterialDef>> {
        self.registry.get_entry_by_index(id.0)
    }

    /// Mengambil referensi definisi material berdasarkan ResourceId
    #[inline(always)]
    pub fn get_by_resource_id(&self, res_id: &ResourceId) -> Option<&MaterialDef> {
        self.registry.get(res_id)
    }

    /// Mengambil entri lengkap berdasarkan ResourceId
    #[inline(always)]
    pub fn get_entry_by_resource_id(
        &self,
        res_id: &ResourceId,
    ) -> Option<&RegistryEntry<MaterialDef>> {
        self.registry.get_entry(res_id)
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

    pub fn iter_entries(&self) -> impl Iterator<Item = &RegistryEntry<MaterialDef>> {
        self.registry.iter_entries()
    }
}
