use std::collections::HashSet;

use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::ResourceId;
use crate::voxel::VoxelBlock;

/// Kebijakan dan registri anchor struktural yang data-driven.
///
/// INVARIANT: Engine TIDAK PERNAH meng-hardcode nama material atau elevasi Y sebagai anchor.
/// Status anchor ditentukan murni dari `BlockComponents::structural_anchor` yang didefinisikan
/// pada file JSON content oleh Core maupun Mod.
#[derive(Debug, Clone, Default)]
pub struct AnchorPolicy {
    /// Kumpulan MaterialId runtime yang berfungsi sebagai anchor penopang
    anchor_materials: HashSet<MaterialId>,
    /// Kumpulan ResourceId persisten dari blok anchor
    anchor_resource_ids: HashSet<ResourceId>,
}

impl AnchorPolicy {
    /// Membuat kebijakan anchor baru dengan memindai `BlockRegistry` dan memetakan ke `MaterialRegistry`
    pub fn from_registries(materials: &MaterialRegistry, blocks: &BlockRegistry) -> Self {
        let mut policy = Self::default();

        for (_block_res_id, block_def) in blocks.iter() {
            if let Some(ref anchor_comp) = block_def.components.structural_anchor {
                if anchor_comp.is_anchor {
                    policy.anchor_resource_ids.insert(block_def.id.clone());

                    // Petakan referensi material blok ke runtime MaterialId
                    if let Some(mat_id) = materials.resolve_material_id(&block_def.material) {
                        policy.anchor_materials.insert(mat_id);
                    }
                }
            }
        }

        policy
    }

    /// Mendaftarkan MaterialId tertentu secara eksplisit sebagai anchor (misal untuk testing unit)
    pub fn register_anchor_material(&mut self, mat_id: MaterialId) {
        self.anchor_materials.insert(mat_id);
    }

    /// Mengecek apakah suatu MaterialId merupakan anchor penopang dunia
    #[inline(always)]
    pub fn is_anchor_material(&self, mat_id: MaterialId) -> bool {
        self.anchor_materials.contains(&mat_id)
    }

    /// Mengecek apakah suatu VoxelBlock merupakan anchor
    #[inline(always)]
    pub fn is_anchor_block(&self, block: &VoxelBlock) -> bool {
        if block.is_air() {
            return false;
        }
        self.is_anchor_material(block.material())
    }

    /// Jumlah tipe material anchor yang aktif terdaftar
    pub fn anchor_count(&self) -> usize {
        self.anchor_materials.len()
    }
}
