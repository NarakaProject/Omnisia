use glam::IVec3;
use std::collections::BTreeMap;

use super::body::{DynamicBody, DynamicBodyId, DynamicBodyState};
use super::config::PhysicsConfig;
use crate::structure::aggregate::DetachedAggregate;

/// Runtime manajemen fisika dan simulasi DynamicBody untuk Phase 8A.
/// Menggunakan `BTreeMap` agar iterasi penanganan simulasi selalu deterministik (Amendment 3).
pub struct PhysicsRuntime {
    pub config: PhysicsConfig,
    pub bodies: BTreeMap<DynamicBodyId, DynamicBody>,
    pub next_body_id: u64,
    pub accumulator: f32,

    // Metrik telemetri runtime fisika (zero-heap)
    pub total_spawned: usize,
    pub total_reintegrated: usize,
    pub physics_ticks_total: u64,
    pub collision_checks_total: usize,
    pub collision_contacts_total: usize,
}

impl Default for PhysicsRuntime {
    fn default() -> Self {
        Self::new(PhysicsConfig::default())
    }
}

impl PhysicsRuntime {
    pub fn new(config: PhysicsConfig) -> Self {
        Self {
            config,
            bodies: BTreeMap::new(),
            next_body_id: 1,
            accumulator: 0.0,
            total_spawned: 0,
            total_reintegrated: 0,
            physics_ticks_total: 0,
            collision_checks_total: 0,
            collision_contacts_total: 0,
        }
    }

    /// Memperbarui simulasi fisika berdasarkan frame render dt dengan loop fixed-timestep 30 Hz.
    /// Mengembalikan jumlah substep fisika yang dieksekusi pada frame ini.
    /// (Amendment 2: Frame-rate independence di bawah cadence normal, bounded catch-up pada lag).
    pub fn update(&mut self, render_dt: f32, store: &crate::streaming::store::ChunkStore) -> usize {
        let clamped_dt = render_dt.min(self.config.max_dt_clamp);
        self.accumulator += clamped_dt;

        let fixed_dt = self.config.fixed_dt;
        let mut ticks = 0;

        while self.accumulator >= fixed_dt && ticks < self.config.max_substeps_per_frame {
            self.tick(fixed_dt, store);
            self.accumulator -= fixed_dt;
            ticks += 1;
        }

        // Mencegah penumpukan tak berbatas jika frame rate jatuh secara patologis
        if ticks >= self.config.max_substeps_per_frame {
            self.accumulator = 0.0;
        }

        ticks
    }

    /// Menjalankan satu tick fisika diskrit berdurasi `dt` detik dengan deteksi tabrakan vertikal sejati
    pub fn tick(&mut self, dt: f32, store: &crate::streaming::store::ChunkStore) {
        self.physics_ticks_total += 1;
        let gravity = self.config.world_gravity;

        for body in self.bodies.values_mut() {
            if body.state == DynamicBodyState::Settled {
                continue;
            }

            // Jika berstatus Sleeping, periksa apakah ada dorongan/kecepatan yang membangunkannya
            if body.state == DynamicBodyState::Sleeping {
                if body.velocity.length() >= self.config.sleep_velocity_threshold {
                    body.set_state(DynamicBodyState::Active);
                } else {
                    continue;
                }
            }

            // 1. Terapkan akselerasi gravitasi
            body.apply_gravity(gravity, dt);

            // 2. Deteksi tabrakan swept horizontal di sumbu X dan Z (8C.3)
            let cand_delta_x = body.velocity.x * dt;
            let cand_delta_z = body.velocity.z * dt;
            if cand_delta_x.abs() > 1e-6 || cand_delta_z.abs() > 1e-6 {
                self.collision_checks_total += 1;
                let horiz_res = super::collision::swept_horizontal_step(
                    body,
                    cand_delta_x,
                    cand_delta_z,
                    store,
                );
                body.position.x = horiz_res.clamped_pos.x;
                body.position.z = horiz_res.clamped_pos.z;
                if horiz_res.hit_x {
                    self.collision_contacts_total += 1;
                    body.velocity.x = 0.0;
                }
                if horiz_res.hit_z {
                    self.collision_contacts_total += 1;
                    body.velocity.z = 0.0;
                }
            }

            // 3. Deteksi tabrakan swept vertikal (Amendment 4 & 6)
            let cand_delta_y = body.velocity.y * dt;
            self.collision_checks_total += 1;

            match super::collision::swept_vertical_step(body, cand_delta_y, store) {
                super::collision::VerticalCollisionResult::Clear { target_pos } => {
                    body.position = target_pos;
                    body.is_grounded = false;
                }
                super::collision::VerticalCollisionResult::GroundContact {
                    clamped_pos, ..
                } => {
                    self.collision_contacts_total += 1;
                    body.position = clamped_pos;
                    body.velocity.y = 0.0;
                    body.is_grounded = true;
                }
                super::collision::VerticalCollisionResult::CeilingContact {
                    clamped_pos, ..
                } => {
                    self.collision_contacts_total += 1;
                    body.position = clamped_pos;
                    body.velocity.y = 0.0;
                }
                super::collision::VerticalCollisionResult::BlockedByUnloaded { clamped_pos } => {
                    body.position = clamped_pos;
                    body.velocity.y = 0.0;
                }
            }

            // 3. Evaluasi deteksi status Sleeping dan Settled (Amendment 13)
            super::collision::update_body_sleep_and_settle(body, &self.config, store);
        }
    }

    /// Membangunkan badan yang sedang Sleeping atau Settled jika tumpuan tanah statis di bawahnya telah hancur (8C.4 & Section 21).
    pub fn handle_static_terrain_mutation(&mut self, store: &crate::streaming::store::ChunkStore) {
        for body in self.bodies.values_mut() {
            if (body.state == DynamicBodyState::Sleeping || body.state == DynamicBodyState::Settled)
                && !super::collision::is_firmly_supported_by_static_ground(body, store)
            {
                body.set_state(DynamicBodyState::Active);
                body.is_grounded = false;
            }
        }
    }

    /// Mendaftarkan DetachedAggregate ke dalam runtime fisika sebagai DynamicBody baru.
    /// Menggunakan move semantics untuk aggregate.
    pub fn spawn_from_detached_aggregate(&mut self, aggregate: DetachedAggregate) -> DynamicBodyId {
        let id = DynamicBodyId(self.next_body_id);
        self.next_body_id += 1;
        self.total_spawned += 1;

        let body = DynamicBody::from_detached_aggregate(id, aggregate);
        self.bodies.insert(id, body);
        id
    }

    /// Mengambil referensi ke DynamicBody berdasarkan ID
    #[inline(always)]
    pub fn get_body(&self, id: DynamicBodyId) -> Option<&DynamicBody> {
        self.bodies.get(&id)
    }

    /// Mengambil referensi mutable ke DynamicBody berdasarkan ID
    #[inline(always)]
    pub fn get_body_mut(&mut self, id: DynamicBodyId) -> Option<&mut DynamicBody> {
        self.bodies.get_mut(&id)
    }

    /// Apakah runtime memegang DynamicBody dengan ID tertentu
    #[inline(always)]
    pub fn contains_body(&self, id: DynamicBodyId) -> bool {
        self.bodies.contains_key(&id)
    }

    /// Jumlah total DynamicBody yang terdaftar
    #[inline(always)]
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Jumlah badan yang berstatus Active
    pub fn active_body_count(&self) -> usize {
        self.bodies
            .values()
            .filter(|b| b.state == DynamicBodyState::Active)
            .count()
    }

    /// Jumlah badan yang berstatus Sleeping
    pub fn sleeping_body_count(&self) -> usize {
        self.bodies
            .values()
            .filter(|b| b.state == DynamicBodyState::Sleeping)
            .count()
    }

    /// Jumlah badan yang berstatus Settled
    pub fn settled_body_count(&self) -> usize {
        self.bodies
            .values()
            .filter(|b| b.state == DynamicBodyState::Settled)
            .count()
    }

    /// Jumlah total voxel yang sedang dimiliki oleh seluruh DynamicBody
    pub fn total_dynamic_voxels(&self) -> usize {
        self.bodies.values().map(|b| b.voxel_count()).sum()
    }

    /// Fase 2: COMMIT Reintegration (Amendment 7)
    /// Menulis seluruh voxel secara atomik ke ChunkStore, menandai MESH_DIRTY & SAVE_DIRTY,
    /// dan menghapus DynamicBody dari runtime dinamis.
    pub fn commit_reintegration(
        &mut self,
        plan: super::reintegrate::ReintegrationPlan,
        store: &mut crate::streaming::store::ChunkStore,
    ) {
        use crate::chunk::dirty_flags;

        // 1. Tulis voxel ke ChunkStore dan propagasikan dirty flag ke tetangga perbatasan (Section 29)
        for &(pos, block) in &plan.voxels {
            store.set_voxel_world(pos, block);

            let (chunk_coord, local) = crate::coord::world_voxel_to_chunk_and_local(pos);
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

        // 2. Tandai seluruh chunk penerima kotor untuk voxel, mesh, dan persistence
        for chunk_coord in plan.affected_chunks {
            store.mark_dirty(
                &chunk_coord,
                dirty_flags::VOXEL_DIRTY | dirty_flags::MESH_DIRTY | dirty_flags::SAVE_DIRTY,
            );
        }

        // 3. Rilis kepemilikan DynamicBody (sekarang 100% dimiliki secara statis oleh ChunkStore)
        self.bodies.remove(&plan.body_id);
        self.total_reintegrated += 1;
    }

    /// Memproses reintegrasi untuk seluruh badan dinamis yang berstatus Settled.
    /// Melakukan validasi dua fase (Prepare -> Validate -> Commit).
    pub fn process_settled_reintegration(
        &mut self,
        store: &mut crate::streaming::store::ChunkStore,
    ) -> Vec<DynamicBodyId> {
        let settled_ids: Vec<DynamicBodyId> = self
            .bodies
            .iter()
            .filter(|(_, b)| b.state == DynamicBodyState::Settled)
            .map(|(&id, _)| id)
            .collect();

        let mut reintegrated = Vec::new();

        for id in settled_ids {
            let plan_res = {
                let body = match self.bodies.get(&id) {
                    Some(b) => b,
                    None => continue,
                };
                body.prepare_reintegration(store)
            };

            match plan_res {
                Ok(plan) => {
                    self.commit_reintegration(plan, store);
                    reintegrated.push(id);
                }
                Err(_err) => {
                    // Konflik atau chunk belum resident: tunda reintegrasi, DynamicBody tetap otoritatif
                }
            }
        }

        reintegrated
    }
}
