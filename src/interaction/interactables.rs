use glam::{IVec3, Vec3};
use std::collections::HashMap;

use crate::coord::world_voxel_to_chunk_and_local;
use crate::material::{MaterialId, MaterialRegistry};
use crate::modding::definitions::InteractableId;
use crate::modding::registry::BlockRegistry;
use crate::modding::resource_id::ResourceId;
use crate::player::PlayerController;
use crate::world::World;

use super::raycast::raycast_player_interaction;
use super::types::{
    AudioCue, FeedbackId, InteractableAction, InteractableDefinition, InteractableState,
    InteractableTarget, InteractionCooldown, InteractionError, InteractionFeedback,
    InteractionProposal, InteractionResult, VisualCue, VoxelHit, VoxelRaycastResult,
};

/// Keberadaan runtime instance dari objek interaktif di dunia (Phase 11.6).
/// INVARIANT: Session-only di memori, tidak dipersistensi langsung ke disk pada Phase 11.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractableInstance {
    /// Identitas interactable yang diasosiasikan dengan instance ini
    pub interactable_id: InteractableId,
    /// Material yang diharapkan untuk validasi integritas / konsistensi stale state
    pub expected_material: MaterialId,
    /// Status semantik runtime saat ini
    pub state: InteractableState,
}

/// Registri runtime untuk objek interaktif dunia (Phase 11.6).
/// INVARIANT GUARDRAILS:
/// 1. Derived / cache lookup layer only dari BlockDefinition.
/// 2. BUKAN otoritas konten independen (otoritas ada di `BlockDefinition.components.interactable`).
/// 3. Tidak menggunakan MaterialId sebagai identitas objek tunggal.
#[derive(Debug, Clone, Default)]
pub struct InteractableRegistry {
    /// Definisi interactable yang terdaftar berdasarkan InteractableId
    pub definitions: HashMap<InteractableId, InteractableDefinition>,
    /// Pemetaan cepat dari ResourceId blok ke InteractableId
    pub block_to_interactable: HashMap<ResourceId, InteractableId>,
    /// Pemetaan koordinat voxel dunia ke InteractableId objek yang ditempatkan
    pub placed: HashMap<IVec3, InteractableId>,
    /// Status runtime instance aktif per koordinat voxel dunia
    pub instances: HashMap<IVec3, InteractableInstance>,
}

impl InteractableRegistry {
    /// Membuat registri interactable kosong
    pub fn new() -> Self {
        Self::default()
    }

    /// Membangun registri interactable dengan memindai `BlockRegistry` dan memetakan ke `MaterialRegistry`
    /// berdasarkan `BlockComponents::interactable` yang didefinisikan dalam data Core / Mod.
    pub fn from_registries(materials: &MaterialRegistry, blocks: &BlockRegistry) -> Self {
        let mut registry = Self::new();

        for (_block_res_id, block_def) in blocks.iter() {
            if let Some(ref inter_comp) = block_def.components.interactable {
                if let Some(mat_id) = materials.resolve_material_id(&block_def.material) {
                    let def = InteractableDefinition {
                        id: inter_comp.id.clone(),
                        source_block: block_def.id.clone(),
                        expected_material: mat_id,
                        allowed_actions: inter_comp.allowed_actions.clone(),
                        initial_state: inter_comp.initial_state,
                        audio_cue: inter_comp.audio_cue,
                        visual_cue: inter_comp.visual_cue,
                    };
                    registry.register_definition(def);
                }
            }
        }

        registry
    }

    /// Mendaftarkan definisi interactable secara eksplisit ke dalam cache runtime
    pub fn register_definition(&mut self, def: InteractableDefinition) {
        self.block_to_interactable
            .insert(def.source_block.clone(), def.id.clone());
        self.definitions.insert(def.id.clone(), def);
    }

    /// Mengambil definisi interactable berdasarkan `InteractableId`
    #[inline(always)]
    pub fn resolve_definition(&self, id: &InteractableId) -> Option<&InteractableDefinition> {
        self.definitions.get(id)
    }

    /// Mengambil definisi interactable berdasarkan `ResourceId` blok
    #[inline(always)]
    pub fn resolve_block_interactable(
        &self,
        block_id: &ResourceId,
    ) -> Option<&InteractableDefinition> {
        self.block_to_interactable
            .get(block_id)
            .and_then(|id| self.definitions.get(id))
    }

    /// Menetapkan keberadaan interactable pada koordinat dunia tertentu
    pub fn set_interactable_at(&mut self, coord: IVec3, id: InteractableId) {
        self.placed.insert(coord, id);
    }

    /// Menghapus interactable dan instansinya dari koordinat dunia
    pub fn remove_interactable_at(&mut self, coord: &IVec3) -> Option<InteractableId> {
        self.instances.remove(coord);
        self.placed.remove(coord)
    }

    /// Mengambil `InteractableId` pada koordinat dunia tertentu jika ada
    #[inline(always)]
    pub fn get_interactable_at(&self, coord: &IVec3) -> Option<&InteractableId> {
        self.placed.get(coord)
    }

    /// Mengambil referensi runtime instance pada koordinat dunia jika ada
    #[inline(always)]
    pub fn get_instance(&self, coord: &IVec3) -> Option<&InteractableInstance> {
        self.instances.get(coord)
    }

    /// Mengatur status instance secara langsung (untuk inisialisasi / testing)
    pub fn set_instance_state(
        &mut self,
        coord: IVec3,
        id: InteractableId,
        material: MaterialId,
        state: InteractableState,
    ) {
        self.instances.insert(
            coord,
            InteractableInstance {
                interactable_id: id,
                expected_material: material,
                state,
            },
        );
    }
}

/// Menyaring daftar aksi yang diizinkan sesuai status saat ini berdasarkan aturan deterministik.
pub fn filter_available_actions(
    allowed: &[InteractableAction],
    current_state: InteractableState,
) -> Vec<InteractableAction> {
    allowed
        .iter()
        .copied()
        .filter(|&action| match (action, current_state) {
            // Examine selalu valid dalam status apa pun (termasuk Disabled)
            (InteractableAction::Examine, _) => true,
            // Jika Disabled, semua aksi mutasi tidak valid
            (_, InteractableState::Disabled) => false,
            // Activate: hanya valid dari Idle
            (InteractableAction::Activate, InteractableState::Idle) => true,
            (InteractableAction::Activate, _) => false,
            // Toggle: valid dari Idle, Active, Closed, Open
            (
                InteractableAction::Toggle,
                InteractableState::Idle
                | InteractableState::Active
                | InteractableState::Closed
                | InteractableState::Open,
            ) => true,
            // Open: hanya valid dari Closed
            (InteractableAction::Open, InteractableState::Closed) => true,
            (InteractableAction::Open, _) => false,
            // Close: hanya valid dari Open
            (InteractableAction::Close, InteractableState::Open) => true,
            (InteractableAction::Close, _) => false,
        })
        .collect()
}

/// Mengevaluasi transisi status semantik secara deterministik dan menghasilkan feedback semantik murni.
pub fn evaluate_transition(
    current_state: InteractableState,
    action: InteractableAction,
    def: &InteractableDefinition,
) -> Result<(InteractableState, InteractionFeedback), InteractionError> {
    // 1. Periksa Examine: selalu diizinkan tanpa merubah status
    if action == InteractableAction::Examine {
        let feedback = InteractionFeedback {
            audio_cue: None,
            visual_cue: None,
            feedback_id: Some(FeedbackId::Examined),
        };
        return Ok((current_state, feedback));
    }

    // 2. Periksa status Disabled untuk aksi mutasi
    if current_state == InteractableState::Disabled {
        return Err(InteractionError::ObjectDisabled {
            interactable_id: def.id.clone(),
        });
    }

    // 3. Evaluasi transisi terprogram
    let (target_state, feedback_id, default_audio, default_visual) = match (action, current_state) {
        (InteractableAction::Activate, InteractableState::Idle) => (
            InteractableState::Active,
            FeedbackId::Activated,
            AudioCue::Click,
            VisualCue::Pulse,
        ),
        (InteractableAction::Toggle, InteractableState::Idle) => (
            InteractableState::Active,
            FeedbackId::Activated,
            AudioCue::SwitchToggle,
            VisualCue::StateTransition,
        ),
        (InteractableAction::Toggle, InteractableState::Active) => (
            InteractableState::Idle,
            FeedbackId::Deactivated,
            AudioCue::SwitchToggle,
            VisualCue::StateTransition,
        ),
        (InteractableAction::Toggle, InteractableState::Closed) => (
            InteractableState::Open,
            FeedbackId::Opened,
            AudioCue::DoorOpen,
            VisualCue::StateTransition,
        ),
        (InteractableAction::Toggle, InteractableState::Open) => (
            InteractableState::Closed,
            FeedbackId::Closed,
            AudioCue::DoorClose,
            VisualCue::StateTransition,
        ),
        (InteractableAction::Open, InteractableState::Closed) => (
            InteractableState::Open,
            FeedbackId::Opened,
            AudioCue::DoorOpen,
            VisualCue::StateTransition,
        ),
        (InteractableAction::Close, InteractableState::Open) => (
            InteractableState::Closed,
            FeedbackId::Closed,
            AudioCue::DoorClose,
            VisualCue::StateTransition,
        ),
        _ => {
            return Err(InteractionError::InvalidActionForState {
                action,
                state: current_state,
            });
        }
    };

    let audio_cue = def.audio_cue.or(Some(default_audio));
    let visual_cue = def.visual_cue.or(Some(default_visual));
    let feedback = InteractionFeedback {
        audio_cue,
        visual_cue,
        feedback_id: Some(feedback_id),
    };

    Ok((target_state, feedback))
}

/// Mendeteksi target interaksi generik dari hasil benturan raycast.
///
/// INVARIANT: Murni read-only (`&World`).
/// Mendeteksi stale instance TIDAK PERNAH memutasi registri atau menghapus cache.
/// Stale instance diabaikan dan sistem menggunakan `def.initial_state`.
pub fn detect_interactable_target(
    world: &World,
    hit: &VoxelHit,
    max_reach: f32,
) -> Result<InteractableTarget, InteractionError> {
    // 1. Validasi jangkauan
    if hit.distance > max_reach {
        return Err(InteractionError::ExceedsReach {
            distance: hit.distance,
            max_reach,
        });
    }

    // 2. Validasi residency chunk
    let (chunk_coord, _) = world_voxel_to_chunk_and_local(hit.voxel_coord);
    if !world.store.is_chunk_resident(&chunk_coord) {
        return Err(InteractionError::TargetNotResident {
            coord: hit.voxel_coord,
        });
    }

    // 3. Validasi isi voxel
    let voxel = world.store.get_voxel_world_checked(hit.voxel_coord).ok_or(
        InteractionError::TargetNotResident {
            coord: hit.voxel_coord,
        },
    )?;

    if voxel.is_air() {
        return Err(InteractionError::TargetIsAir {
            coord: hit.voxel_coord,
        });
    }

    // 4. Resolusi identitas interactable pada koordinat
    let id = world
        .interactables
        .get_interactable_at(&hit.voxel_coord)
        .ok_or(InteractionError::NotInteractable {
            coord: hit.voxel_coord,
        })?;

    let def = world
        .interactables
        .resolve_definition(id)
        .ok_or_else(|| InteractionError::UnknownInteractable { id: id.clone() })?;

    // 5. Evaluasi status runtime instance (MURNI READ-ONLY, TANPA MUTASI)
    let current_state = if let Some(instance) = world.interactables.instances.get(&hit.voxel_coord)
    {
        // Stale-state invariant:
        // Valid jika dan hanya jika identitas interactable dan material keduanya cocok persis.
        if instance.interactable_id == def.id && instance.expected_material == voxel.material {
            instance.state
        } else {
            // Stale: abaikan instansi yang tidak valid tanpa memutasi cache
            def.initial_state
        }
    } else {
        def.initial_state
    };

    // 6. Saring aksi yang diizinkan dan tentukan aksi preferensi
    let available_actions = filter_available_actions(&def.allowed_actions, current_state);
    let preferred_action = available_actions.first().copied();

    Ok(InteractableTarget {
        interactable_id: def.id.clone(),
        source_block: def.source_block.clone(),
        expected_material: voxel.material,
        coord: hit.voxel_coord,
        current_state,
        available_actions,
        preferred_action,
    })
}

/// Melakukan kueri target interaksi dari sudut pandang kamera pemain.
///
/// INVARIANT: Murni read-only.
pub fn query_interactable_target(
    world: &World,
    player: &PlayerController,
    look_direction: Vec3,
) -> Result<InteractableTarget, InteractionError> {
    let hit_result = raycast_player_interaction(&world.store, player, look_direction);
    match hit_result {
        VoxelRaycastResult::Hit(hit) => {
            detect_interactable_target(world, &hit, player.config.interaction_reach)
        }
        VoxelRaycastResult::Miss => Err(InteractionError::NoTargetHit),
        VoxelRaycastResult::NonResident { voxel_coord, .. } => {
            Err(InteractionError::TargetNotResident { coord: voxel_coord })
        }
    }
}

/// Memvalidasi intensi interaksi dan membangun `InteractionProposal`.
///
/// INVARIANT: Murni read-only (`&World`), zero state mutation.
/// Menyimpan `expected_material` untuk revalidasi TOCTOU akhir sebelum eksekusi.
pub fn validate_interaction(
    world: &World,
    target: &InteractableTarget,
    action: InteractableAction,
    player_eye: Vec3,
    max_reach: f32,
) -> Result<InteractionProposal, InteractionError> {
    // 1. Validasi jarak Euclidean dari mata pemain ke pusat voxel target
    let aabb_min = crate::coord::world_voxel_to_world_pos(target.coord);
    let aabb_max = aabb_min + Vec3::splat(crate::voxel::VOXEL_SIZE);
    let closest_point = player_eye.clamp(aabb_min, aabb_max);
    let distance = (closest_point - player_eye).length();
    if distance > max_reach {
        return Err(InteractionError::ExceedsReach {
            distance,
            max_reach,
        });
    }

    // 2. Validasi residency chunk
    let (chunk_coord, _) = world_voxel_to_chunk_and_local(target.coord);
    if !world.store.is_chunk_resident(&chunk_coord) {
        return Err(InteractionError::TargetNotResident {
            coord: target.coord,
        });
    }

    // 3. Validasi isi voxel
    let voxel = world.store.get_voxel_world_checked(target.coord).ok_or(
        InteractionError::TargetNotResident {
            coord: target.coord,
        },
    )?;

    if voxel.is_air() {
        return Err(InteractionError::TargetIsAir {
            coord: target.coord,
        });
    }

    // 4. Resolusi definisi interactable
    let def = world
        .interactables
        .resolve_definition(&target.interactable_id)
        .ok_or_else(|| InteractionError::UnknownInteractable {
            id: target.interactable_id.clone(),
        })?;

    // Periksa apakah aksi diizinkan oleh definisi konten
    if !def.allowed_actions.contains(&action) {
        return Err(InteractionError::ActionNotAllowed {
            action,
            interactable_id: target.interactable_id.clone(),
        });
    }

    // 5. Evaluasi transisi status
    let (target_state, feedback) = evaluate_transition(target.current_state, action, def)?;

    Ok(InteractionProposal {
        interactable_id: target.interactable_id.clone(),
        coord: target.coord,
        expected_material: target.expected_material,
        action,
        previous_state: target.current_state,
        target_state,
        feedback,
    })
}

/// Mengeksekusi proposal interaksi secara atomik setelah revalidasi TOCTOU akhir.
///
/// MANDATORY TOCTOU REVALIDATION:
/// Proposal hanya valid jika KETIGA kondisi ini terpenuhi persis:
/// 1. `current_interactable_id == proposal.interactable_id`
/// 2. `current_material == proposal.expected_material`
/// 3. `current_state == proposal.previous_state`
///
/// Jika SALAH SATU gagal:
/// - Eksekusi gagal deterministik
/// - Status instance TIDAK bermutasi
/// - Cooldown TIDAK dikonsumsi
/// - Tidak ada mutasi voxel CSG atau dunia
pub fn execute_interaction(
    world: &mut World,
    proposal: &InteractionProposal,
) -> Result<InteractionResult, InteractionError> {
    // 1. Validasi residency chunk
    let (chunk_coord, _) = world_voxel_to_chunk_and_local(proposal.coord);
    if !world.store.is_chunk_resident(&chunk_coord) {
        return Err(InteractionError::TargetNotResident {
            coord: proposal.coord,
        });
    }

    // 2. Validasi voxel
    let voxel = world.store.get_voxel_world_checked(proposal.coord).ok_or(
        InteractionError::TargetNotResident {
            coord: proposal.coord,
        },
    )?;

    if voxel.is_air() {
        return Err(InteractionError::TargetIsAir {
            coord: proposal.coord,
        });
    }

    // 3. Resolusi identitas interactable aktual
    let current_id = world
        .interactables
        .get_interactable_at(&proposal.coord)
        .cloned();

    // TOCTOU CHECK 1: Validasi identitas interactable
    match current_id {
        Some(ref id) if id == &proposal.interactable_id => {
            // Cocok
        }
        other => {
            return Err(InteractionError::InteractableMismatch {
                expected: proposal.interactable_id.clone(),
                actual: other,
            });
        }
    }

    // TOCTOU CHECK 2: Validasi material fisik aktual
    if voxel.material != proposal.expected_material {
        return Err(InteractionError::MaterialMismatch {
            expected: proposal.expected_material,
            actual: voxel.material,
        });
    }

    // Resolusi definisi untuk menentukan status awal jika instansi belum ada
    let def = world
        .interactables
        .resolve_definition(&proposal.interactable_id)
        .ok_or_else(|| InteractionError::UnknownInteractable {
            id: proposal.interactable_id.clone(),
        })?;

    // Evaluasi status aktual saat ini
    let current_state = if let Some(instance) = world.interactables.instances.get(&proposal.coord) {
        if instance.interactable_id == proposal.interactable_id
            && instance.expected_material == voxel.material
        {
            instance.state
        } else {
            def.initial_state
        }
    } else {
        def.initial_state
    };

    // TOCTOU CHECK 3: Validasi status awal sebelum komit
    if current_state != proposal.previous_state {
        return Err(InteractionError::StateMismatch {
            expected: proposal.previous_state,
            actual: current_state,
        });
    }

    // KETIGA PENGECEKAN TOCTOU LOLOS: Komit mutasi status instance semantik secara atomik
    world.interactables.instances.insert(
        proposal.coord,
        InteractableInstance {
            interactable_id: proposal.interactable_id.clone(),
            expected_material: proposal.expected_material,
            state: proposal.target_state,
        },
    );

    Ok(InteractionResult {
        interactable_id: proposal.interactable_id.clone(),
        coord: proposal.coord,
        action: proposal.action,
        previous_state: proposal.previous_state,
        new_state: proposal.target_state,
        feedback: proposal.feedback,
    })
}

/// Menangani orkestrasi lengkap interaksi generik pemain.
///
/// KEPEMILIKAN COOLDOWN TUNGGAL:
/// `handle_player_generic_interaction` adalah satu-satunya pemilik pemeriksaan dan pemicu cooldown:
/// 1. `cooldown.can_act()` gate awal
/// 2. `query_interactable_target`
/// 3. Resolusi aksi
/// 4. `validate_interaction`
/// 5. `execute_interaction`
/// 6. `cooldown.trigger()` strictly POST-COMMIT setelah eksekusi sukses
/// 7. Mengembalikan hasil
pub fn handle_player_generic_interaction(
    world: &mut World,
    player: &PlayerController,
    look_direction: Vec3,
    action: Option<InteractableAction>,
    cooldown: &mut InteractionCooldown,
) -> Result<InteractionResult, InteractionError> {
    // 1. Periksa cooldown debounce
    if !cooldown.can_act() {
        return Err(InteractionError::CooldownActive {
            remaining: cooldown.timer,
        });
    }

    // 2. Kueri target interaksi (murni read-only)
    let target = query_interactable_target(world, player, look_direction)?;

    // 3. Tentukan aksi (permintaan eksplisit atau aksi preferensi)
    let chosen_action =
        action
            .or(target.preferred_action)
            .ok_or(InteractionError::InvalidActionForState {
                action: InteractableAction::Activate,
                state: target.current_state,
            })?;

    // 4. Validasi intensi dan bangun proposal (murni read-only)
    let proposal = validate_interaction(
        world,
        &target,
        chosen_action,
        player.eye_position(),
        player.config.interaction_reach,
    )?;

    // 5. Eksekusi proposal secara atomik dengan revalidasi TOCTOU akhir
    let result = execute_interaction(world, &proposal)?;

    // 6. Konsumsi cooldown HANYA SETELAH EKSEKUSI BERHASIL
    cooldown.trigger();

    // 7. Kembalikan hasil
    Ok(result)
}
