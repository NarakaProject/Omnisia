use glam::{IVec3, Vec3};
use std::collections::HashMap;

use super::raycast::raycast_player_interaction;
use super::types::{
    BlockOrientation, InteractionCooldown, PlacementError, PlacementProposal,
    PlacementRejectionReason, PlacementResult, PlacementValidity, VoxelHit, VoxelMutationResult,
    VoxelRaycastResult,
};
use crate::coord::{world_voxel_to_chunk_and_local, world_voxel_to_world_pos};
use crate::csg::edit::VoxelEdit;
use crate::csg::transaction::VoxelEditTransaction;
use crate::material::{MaterialId, MaterialRegistry};
use crate::mesh::types::FaceDirection;
use crate::modding::definitions::SupportRule;
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::ResourceId;
use crate::player::PlayerController;
use crate::streaming::store::ChunkStore;
use crate::voxel::{VoxelBlock, VOXEL_SIZE};
use crate::world::World;

/// Definisi aturan penempatan untuk suatu jenis blok (Phase 11.4).
///
/// Merupakan derived lookup cache dari `BlockDefinition` dalam data modding.
/// Menjamin bahwa `BlockDefinition` tetap menjadi sumber kebenaran data otoritatif tunggal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRuleDefinition {
    /// ResourceId blok asal
    pub block_id: ResourceId,
    /// Apakah blok membutuhkan penopang fisik saat ditempatkan
    pub requires_support: bool,
    /// Aturan penopang yang disyaratkan jika `requires_support == true`
    pub support_rule: SupportRule,
    /// Batasan orientasi opsional (None = semua orientasi diizinkan)
    pub allowed_orientations: Option<Vec<BlockOrientation>>,
}

impl BuildRuleDefinition {
    /// Membuat aturan penempatan baru dengan nilai default
    pub fn new(block_id: ResourceId) -> Self {
        Self {
            block_id,
            requires_support: true,
            support_rule: SupportRule::AnyAdjacent,
            allowed_orientations: None,
        }
    }

    /// Mengonfigurasi parameter penopang
    pub fn with_support(mut self, requires_support: bool, rule: SupportRule) -> Self {
        self.requires_support = requires_support;
        self.support_rule = rule;
        self
    }

    /// Mengonfigurasi batasan orientasi yang diizinkan
    pub fn with_allowed_orientations(mut self, orientations: Vec<BlockOrientation>) -> Self {
        self.allowed_orientations = Some(orientations);
        self
    }

    /// Apakah orientasi tertentu diizinkan untuk blok ini
    #[inline]
    pub fn is_orientation_allowed(&self, orientation: BlockOrientation) -> bool {
        if let Some(ref allowed) = self.allowed_orientations {
            allowed.contains(&orientation)
        } else {
            true
        }
    }
}

impl Default for BuildRuleDefinition {
    fn default() -> Self {
        Self {
            block_id: ResourceId::core("unknown").expect("valid default resource id"),
            requires_support: true,
            support_rule: SupportRule::AnyAdjacent,
            allowed_orientations: None,
        }
    }
}

/// Registri aturan penempatan dan pembangunan balok (Phase 11.4).
///
/// Menghubungkan identitas blok semantik (`ResourceId`) dan material runtime (`MaterialId`)
/// ke aturan pembangunan yang tervalidasi.
#[derive(Debug, Clone, Default)]
pub struct BuildRuleRegistry {
    /// Pemetaan otoritatif dari ResourceId blok ke BuildRuleDefinition
    by_block: HashMap<ResourceId, BuildRuleDefinition>,
    /// Indeks turunan dari MaterialId ke ResourceId blok untuk resolusi cepat
    material_to_block: HashMap<MaterialId, ResourceId>,
}

impl BuildRuleRegistry {
    /// Membuat registri aturan penempatan baru yang kosong
    pub fn new() -> Self {
        Self {
            by_block: HashMap::new(),
            material_to_block: HashMap::new(),
        }
    }

    /// Membangun registri aturan penempatan dengan memindai `BlockRegistry` dan `MaterialRegistry`
    pub fn from_registries(materials: &MaterialRegistry, blocks: &BlockRegistry) -> Self {
        let mut registry = Self::new();
        let mut ambiguous_materials = std::collections::HashSet::new();

        for (_block_id, block_def) in blocks.iter() {
            let mut rule = BuildRuleDefinition::new(block_def.id.clone());

            if let Some(ref build_comp) = block_def.components.build {
                rule.requires_support = build_comp.requires_support;
                rule.support_rule = build_comp.support_rule;
                rule.allowed_orientations = build_comp.allowed_orientations.clone();
            }

            if let Some(mat_id) = materials.resolve_material_id(&block_def.material) {
                match registry.material_to_block.entry(mat_id) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(block_def.id.clone());
                    }
                    std::collections::hash_map::Entry::Occupied(_) => {
                        // Material dipakai oleh lebih dari 1 blok -> ambigu!
                        ambiguous_materials.insert(mat_id);
                    }
                }
            }

            registry.by_block.insert(block_def.id.clone(), rule);
        }

        // Hapus pemetaan material yang ambigu agar tidak salah mengasumsikan 1-to-1
        for mat_id in ambiguous_materials {
            registry.material_to_block.remove(&mat_id);
        }

        registry
    }

    /// Mendaftarkan aturan pembangunan untuk blok secara manual (berguna untuk unit test)
    pub fn register(&mut self, material: MaterialId, rule: BuildRuleDefinition) {
        self.material_to_block
            .insert(material, rule.block_id.clone());
        self.by_block.insert(rule.block_id.clone(), rule);
    }

    /// Mengambil aturan penempatan berdasarkan ResourceId blok
    pub fn get_by_block(&self, id: &ResourceId) -> Option<&BuildRuleDefinition> {
        self.by_block.get(id)
    }

    /// Mengambil aturan penempatan berdasarkan MaterialId
    pub fn get_by_material(&self, material: MaterialId) -> Option<&BuildRuleDefinition> {
        self.material_to_block
            .get(&material)
            .and_then(|id| self.by_block.get(id))
    }

    /// Menyelesaikan aturan penempatan: mencari via block_id terlebih dahulu, kemudian material_id,
    /// dan mengembalikan default jika tidak ditemukan.
    pub fn resolve_rule(
        &self,
        block_id: Option<&ResourceId>,
        material: MaterialId,
    ) -> BuildRuleDefinition {
        if let Some(id) = block_id {
            if let Some(rule) = self.by_block.get(id) {
                return rule.clone();
            }
        }

        if let Some(rule) = self.get_by_material(material) {
            return rule.clone();
        }

        // Fallback default backward-compatible
        let id = block_id
            .cloned()
            .unwrap_or_else(|| ResourceId::core("default_block").expect("valid id"));
        BuildRuleDefinition::new(id)
    }
}

/// Memvalidasi penopang struktural spasial lokal untuk penempatan balok (Phase 11.4).
///
/// INVARIANTS:
/// 1. Jika `requires_support == false`, `rule` diabaikan sepenuhnya dan validasi sukses.
/// 2. Lokasi penopang yang belum termuat di memori menghasilkan `SupportNotResident`.
/// 3. Tidak ada pemindaian global atau rekursi; hanya evaluasi tetangga lokal (O(1)).
pub fn validate_support(
    store: &ChunkStore,
    target_coord: IVec3,
    candidate_coord: IVec3,
    requires_support: bool,
    rule: SupportRule,
) -> Result<(), PlacementError> {
    if !requires_support || rule == SupportRule::None {
        return Ok(());
    }

    match rule {
        SupportRule::None => Ok(()),
        SupportRule::AttachmentFace => {
            let (target_chunk, _) = world_voxel_to_chunk_and_local(target_coord);
            if !store.is_chunk_resident(&target_chunk) {
                return Err(PlacementError::SupportNotResident {
                    coord: target_coord,
                });
            }
            let block = store.get_voxel_world_checked(target_coord).ok_or(
                PlacementError::SupportNotResident {
                    coord: target_coord,
                },
            )?;
            if block.is_air() {
                return Err(PlacementError::SupportMissing {
                    coord: candidate_coord,
                    rule: SupportRule::AttachmentFace,
                });
            }
            Ok(())
        }
        SupportRule::FloorOnly => {
            let floor_coord = candidate_coord + IVec3::new(0, -1, 0);
            let (floor_chunk, _) = world_voxel_to_chunk_and_local(floor_coord);
            if !store.is_chunk_resident(&floor_chunk) {
                return Err(PlacementError::SupportNotResident { coord: floor_coord });
            }
            let block = store
                .get_voxel_world_checked(floor_coord)
                .ok_or(PlacementError::SupportNotResident { coord: floor_coord })?;
            if block.is_air() {
                return Err(PlacementError::SupportMissing {
                    coord: candidate_coord,
                    rule: SupportRule::FloorOnly,
                });
            }
            Ok(())
        }
        SupportRule::AnyAdjacent => {
            let mut unresident_neighbor = None;
            for face in FaceDirection::ALL {
                let neighbor_coord = candidate_coord + face.normal_ivec3();
                let (chunk_coord, _) = world_voxel_to_chunk_and_local(neighbor_coord);
                if !store.is_chunk_resident(&chunk_coord) {
                    if unresident_neighbor.is_none() {
                        unresident_neighbor = Some(neighbor_coord);
                    }
                    continue;
                }
                if let Some(block) = store.get_voxel_world_checked(neighbor_coord) {
                    if !block.is_air() {
                        // Ditemukan tetangga solid yang resident
                        return Ok(());
                    }
                }
            }

            // Jika tidak ada tetangga solid, prioritaskan melaporkan non-resident jika ada
            if let Some(unresident_coord) = unresident_neighbor {
                Err(PlacementError::SupportNotResident {
                    coord: unresident_coord,
                })
            } else {
                Err(PlacementError::SupportMissing {
                    coord: candidate_coord,
                    rule: SupportRule::AnyAdjacent,
                })
            }
        }
    }
}

/// Melakukan validasi preflight penempatan balok baru secara otoritatif dan read-only (Phase 11.4).
///
/// Syarat Validasi:
/// 1. Jarak benturan raycast tidak melampaui `max_reach` (inklusif).
/// 2. Material yang hendak ditempatkan valid (bukan AIR).
/// 3. Chunk target resident di ChunkStore dan voxel target berupa balok solid non-air.
/// 4. Koordinat kandidat dihitung dari `target_coord + hit.face.normal_ivec3()`.
/// 5. Chunk kandidat resident di ChunkStore dan saat ini berupa udara (AIR).
/// 6. Aturan penopang (`SupportRule`) terpenuhi oleh tetangga resident.
/// 7. Volume AABB kandidat tidak beririsan dengan kapsul pemain (`player.current_capsule()`).
#[allow(clippy::too_many_arguments)]
pub fn can_place_voxel(
    store: &ChunkStore,
    rules: &BuildRuleRegistry,
    player: &PlayerController,
    hit: &VoxelHit,
    material: MaterialId,
    block_id: Option<&ResourceId>,
    orientation: BlockOrientation,
    max_reach: f32,
) -> Result<VoxelEdit, PlacementError> {
    // 1. Validasi reach
    if hit.distance > max_reach {
        return Err(PlacementError::ExceedsReach {
            distance: hit.distance,
            max_reach,
        });
    }

    // 2. Validasi material (bukan AIR)
    if material.0 == 0 {
        return Err(PlacementError::InvalidMaterial(
            "Cannot place VoxelBlock::AIR as a solid block".to_string(),
        ));
    }

    // 3. Validasi residency dan solidness target
    let (target_chunk, _) = world_voxel_to_chunk_and_local(hit.voxel_coord);
    if !store.is_chunk_resident(&target_chunk) {
        return Err(PlacementError::TargetNotResident {
            coord: hit.voxel_coord,
        });
    }

    let target_block = store.get_voxel_world_checked(hit.voxel_coord).ok_or(
        PlacementError::TargetNotResident {
            coord: hit.voxel_coord,
        },
    )?;
    if target_block.is_air() {
        return Err(PlacementError::TargetIsAir {
            coord: hit.voxel_coord,
        });
    }

    // 4. Hitung koordinat kandidat penempatan
    let candidate_coord = hit.voxel_coord + hit.face.normal_ivec3();

    // 5. Validasi residency dan okupansi kandidat
    let (cand_chunk, _) = world_voxel_to_chunk_and_local(candidate_coord);
    if !store.is_chunk_resident(&cand_chunk) {
        return Err(PlacementError::CandidateNotResident {
            coord: candidate_coord,
        });
    }

    let cand_block = store.get_voxel_world_checked(candidate_coord).ok_or(
        PlacementError::CandidateNotResident {
            coord: candidate_coord,
        },
    )?;
    if !cand_block.is_air() {
        return Err(PlacementError::CandidateOccupied {
            coord: candidate_coord,
            current_material: cand_block.material(),
        });
    }

    // 6. Validasi aturan penopang (SupportRule) dan batasan orientasi
    let rule = rules.resolve_rule(block_id, material);
    if !rule.is_orientation_allowed(orientation) {
        return Err(PlacementError::InvalidOrientation(format!(
            "Orientation {:?} is not allowed for block {:?}",
            orientation, rule.block_id
        )));
    }

    validate_support(
        store,
        hit.voxel_coord,
        candidate_coord,
        rule.requires_support,
        rule.support_rule,
    )?;

    // 7. Player Capsule Clearance Guard
    let aabb_min = world_voxel_to_world_pos(candidate_coord);
    let aabb_max = aabb_min + Vec3::splat(VOXEL_SIZE);
    let capsule = player.current_capsule();

    if capsule.intersects_aabb(aabb_min, aabb_max) {
        return Err(PlacementError::PlayerCapsuleOverlap {
            coord: candidate_coord,
        });
    }

    Ok(VoxelEdit::add(candidate_coord, VoxelBlock::new(material)))
}

/// Menghasilkan proposal semantik penempatan balok (Phase 11.4).
///
/// JAMINAN:
/// 1. Murni read-only.
/// 2. Tidak memutasi ChunkStore atau World.
/// 3. Tidak memicu chunk loading, generation, disk I/O, atau interaksi renderer GPU.
/// 4. Berstatus derived state untuk dikonsumsi sistem input, pengujian, atau UI masa depan.
#[allow(clippy::too_many_arguments)]
pub fn build_placement_proposal(
    store: &ChunkStore,
    rules: &BuildRuleRegistry,
    player: &PlayerController,
    hit_result: &VoxelRaycastResult,
    material: MaterialId,
    block_id: Option<ResourceId>,
    orientation: BlockOrientation,
    max_reach: f32,
) -> PlacementProposal {
    match hit_result {
        VoxelRaycastResult::Miss => PlacementProposal {
            target_voxel: IVec3::ZERO,
            candidate_voxel: IVec3::ZERO,
            target_face: FaceDirection::PosY,
            orientation,
            material,
            block_id,
            validity: PlacementValidity::Invalid(PlacementRejectionReason::NoTargetHit),
        },
        VoxelRaycastResult::NonResident {
            voxel_coord, face, ..
        } => PlacementProposal {
            target_voxel: *voxel_coord,
            candidate_voxel: *voxel_coord + face.normal_ivec3(),
            target_face: *face,
            orientation,
            material,
            block_id,
            validity: PlacementValidity::Invalid(PlacementRejectionReason::TargetNotResident {
                coord: *voxel_coord,
            }),
        },
        VoxelRaycastResult::Hit(hit) => {
            let candidate_voxel = hit.voxel_coord + hit.face.normal_ivec3();

            let validity = match can_place_voxel(
                store,
                rules,
                player,
                hit,
                material,
                block_id.as_ref(),
                orientation,
                max_reach,
            ) {
                Ok(_) => PlacementValidity::Valid,
                Err(e) => {
                    let reason = match e {
                        PlacementError::NoTargetHit => PlacementRejectionReason::NoTargetHit,
                        PlacementError::TargetNotResident { coord } => {
                            PlacementRejectionReason::TargetNotResident { coord }
                        }
                        PlacementError::CandidateNotResident { coord } => {
                            PlacementRejectionReason::CandidateNotResident { coord }
                        }
                        PlacementError::ExceedsReach {
                            distance,
                            max_reach,
                        } => PlacementRejectionReason::ExceedsReach {
                            distance,
                            max_reach,
                        },
                        PlacementError::TargetIsAir { coord } => {
                            PlacementRejectionReason::TargetIsAir { coord }
                        }
                        PlacementError::CandidateOccupied {
                            coord,
                            current_material,
                        } => PlacementRejectionReason::CandidateOccupied {
                            coord,
                            current_material,
                        },
                        PlacementError::PlayerCapsuleOverlap { coord } => {
                            PlacementRejectionReason::PlayerCapsuleOverlap { coord }
                        }
                        PlacementError::InvalidMaterial(msg) => {
                            PlacementRejectionReason::InvalidMaterial(msg)
                        }
                        PlacementError::SupportMissing { coord, rule } => {
                            PlacementRejectionReason::SupportMissing { coord, rule }
                        }
                        PlacementError::SupportNotResident { coord } => {
                            PlacementRejectionReason::SupportNotResident { coord }
                        }
                        PlacementError::InvalidOrientation(msg) => {
                            PlacementRejectionReason::InvalidOrientation(msg)
                        }
                        PlacementError::CooldownActive { remaining } => {
                            PlacementRejectionReason::CooldownActive { remaining }
                        }
                        PlacementError::TransactionError(msg) => {
                            PlacementRejectionReason::TransactionError(msg.to_string())
                        }
                        PlacementError::MutationError(msg) => {
                            PlacementRejectionReason::TransactionError(msg.to_string())
                        }
                        PlacementError::StaleProposal(msg) => {
                            PlacementRejectionReason::StaleProposal { reason: msg }
                        }
                    };
                    PlacementValidity::Invalid(reason)
                }
            };

            PlacementProposal {
                target_voxel: hit.voxel_coord,
                candidate_voxel,
                target_face: hit.face,
                orientation,
                material,
                block_id,
                validity,
            }
        }
    }
}

/// Memvalidasi ulang proposal penempatan balok terhadap kondisi dunia otoritatif terkini (Phase 11.4).
///
/// INVARIANT:
/// Mencegah proposal usang (stale proposal) melewati validasi jika dunia atau posisi pemain
/// telah berubah sejak proposal dibuat.
pub fn validate_placement_proposal(
    store: &ChunkStore,
    rules: &BuildRuleRegistry,
    player: &PlayerController,
    proposal: &PlacementProposal,
    max_reach: f32,
) -> Result<VoxelEdit, PlacementError> {
    // 0. Periksa validitas proposal awal
    if let PlacementValidity::Invalid(ref reason) = proposal.validity {
        return Err(reason.clone().into());
    }

    // 1. Validasi reach dari mata pemain terkini ke target voxel (jarak terdekat ke kubus target)
    let eye_pos = player.eye_position();
    let aabb_min = world_voxel_to_world_pos(proposal.target_voxel);
    let aabb_max = aabb_min + Vec3::splat(VOXEL_SIZE);
    let closest_point = eye_pos.clamp(aabb_min, aabb_max);
    let distance = (closest_point - eye_pos).length();

    if distance > max_reach {
        return Err(PlacementError::ExceedsReach {
            distance,
            max_reach,
        });
    }

    // 2. Validasi material (bukan AIR)
    if proposal.material.0 == 0 {
        return Err(PlacementError::InvalidMaterial(
            "Cannot place VoxelBlock::AIR as a solid block".to_string(),
        ));
    }

    // 3. Validasi residency dan isi target voxel saat ini
    let (target_chunk, _) = world_voxel_to_chunk_and_local(proposal.target_voxel);
    if !store.is_chunk_resident(&target_chunk) {
        return Err(PlacementError::TargetNotResident {
            coord: proposal.target_voxel,
        });
    }
    let target_block = store.get_voxel_world_checked(proposal.target_voxel).ok_or(
        PlacementError::TargetNotResident {
            coord: proposal.target_voxel,
        },
    )?;
    if target_block.is_air() {
        return Err(PlacementError::TargetIsAir {
            coord: proposal.target_voxel,
        });
    }

    // 4. Validasi integritas koordinat kandidat terhadap target face
    let expected_candidate = proposal.target_voxel + proposal.target_face.normal_ivec3();
    if proposal.candidate_voxel != expected_candidate {
        return Err(PlacementError::StaleProposal(format!(
            "Candidate voxel {:?} does not match target {:?} + face {:?}",
            proposal.candidate_voxel, proposal.target_voxel, proposal.target_face
        )));
    }

    // 5. Validasi residency dan okupansi kandidat saat ini (HARUS TETAP AIR)
    let (cand_chunk, _) = world_voxel_to_chunk_and_local(proposal.candidate_voxel);
    if !store.is_chunk_resident(&cand_chunk) {
        return Err(PlacementError::CandidateNotResident {
            coord: proposal.candidate_voxel,
        });
    }
    let cand_block = store
        .get_voxel_world_checked(proposal.candidate_voxel)
        .ok_or(PlacementError::CandidateNotResident {
            coord: proposal.candidate_voxel,
        })?;
    if !cand_block.is_air() {
        return Err(PlacementError::CandidateOccupied {
            coord: proposal.candidate_voxel,
            current_material: cand_block.material(),
        });
    }

    // 6. Validasi aturan penopang dan batasan orientasi saat ini
    let rule = rules.resolve_rule(proposal.block_id.as_ref(), proposal.material);
    if !rule.is_orientation_allowed(proposal.orientation) {
        return Err(PlacementError::InvalidOrientation(format!(
            "Orientation {:?} is not allowed for block {:?}",
            proposal.orientation, rule.block_id
        )));
    }

    validate_support(
        store,
        proposal.target_voxel,
        proposal.candidate_voxel,
        rule.requires_support,
        rule.support_rule,
    )?;

    // 7. Validasi ulang irisan kapsul tabrakan pemain terkini
    let cand_min = world_voxel_to_world_pos(proposal.candidate_voxel);
    let cand_max = cand_min + Vec3::splat(VOXEL_SIZE);
    let capsule = player.current_capsule();

    if capsule.intersects_aabb(cand_min, cand_max) {
        return Err(PlacementError::PlayerCapsuleOverlap {
            coord: proposal.candidate_voxel,
        });
    }

    Ok(VoxelEdit::add(
        proposal.candidate_voxel,
        VoxelBlock::new(proposal.material),
    ))
}

/// Mengeksekusi transaksi penempatan balok ke dalam dunia otoritatif (`World`).
pub fn execute_placement_transaction(
    world: &mut World,
    transaction: &VoxelEditTransaction,
    proposal: PlacementProposal,
) -> Result<PlacementResult, PlacementError> {
    let (commit_result, newly_detached_aggregates) = world
        .commit_voxel_transaction(transaction)
        .map_err(PlacementError::TransactionError)?;

    Ok(PlacementResult {
        proposal,
        mutation: VoxelMutationResult {
            commit_result,
            newly_detached_aggregates,
        },
    })
}

/// Menangani alur penempatan balok pemain dari input hingga komit atomik ke dunia.
pub fn handle_player_placement(
    world: &mut World,
    player: &PlayerController,
    look_direction: Vec3,
    material: MaterialId,
    block_id: Option<ResourceId>,
    orientation: BlockOrientation,
    cooldown: &mut InteractionCooldown,
) -> Result<PlacementResult, PlacementError> {
    // 1. Periksa cooldown debounce
    if !cooldown.can_act() {
        return Err(PlacementError::CooldownActive {
            remaining: cooldown.timer,
        });
    }

    // 2. Raycast dari sudut pandang pemain (Phase 11.1)
    let hit_result = raycast_player_interaction(&world.store, player, look_direction);

    // 3. Bangun proposal semantik
    let proposal = build_placement_proposal(
        &world.store,
        &world.build_rules,
        player,
        &hit_result,
        material,
        block_id,
        orientation,
        player.config.interaction_reach,
    );

    // 4. Validasi ulang proposal secara otoritatif
    let edit = validate_placement_proposal(
        &world.store,
        &world.build_rules,
        player,
        &proposal,
        player.config.interaction_reach,
    )?;

    // 5. Bangun transaksi CSG atomik
    let mut transaction = VoxelEditTransaction::new();
    transaction.add_edit(edit);
    transaction.validate(&world.store)?;

    // 6. Eksekusi transaksi ke dunia
    let result = execute_placement_transaction(world, &transaction, proposal)?;

    // 7. Picu cooldown HANYA setelah penempatan berhasil
    cooldown.trigger();

    Ok(result)
}
