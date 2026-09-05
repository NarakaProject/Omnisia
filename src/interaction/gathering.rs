use glam::{IVec3, Vec3};
use std::collections::HashMap;

use super::mutation::execute_interaction_transaction;
use super::raycast::raycast_player_interaction;
use super::types::{
    CollectionResult, GatheringError, GatheringResult, InteractionCooldown, ResourceDefinition,
    VoxelHit, VoxelRaycastResult,
};
use crate::coord::world_voxel_to_chunk_and_local;
use crate::csg::edit::VoxelEdit;
use crate::csg::transaction::VoxelEditTransaction;
use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::ResourceId;
use crate::player::PlayerController;
use crate::world::World;

/// Registri data-driven untuk pemetaan MaterialId dan ResourceId ke definisi resource yang dapat dipanen (Phase 11.3).
///
/// Menghubungkan dunia voxel fisik ke semantik resource secara O(1).
#[derive(Debug, Clone, Default)]
pub struct ResourceGatheringRegistry {
    /// Pemetaan cepat O(1) dari MaterialId runtime ke ResourceDefinition
    by_material: HashMap<MaterialId, ResourceDefinition>,
    /// Pemetaan dari ResourceId persisten ke ResourceDefinition
    by_resource_id: HashMap<ResourceId, ResourceDefinition>,
    /// Pemetaan balik dari Block ResourceId ke MaterialId
    block_to_material: HashMap<ResourceId, MaterialId>,
}

impl ResourceGatheringRegistry {
    /// Membuat registri resource gathering baru yang kosong
    pub fn new() -> Self {
        Self {
            by_material: HashMap::new(),
            by_resource_id: HashMap::new(),
            block_to_material: HashMap::new(),
        }
    }

    /// Membangun registri resource gathering dengan memindai `BlockRegistry` dan memetakan ke `MaterialRegistry`
    /// berdasarkan `BlockComponents::harvestable` yang didefinisikan dalam data Core / Mod.
    pub fn from_registries(materials: &MaterialRegistry, blocks: &BlockRegistry) -> Self {
        let mut registry = Self::new();

        for (_block_res_id, block_def) in blocks.iter() {
            if let Some(ref harvest_comp) = block_def.components.harvestable {
                if harvest_comp.harvestable {
                    if let Some(mat_id) = materials.resolve_material_id(&block_def.material) {
                        let def = ResourceDefinition {
                            resource_id: harvest_comp.resource.clone(),
                            base_yield: harvest_comp.yield_quantity,
                            harvestable: true,
                            source_block: Some(block_def.id.clone()),
                            required_tool: harvest_comp.required_tool.clone(),
                        };
                        registry.register(mat_id, def);
                        registry
                            .block_to_material
                            .insert(block_def.id.clone(), mat_id);
                    }
                }
            }
        }

        registry
    }

    /// Mendaftarkan definisi resource untuk suatu MaterialId tertentu secara eksplisit (misal untuk unit test)
    pub fn register(&mut self, material: MaterialId, def: ResourceDefinition) {
        if let Some(ref source_block) = def.source_block {
            self.block_to_material
                .insert(source_block.clone(), material);
        }
        self.by_resource_id
            .insert(def.resource_id.clone(), def.clone());
        self.by_material.insert(material, def);
    }

    /// Mengambil referensi definisi resource berdasarkan MaterialId secara O(1)
    #[inline(always)]
    pub fn get_by_material(&self, material: MaterialId) -> Option<&ResourceDefinition> {
        self.by_material.get(&material)
    }

    /// Mengambil referensi definisi resource berdasarkan ResourceId persisten secara O(1)
    #[inline(always)]
    pub fn get_by_resource_id(&self, id: &ResourceId) -> Option<&ResourceDefinition> {
        self.by_resource_id.get(id)
    }

    /// Mengecek apakah suatu MaterialId merupakan resource yang dapat dipanen saat ini
    #[inline(always)]
    pub fn is_harvestable(&self, material: MaterialId) -> bool {
        self.by_material
            .get(&material)
            .is_some_and(|d| d.harvestable)
    }

    /// Jumlah tipe material resource harvestable yang terdaftar
    pub fn len(&self) -> usize {
        self.by_material.len()
    }

    /// Apakah registri resource kosong
    pub fn is_empty(&self) -> bool {
        self.by_material.is_empty()
    }

    /// Iterasi seluruh pasangan (MaterialId, &ResourceDefinition)
    pub fn iter(&self) -> impl Iterator<Item = (&MaterialId, &ResourceDefinition)> {
        self.by_material.iter()
    }
}

/// Mengambil resolusi definisi resource berdasarkan MaterialId secara read-only
#[inline(always)]
pub fn resolve_resource(
    registry: &ResourceGatheringRegistry,
    material: MaterialId,
) -> Option<&ResourceDefinition> {
    registry.get_by_material(material)
}

/// Menghitung kuantitas hasil panen (yield) secara murni deterministik tanpa keacakan global
#[inline(always)]
pub fn calculate_yield(def: &ResourceDefinition) -> u32 {
    def.base_yield
}

/// Memvalidasi kelayakan pemanenan voxel yang ditarget oleh raycast (Phase 11.3).
///
/// Syarat Validasi:
/// 1. Jarak kontak tidak melampaui `max_reach` (inklusif).
/// 2. Chunk target berstatus resident di memori `ChunkStore`.
/// 3. Voxel target bukan udara (AIR).
/// 4. Voxel target terdaftar sebagai resource yang dapat dipanen (`harvestable == true`).
///
/// JAMINAN: Murni read-only, tanpa memutasi ChunkStore atau registry.
pub fn can_gather(
    world: &World,
    hit: &VoxelHit,
    max_reach: f32,
) -> Result<(ResourceDefinition, VoxelEdit), GatheringError> {
    // 1. Validasi reach
    if hit.distance > max_reach {
        return Err(GatheringError::ExceedsReach {
            distance: hit.distance,
            max_reach,
        });
    }

    // 2. Validasi residency chunk target
    let (chunk_coord, _) = world_voxel_to_chunk_and_local(hit.voxel_coord);
    if !world.store.is_chunk_resident(&chunk_coord) {
        return Err(GatheringError::TargetNotResident {
            coord: hit.voxel_coord,
        });
    }

    // 3. Validasi isi voxel target
    let block = world.store.get_voxel_world_checked(hit.voxel_coord).ok_or(
        GatheringError::TargetNotResident {
            coord: hit.voxel_coord,
        },
    )?;

    if block.is_air() {
        return Err(GatheringError::TargetIsAir {
            coord: hit.voxel_coord,
        });
    }

    // 4. Resolusi resource dan validasi status harvestable
    let res_def = world.resources.get_by_material(block.material()).cloned();
    match res_def {
        Some(def) if def.harvestable => {
            let edit = VoxelEdit::remove(hit.voxel_coord);
            Ok((def, edit))
        }
        _ => Err(GatheringError::NotHarvestable {
            coord: hit.voxel_coord,
            material: block.material(),
            block_id: None,
        }),
    }
}

/// Melakukan validasi menyeluruh terhadap aksi gathering pemain dan membangun transaksi CSG.
///
/// Menggunakan raycast pemain Phase 11.1 dan preflight validation CSG Phase 10.2 / 10.4.
pub fn validate_gather_action(
    world: &World,
    player: &PlayerController,
    look_direction: Vec3,
) -> Result<(ResourceDefinition, VoxelEditTransaction), GatheringError> {
    let result = raycast_player_interaction(&world.store, player, look_direction);
    let hit = match result {
        VoxelRaycastResult::Hit(h) => h,
        _ => return Err(GatheringError::NoTargetHit),
    };

    let (res_def, edit) = can_gather(world, &hit, player.config.interaction_reach)?;

    let mut transaction = VoxelEditTransaction::new();
    transaction.add_edit(edit);

    // Preflight validation pada level transaksi CSG
    transaction.validate(&world.store)?;

    Ok((res_def, transaction))
}

/// Mengeksekusi transaksi gathering secara atomik:
/// - Mengomit mutasi penghapusan voxel ke dunia melalui pipeline CSG/struktural/fisika.
/// - Hanya jika komit berhasil, memproduksi `CollectionResult`.
///
/// JAMINAN ATOMIK: Jika komit transaksi CSG gagal, tidak ada mutasi dunia dan tidak ada CollectionResult.
pub fn execute_gather_transaction(
    world: &mut World,
    target_coord: IVec3,
    resource_def: &ResourceDefinition,
    transaction: &VoxelEditTransaction,
) -> Result<GatheringResult, GatheringError> {
    let quantity = calculate_yield(resource_def);

    // Eksekusi mutasi dunia via pipeline interaksi otoritatif (Phase 11.2)
    let mutation = execute_interaction_transaction(world, transaction)?;

    let collection = CollectionResult {
        source_coord: target_coord,
        resource_id: resource_def.resource_id.clone(),
        quantity,
    };

    Ok(GatheringResult {
        collection,
        mutation,
    })
}

/// Menangani alur gathering pemain dari input look_direction hingga mutasi dunia dan hasil panen dengan cooldown debounce.
pub fn handle_player_gather(
    world: &mut World,
    player: &PlayerController,
    look_direction: Vec3,
    cooldown: &mut InteractionCooldown,
) -> Result<GatheringResult, GatheringError> {
    // 1. Periksa cooldown debounce
    if !cooldown.can_act() {
        return Err(GatheringError::CooldownActive {
            remaining: cooldown.timer,
        });
    }

    // 2. Validasi aksi gathering dan bangun transaksi CSG
    let (resource_def, transaction) = validate_gather_action(world, player, look_direction)?;

    // Ambil koordinat target dari proposal edit transaksi
    let target_coord = transaction
        .edits()
        .first()
        .map(|e| e.position)
        .ok_or(GatheringError::NoTargetHit)?;

    // 3. Eksekusi transaksi gathering secara atomik
    let result = execute_gather_transaction(world, target_coord, &resource_def, &transaction)?;

    // 4. Picu timer cooldown setelah berhasil dieksekusi
    cooldown.trigger();

    Ok(result)
}
