use glam::Vec3;

use super::raycast::raycast_player_interaction;
use super::types::{
    InteractionAction, InteractionCooldown, InteractionMutationError, VoxelHit, VoxelMutationResult,
};
use crate::coord::{world_voxel_to_chunk_and_local, world_voxel_to_world_pos};
use crate::csg::edit::VoxelEdit;
use crate::csg::transaction::VoxelEditTransaction;
use crate::material::MaterialId;
use crate::player::PlayerController;
use crate::streaming::store::ChunkStore;
use crate::voxel::{VoxelBlock, VOXEL_SIZE};
use crate::world::World;

/// Memvalidasi proposal penghapusan voxel solid dari dunia (Phase 11.2).
///
/// Syarat Validasi:
/// 1. Jarak benturan raycast tidak melampaui `max_reach` (inklusif).
/// 2. Chunk target harus berstatus resident di dalam memori `ChunkStore`.
/// 3. Voxel pada koordinat target harus berupa balok solid non-air.
///
/// JAMINAN: Fungsi ini murni read-only dan tidak memutasi ChunkStore.
pub fn can_remove(
    store: &ChunkStore,
    hit: &VoxelHit,
    max_reach: f32,
) -> Result<VoxelEdit, InteractionMutationError> {
    // 1. Validasi reach
    if hit.distance > max_reach {
        return Err(InteractionMutationError::ExceedsReach {
            distance: hit.distance,
            max_reach,
        });
    }

    // 2. Validasi residency chunk target
    let (chunk_coord, _) = world_voxel_to_chunk_and_local(hit.voxel_coord);
    if !store.is_chunk_resident(&chunk_coord) {
        return Err(InteractionMutationError::TargetNotResident {
            coord: hit.voxel_coord,
        });
    }

    // 3. Validasi isi voxel target (harus solid, bukan air)
    let block = store.get_voxel_world_checked(hit.voxel_coord).ok_or(
        InteractionMutationError::TargetNotResident {
            coord: hit.voxel_coord,
        },
    )?;

    if block.is_air() {
        return Err(InteractionMutationError::RemovalTargetIsAir {
            coord: hit.voxel_coord,
        });
    }

    Ok(VoxelEdit::remove(hit.voxel_coord))
}

/// Memvalidasi proposal penempatan voxel solid baru bersebelahan dengan sisi target (Phase 11.2).
///
/// Syarat Validasi:
/// 1. Jarak benturan raycast tidak melampaui `max_reach` (inklusif).
/// 2. Material yang hendak ditempatkan valid (bukan material AIR).
/// 3. Menghitung koordinat kandidat penempatan tepat di depan sisi target menggunakan normal kubus.
/// 4. Chunk tempat koordinat kandidat berada harus berstatus resident di dalam memori `ChunkStore`.
/// 5. Koordinat kandidat saat ini harus kosong / berupa udara (air).
/// 6. Volume AABB voxel kandidat TIDAK BOLEH beririsan dengan kapsul tabrakan pemain (`PlayerCapsuleOverlap`).
///
/// JAMINAN: Fungsi ini murni read-only dan tidak memutasi ChunkStore atau posisi pemain.
pub fn can_place(
    store: &ChunkStore,
    hit: &VoxelHit,
    material: MaterialId,
    player: &PlayerController,
    max_reach: f32,
) -> Result<VoxelEdit, InteractionMutationError> {
    // 1. Validasi reach
    if hit.distance > max_reach {
        return Err(InteractionMutationError::ExceedsReach {
            distance: hit.distance,
            max_reach,
        });
    }

    // 2. Validasi material
    if material.0 == 0 {
        return Err(InteractionMutationError::InvalidMaterial(
            "Cannot place VoxelBlock::AIR as a solid block".to_string(),
        ));
    }

    // 3. Hitung koordinat kandidat di depan sisi kubus yang terkena ray
    let normal_offset = hit.face.normal_ivec3();
    let candidate_coord = hit.voxel_coord + normal_offset;

    // 4. Validasi residency chunk tujuan penempatan
    let (cand_chunk, _) = world_voxel_to_chunk_and_local(candidate_coord);
    if !store.is_chunk_resident(&cand_chunk) {
        return Err(InteractionMutationError::DestinationNotResident {
            coord: candidate_coord,
        });
    }

    // 5. Validasi okupansi (koordinat tujuan harus berupa air)
    let current_block = store.get_voxel_world_checked(candidate_coord).ok_or(
        InteractionMutationError::DestinationNotResident {
            coord: candidate_coord,
        },
    )?;

    if !current_block.is_air() {
        return Err(InteractionMutationError::PlacementOccupied {
            coord: candidate_coord,
            current: current_block,
        });
    }

    // 6. PLAYER CAPSULE OVERLAP GUARD: Menolak penempatan yang menabrak kapsul pemain
    let aabb_min = world_voxel_to_world_pos(candidate_coord);
    let aabb_max = aabb_min + Vec3::splat(VOXEL_SIZE);
    let capsule = player.current_capsule();

    if capsule.intersects_aabb(aabb_min, aabb_max) {
        return Err(InteractionMutationError::PlayerCapsuleOverlap {
            coord: candidate_coord,
        });
    }

    Ok(VoxelEdit::add(candidate_coord, VoxelBlock::new(material)))
}

/// Melakukan validasi menyeluruh terhadap proposal aksi interaksi pemain dan membangun transaksi CSG.
///
/// Jika validasi berhasil, mengembalikan `VoxelEditTransaction` yang telah teruji dan siap dieksekusi.
/// Jika validasi gagal, mengembalikan `InteractionMutationError` tanpa menimbulkan efek samping apa pun.
pub fn validate_interaction_action(
    store: &ChunkStore,
    player: &PlayerController,
    look_direction: Vec3,
    action: InteractionAction,
) -> Result<VoxelEditTransaction, InteractionMutationError> {
    let result = raycast_player_interaction(store, player, look_direction);
    let hit = match result {
        crate::interaction::types::VoxelRaycastResult::Hit(h) => h,
        _ => return Err(InteractionMutationError::NoTargetHit),
    };

    let edit = match action {
        InteractionAction::RemoveVoxel => can_remove(store, &hit, player.config.interaction_reach)?,
        InteractionAction::PlaceVoxel { material } => can_place(
            store,
            &hit,
            material,
            player,
            player.config.interaction_reach,
        )?,
    };

    let mut transaction = VoxelEditTransaction::new();
    transaction.add_edit(edit);

    // Verifikasi preflight pada level transaksi CSG untuk menjamin konsistensi multi-chunk
    transaction.validate(store)?;

    Ok(transaction)
}

/// Mengeksekusi transaksi mutasi voxel yang telah tervalidasi ke dalam dunia permainan (`World`).
///
/// Alur Eksekusi Otoritatif:
/// 1. Komit mutasi secara atomik ke `ChunkStore` via `world.commit_voxel_transaction`.
/// 2. Downstream rekonsiliasi struktural (`StructuralSystem`).
/// 3. Ekstraksi gugusan lepas ke `PhysicsRuntime` (`DynamicBody`).
/// 4. Invalidation dirty mesh chunks untuk `ChunkScheduler`.
pub fn execute_interaction_transaction(
    world: &mut World,
    transaction: &VoxelEditTransaction,
) -> Result<VoxelMutationResult, InteractionMutationError> {
    let (commit_result, newly_detached_aggregates) = world
        .commit_voxel_transaction(transaction)
        .map_err(InteractionMutationError::TransactionError)?;

    Ok(VoxelMutationResult {
        commit_result,
        newly_detached_aggregates,
    })
}

/// Menangani alur interaksi pemain dari input hingga eksekusi mutasi dunia secara utuh dengan cooldown debounce.
pub fn handle_player_interaction(
    world: &mut World,
    player: &PlayerController,
    look_direction: Vec3,
    action: InteractionAction,
    cooldown: &mut InteractionCooldown,
) -> Result<VoxelMutationResult, InteractionMutationError> {
    // 1. Periksa cooldown debounce
    if !cooldown.can_act() {
        return Err(InteractionMutationError::CooldownActive {
            remaining: cooldown.timer,
        });
    }

    // 2. Validasi aksi interaksi dan bangun transaksi atomik
    let transaction = validate_interaction_action(&world.store, player, look_direction, action)?;

    // 3. Eksekusi transaksi ke dunia otoritatif
    let result = execute_interaction_transaction(world, &transaction)?;

    // 4. Picu timer cooldown setelah berhasil dieksekusi
    cooldown.trigger();

    Ok(result)
}
