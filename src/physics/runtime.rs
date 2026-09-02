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
    pub fn update(&mut self, render_dt: f32) -> usize {
        let clamped_dt = render_dt.min(self.config.max_dt_clamp);
        self.accumulator += clamped_dt;

        let fixed_dt = self.config.fixed_dt;
        let mut ticks = 0;

        while self.accumulator >= fixed_dt && ticks < self.config.max_substeps_per_frame {
            self.tick(fixed_dt);
            self.accumulator -= fixed_dt;
            ticks += 1;
        }

        // Mencegah penumpukan tak berbatas jika frame rate jatuh secara patologis
        if ticks >= self.config.max_substeps_per_frame {
            self.accumulator = 0.0;
        }

        ticks
    }

    /// Menjalankan satu tick fisika diskrit berdurasi `dt` detik
    pub fn tick(&mut self, dt: f32) {
        self.physics_ticks_total += 1;
        let gravity = self.config.world_gravity;

        for body in self.bodies.values_mut() {
            if body.state == DynamicBodyState::Active {
                // 1. Terapkan akselerasi gravitasi
                body.apply_gravity(gravity, dt);

                // 2. Integrasikan perpindahan translasi linier
                body.integrate_motion(dt);
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
}
