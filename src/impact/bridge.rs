use std::collections::HashMap;

use glam::{IVec3, Vec3};

use crate::chunk::dirty_flags;
use crate::coord::{world_voxel_to_chunk_and_local, world_voxel_to_world_pos};
use crate::csg::transaction::VoxelEditCommitResult;
use crate::impact::event::ImpactEvent;
use crate::material::MaterialRegistry;
use crate::physics::aggregate::{
    AggregateColliderStrategy, AggregatePhysicsError, OrientationQuantizationPolicy,
};
use crate::physics::body::DynamicBodyId;
use crate::physics::world::PhysicsWorld;
use crate::streaming::store::ChunkStore;
use crate::structure::aggregate::DetachedAggregate;
use crate::structure::connectivity::{check_structural_connectivity, ConnectivityStatus};
use crate::structure::manager::{StructuralSystem, StructuralTransactionSnapshot};
use crate::voxel::{VoxelBlock, VOXEL_SIZE};

/// Snapshot kondisi pra-transaksi untuk sebuah chunk yang terdampak mutasi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPreState {
    pub chunk_coord: IVec3,
    pub dirty_flags: u16,
    pub revision: u64,
}

/// Jurnal mutasi transaksional untuk menjamin atomisitas Fase A (Whole-Impact Atomicity).
#[derive(Debug, Default)]
pub struct ImpactTransactionJournal {
    /// Seluruh koordinat dan tipe blok awal voxel yang dikosongkan dari ChunkStore
    pub cleared_voxels: Vec<(IVec3, VoxelBlock)>,
    /// State awal dirty flags dan revision untuk setiap chunk yang tersentuh
    pub chunk_pre_states: HashMap<IVec3, ChunkPreState>,
    /// Daftar ID badan dinamis yang telah dibuat di PhysicsWorld pada Fase A
    pub physicalized_bodies: Vec<DynamicBodyId>,
}

impl ImpactTransactionJournal {
    /// Mencatat state awal suatu chunk jika belum tercatat di jurnal
    pub fn record_chunk_pre_state(&mut self, store: &ChunkStore, chunk_coord: IVec3) {
        if self.chunk_pre_states.contains_key(&chunk_coord) {
            return;
        }
        if let Some(chunk) = store.get(&chunk_coord) {
            self.chunk_pre_states.insert(
                chunk_coord,
                ChunkPreState {
                    chunk_coord,
                    dirty_flags: chunk.dirty_flags,
                    revision: chunk.revision,
                },
            );
        }
    }
}

/// Hasil dari rekonsiliasi struktural dan physicalization benturan (Phase 10.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ImpactIntegrationResult {
    /// ID DynamicBody baru untuk gugusan yang terlepas
    pub detached_bodies: Vec<DynamicBodyId>,
    /// ID persisten aggregate struktural yang diekstraksi
    pub detached_aggregate_ids: Vec<u64>,
    /// Total voxel yang dialihkan dari static world ke dynamic simulation
    pub total_voxels_detached: usize,
    /// Besaran impuls yang berhasil diterapkan dalam Newton-detik (None jika energi murni atau arah degenerasi)
    pub impulse_applied: Option<f32>,
}

/// Kesalahan yang terjadi selama Fase A (Ownership Transaction).
#[derive(Debug, Clone, PartialEq)]
pub enum ImpactIntegrationError {
    /// Kegagalan pembuatan atau registrasi RigidBody / Collider di PhysicsWorld
    PhysicalizationFailed(AggregatePhysicsError),
    /// Komponen unanchored menyentuh batas chunk yang belum dimuat (PendingUnloadedNeighbor)
    UnloadedChunk(IVec3),
}

impl std::fmt::Display for ImpactIntegrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PhysicalizationFailed(err) => {
                write!(f, "Physicalization failed: {}", err)
            }
            Self::UnloadedChunk(coord) => {
                write!(f, "Detachment touched unloaded chunk at {:?}", coord)
            }
        }
    }
}

impl std::error::Error for ImpactIntegrationError {}

/// Jembatan integrasi otoritatif antara ImpactEvent, mutasi CSG, sistem struktural, dan dunia fisika.
pub struct ImpactBridge;

impl ImpactBridge {
    /// Merekonsiliasi perubahan struktural pasca-commit CSG dan melakukan physicalization transaksional.
    ///
    /// INVARIAN INTI:
    /// 1. FASE A (Ownership Transaction): Whole-Impact Atomicity.
    ///    Jika ada satu kegagalan physicalization atau batas unloaded chunk, SELURUH mutasi Fase A di-rollback.
    ///    Voxel dikembalikan, dirty_flags dan revision dipulihkan persis sama, PhysicsWorld dibersihkan.
    /// 2. FASE B (Impulse Response): Pasca-komit kepemilikan.
    ///    Impuls diterapkan langsung pada RigidBody di titik kontak voxel terdekat yang di-clamp ke AABB.
    ///    Kegagalan pada Fase B bersifat non-fatal dan tidak membatalkan transfer kepemilikan voxel.
    pub fn reconcile_and_physicalize(
        impact: &ImpactEvent,
        commit_result: &VoxelEditCommitResult,
        store: &mut ChunkStore,
        structural_sys: &mut StructuralSystem,
        physics_world: &mut PhysicsWorld,
        materials: Option<&MaterialRegistry>,
        collider_strategy: AggregateColliderStrategy,
    ) -> Result<ImpactIntegrationResult, ImpactIntegrationError> {
        let structural_snapshot = structural_sys.create_transaction_snapshot();
        let mut journal = ImpactTransactionJournal::default();

        // 1. Eksekusi FASE A: Transaksi Kepemilikan (Whole-Impact Atomicity)
        let phase_a_result = Self::execute_phase_a(
            commit_result,
            store,
            structural_sys,
            physics_world,
            materials,
            collider_strategy,
            &mut journal,
        );

        let (extracted_aggregates, physicalized_bodies) = match phase_a_result {
            Ok(res) => res,
            Err(err) => {
                // Whole-Impact Rollback
                Self::rollback_phase_a(
                    store,
                    structural_sys,
                    physics_world,
                    &structural_snapshot,
                    &journal,
                );
                return Err(err);
            }
        };

        let total_voxels_detached = journal.cleared_voxels.len();
        let detached_aggregate_ids: Vec<u64> = extracted_aggregates.iter().map(|a| a.id).collect();

        // 2. Eksekusi FASE B: Respon Fisik Impuls Pasca-Komit Kepemilikan
        let impulse_applied = Self::execute_phase_b(
            impact,
            &extracted_aggregates,
            &physicalized_bodies,
            physics_world,
        );

        Ok(ImpactIntegrationResult {
            detached_bodies: physicalized_bodies,
            detached_aggregate_ids,
            total_voxels_detached,
            impulse_applied,
        })
    }

    /// Eksekusi transaksional Fase A: penelusuran BFS lokal, ekstraksi aggregate, pemindahan kepemilikan voxel, dan physicalization.
    fn execute_phase_a(
        commit_result: &VoxelEditCommitResult,
        store: &mut ChunkStore,
        structural_sys: &mut StructuralSystem,
        physics_world: &mut PhysicsWorld,
        materials: Option<&MaterialRegistry>,
        collider_strategy: AggregateColliderStrategy,
        journal: &mut ImpactTransactionJournal,
    ) -> Result<(Vec<DetachedAggregate>, Vec<DynamicBodyId>), ImpactIntegrationError> {
        structural_sys.total_events_processed += commit_result.structural_events.len();

        // Dapatkan kandidat benih hanya dari mutasi yang dapat memutus struktur (can_cause_detachment)
        let candidate_seeds =
            StructuralSystem::collect_candidate_seeds(&commit_result.structural_events, store);

        let mut extracted_aggregates = Vec::new();

        for seed in candidate_seeds {
            let block = store.get_voxel_world(seed);
            if block.is_air() {
                continue;
            }

            structural_sys.total_connectivity_checks += 1;
            let status = check_structural_connectivity(
                seed,
                store,
                &structural_sys.anchor_policy,
                &structural_sys.config,
                Some(&mut structural_sys.total_voxels_inspected),
            );

            match status {
                ConnectivityStatus::ConnectedToAnchor => {}
                ConnectivityStatus::PendingUnloadedNeighbor { unloaded_chunk, .. } => {
                    structural_sys.pending_unloaded_count += 1;
                    structural_sys.pending_checks.push_back(seed);
                    // Jika komponen tak berpenopang menyentuh chunk unloaded, batalkan detachment
                    return Err(ImpactIntegrationError::UnloadedChunk(unloaded_chunk));
                }
                ConnectivityStatus::IndeterminateBudgetExceeded { .. } => {
                    structural_sys.pending_checks.push_back(seed);
                }
                ConnectivityStatus::Detached { component_voxels } => {
                    if component_voxels.is_empty() {
                        continue;
                    }

                    // Urutkan component_voxels secara kanonikal untuk determinisme absolut
                    let mut sorted_voxels = component_voxels;
                    sorted_voxels.sort_by_key(|(pos, _)| (pos.x, pos.y, pos.z));

                    let agg = match DetachedAggregate::from_world_voxels(
                        structural_sys.next_aggregate_id,
                        &sorted_voxels,
                    ) {
                        Some(a) => a,
                        None => continue,
                    };

                    // Catat pre-state chunk sebelum mengosongkan voxel
                    for &(vpos, _) in &sorted_voxels {
                        let (chunk_coord, local) = world_voxel_to_chunk_and_local(vpos);
                        journal.record_chunk_pre_state(store, chunk_coord);

                        // Catat pula tetangga batas chunk yang berpotensi terinvalida mesh
                        if local.x == 0 {
                            journal
                                .record_chunk_pre_state(store, chunk_coord + IVec3::new(-1, 0, 0));
                        } else if local.x == 31 {
                            journal
                                .record_chunk_pre_state(store, chunk_coord + IVec3::new(1, 0, 0));
                        }
                        if local.y == 0 {
                            journal
                                .record_chunk_pre_state(store, chunk_coord + IVec3::new(0, -1, 0));
                        } else if local.y == 31 {
                            journal
                                .record_chunk_pre_state(store, chunk_coord + IVec3::new(0, 1, 0));
                        }
                        if local.z == 0 {
                            journal
                                .record_chunk_pre_state(store, chunk_coord + IVec3::new(0, 0, -1));
                        } else if local.z == 31 {
                            journal
                                .record_chunk_pre_state(store, chunk_coord + IVec3::new(0, 0, 1));
                        }
                    }

                    // Kosongkan voxel dari ChunkStore (beralih ke AIR) dan catat ke jurnal
                    for &(vpos, old_block) in &sorted_voxels {
                        journal.cleared_voxels.push((vpos, old_block));
                        store.set_voxel_world(vpos, VoxelBlock::AIR);

                        // Invalidate mesh pada batas chunk jika diperlukan
                        let (chunk_coord, local) = world_voxel_to_chunk_and_local(vpos);
                        if local.x == 0 {
                            store.mark_dirty(
                                &(chunk_coord + IVec3::new(-1, 0, 0)),
                                dirty_flags::MESH_DIRTY,
                            );
                        } else if local.x == 31 {
                            store.mark_dirty(
                                &(chunk_coord + IVec3::new(1, 0, 0)),
                                dirty_flags::MESH_DIRTY,
                            );
                        }
                        if local.y == 0 {
                            store.mark_dirty(
                                &(chunk_coord + IVec3::new(0, -1, 0)),
                                dirty_flags::MESH_DIRTY,
                            );
                        } else if local.y == 31 {
                            store.mark_dirty(
                                &(chunk_coord + IVec3::new(0, 1, 0)),
                                dirty_flags::MESH_DIRTY,
                            );
                        }
                        if local.z == 0 {
                            store.mark_dirty(
                                &(chunk_coord + IVec3::new(0, 0, -1)),
                                dirty_flags::MESH_DIRTY,
                            );
                        } else if local.z == 31 {
                            store.mark_dirty(
                                &(chunk_coord + IVec3::new(0, 0, 1)),
                                dirty_flags::MESH_DIRTY,
                            );
                        }
                    }

                    structural_sys.next_aggregate_id += 1;
                    structural_sys.total_detached_extracted += 1;
                    structural_sys.detached_aggregates.push(agg.clone());
                    extracted_aggregates.push(agg);
                }
            }
        }

        // Lakukan physicalization untuk seluruh aggregate yang diekstraksi
        let mut physicalized_bodies = Vec::with_capacity(extracted_aggregates.len());
        for agg in &extracted_aggregates {
            match physics_world.physicalize_aggregate(agg.clone(), materials, collider_strategy) {
                Ok(dyn_id) => {
                    journal.physicalized_bodies.push(dyn_id);
                    physicalized_bodies.push(dyn_id);
                }
                Err(err) => {
                    return Err(ImpactIntegrationError::PhysicalizationFailed(err));
                }
            }
        }

        Ok((extracted_aggregates, physicalized_bodies))
    }

    /// Rollback penuh Fase A yang menjamin observabilitas ChunkStore dan StructuralSystem persis sama sebelum transaksi.
    fn rollback_phase_a(
        store: &mut ChunkStore,
        structural_sys: &mut StructuralSystem,
        physics_world: &mut PhysicsWorld,
        structural_snapshot: &StructuralTransactionSnapshot,
        journal: &ImpactTransactionJournal,
    ) {
        // 1. Deregistrasi seluruh DynamicAggregateRecord dan RigidBody dalam urutan terbalik
        for &dyn_id in journal.physicalized_bodies.iter().rev() {
            physics_world.remove_dynamic_aggregate(dyn_id);
        }

        // 2. Pulihkan seluruh voxel asli ke ChunkStore dalam urutan terbalik
        for &(pos, block) in journal.cleared_voxels.iter().rev() {
            store.set_voxel_world(pos, block);
        }

        // 3. Pulihkan dirty_flags dan revision asli untuk seluruh chunk yang terdampak
        for (coord, pre_state) in &journal.chunk_pre_states {
            if let Some(chunk) = store.get_mut(coord) {
                chunk.dirty_flags = pre_state.dirty_flags;
                chunk.revision = pre_state.revision;
            }
        }

        // 4. Pulihkan StructuralSystem ke snapshot persis sebelum transaksi
        structural_sys.restore_transaction_snapshot(structural_snapshot);
    }

    /// Menghitung titik kontak kontinu terdekat pada AABB voxel terdekat aggregate terhadap titik benturan.
    pub fn compute_contact_point(aggregate: &DetachedAggregate, impact_pos: Vec3) -> Vec3 {
        let half_size = Vec3::splat(VOXEL_SIZE * 0.5);

        // Cari voxel center terdekat dengan tie-break kanonikal
        let mut best_voxel: Option<(IVec3, Vec3, f32)> = None;

        for v in &aggregate.voxels {
            let world_pos = aggregate.world_coord_of(v);
            let voxel_center = world_voxel_to_world_pos(world_pos) + half_size;
            let dist_sq = (voxel_center - impact_pos).length_squared();

            match best_voxel {
                None => {
                    best_voxel = Some((world_pos, voxel_center, dist_sq));
                }
                Some((best_pos, _, best_dist_sq)) => {
                    if (dist_sq - best_dist_sq).abs() < 1e-6 {
                        // Tie-break kanonikal (x -> y -> z)
                        if (world_pos.x, world_pos.y, world_pos.z)
                            < (best_pos.x, best_pos.y, best_pos.z)
                        {
                            best_voxel = Some((world_pos, voxel_center, dist_sq));
                        }
                    } else if dist_sq < best_dist_sq {
                        best_voxel = Some((world_pos, voxel_center, dist_sq));
                    }
                }
            }
        }

        if let Some((_, center, _)) = best_voxel {
            // Komponen-wise clamping impact position ke AABB voxel
            let min_bound = center - half_size;
            let max_bound = center + half_size;
            impact_pos.clamp(min_bound, max_bound)
        } else {
            impact_pos
        }
    }

    /// Menentukan arah vektor satuan impuls berdasarkan kontrak prioritas tanpa nilai acak.
    pub fn determine_impulse_direction(impact: &ImpactEvent, contact_point: Vec3) -> Option<Vec3> {
        // Prioritas 1: impact.direction
        if let Some(dir) = impact.direction {
            if dir.is_finite() && dir.length_squared() > 1e-10 {
                return Some(dir.normalize());
            }
        }

        // Prioritas 2: -surface_normal
        if let Some(norm) = impact.surface_normal {
            if norm.is_finite() && norm.length_squared() > 1e-10 {
                return Some((-norm).normalize());
            }
        }

        // Prioritas 3: Vektor radial dari impact position ke contact point
        let radial = contact_point - impact.position;
        if radial.is_finite() && radial.length_squared() >= 1e-10 {
            return Some(radial.normalize());
        }

        // Arah degenerasi -> tidak ada impuls yang dapat diterapkan secara valid
        None
    }

    /// Eksekusi Fase B: Penerapan impuls fisik pasca-physicalization.
    fn execute_phase_b(
        impact: &ImpactEvent,
        extracted_aggregates: &[DetachedAggregate],
        physicalized_bodies: &[DynamicBodyId],
        physics_world: &mut PhysicsWorld,
    ) -> Option<f32> {
        let impulse_mag = match impact.magnitude.impulse() {
            Some(j) if j.is_finite() && j > 0.0 => j,
            _ => return None,
        };

        let mut applied_any = false;

        for (agg, &dyn_id) in extracted_aggregates.iter().zip(physicalized_bodies) {
            let contact_point = Self::compute_contact_point(agg, impact.position);
            let dir = match Self::determine_impulse_direction(impact, contact_point) {
                Some(d) => d,
                None => continue,
            };

            let impulse_vec = dir * impulse_mag;

            if let Some(record) = physics_world.get_dynamic_aggregate(dyn_id) {
                if let Some(rb) = physics_world.get_rigid_body_mut(record.rigid_body_id) {
                    let _ = rb.apply_impulse_at_point(impulse_vec, contact_point);
                    applied_any = true;
                }
            }
        }

        if applied_any {
            Some(impulse_mag)
        } else {
            None
        }
    }

    /// Memeriksa apakah suatu DynamicBody memenuhi seluruh syarat kelayakan untuk reintegrasi statis.
    pub fn is_eligible_for_reintegration(
        body_id: DynamicBodyId,
        physics_world: &PhysicsWorld,
        store: &ChunkStore,
    ) -> bool {
        let record = match physics_world.get_dynamic_aggregate(body_id) {
            Some(r) => r,
            None => return false,
        };
        let rb = match physics_world.get_rigid_body(record.rigid_body_id) {
            Some(b) => b,
            None => return false,
        };

        // 1. Harus dalam status Sleeping
        if !rb.is_sleeping() {
            return false;
        }

        // 2. Kecepatan linier dan angular harus di bawah ambang batas praktis nol
        if rb.linear_velocity().length_squared() > 1e-6
            || rb.angular_velocity().length_squared() > 1e-6
        {
            return false;
        }

        // 3. Harus bertumpu kokoh pada tanah statis (Dynamic-on-dynamic support mengembalikan false)
        if let Some(dyn_body) = record.to_dynamic_body(physics_world) {
            if !crate::physics::collision::is_firmly_supported_by_static_ground(&dyn_body, store) {
                return false;
            }
        } else {
            return false;
        }

        // 4. Seluruh chunk tujuan harus berstatus resident
        for v in &record.aggregate.voxels {
            let world_coord = record.aggregate.world_coord_of(v);
            let (chunk_coord, _) = world_voxel_to_chunk_and_local(world_coord);
            if !store.is_chunk_resident(&chunk_coord) {
                return false;
            }
        }

        true
    }

    /// Memproses reintegrasi untuk seluruh badan dinamis yang telah memenuhi syarat kelayakan.
    pub fn process_settled_reintegration(
        physics_world: &mut PhysicsWorld,
        store: &mut ChunkStore,
        policy: OrientationQuantizationPolicy,
    ) -> Vec<DynamicBodyId> {
        let eligible_ids: Vec<DynamicBodyId> = physics_world
            .dynamic_aggregates
            .keys()
            .copied()
            .filter(|&id| Self::is_eligible_for_reintegration(id, physics_world, store))
            .collect();

        let mut reintegrated = Vec::new();

        for id in eligible_ids {
            if physics_world
                .reintegrate_aggregate(id, store, policy)
                .is_ok()
            {
                reintegrated.push(id);
            }
        }

        reintegrated
    }
}
